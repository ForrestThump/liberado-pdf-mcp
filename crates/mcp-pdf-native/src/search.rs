use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::UNIX_EPOCH;

use lopdf::Document;
use mcp_pdf_core::{PdfError, PdfResult, SearchResult};
use strsim::normalized_levenshtein;

/// Search for PDF files in a directory, optionally filtering by filename pattern.
///
/// If `recursive` is true, searches all subdirectories.
/// If `pattern` is non-empty, only files whose name contains `pattern`
/// (case-insensitive substring match) are returned.
pub fn search_pdfs(base_path: &str, pattern: &str, recursive: bool) -> PdfResult<Vec<SearchResult>> {
    let base = Path::new(base_path);
    if !base.is_dir() {
        return Err(PdfError::FileNotFound(base.to_path_buf()));
    }

    let glob_pattern = if recursive {
        let mut p = base.to_path_buf();
        p.push("**");
        p.push("*.pdf");
        p.to_string_lossy().replace('\\', "/")
    } else {
        let mut p = base.to_path_buf();
        p.push("*.pdf");
        p.to_string_lossy().replace('\\', "/")
    };

    let mut results = Vec::new();

    let entries = glob::glob(&glob_pattern)
        .map_err(|e| PdfError::Other(format!("Glob pattern error: {e}")))?;

    let pattern_lower = pattern.to_lowercase();
    let filter_by_pattern = !pattern.is_empty();

    for entry in entries {
        match entry {
            Ok(path) => {
                if filter_by_pattern {
                    let filename = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
                    if !filename.to_lowercase().contains(&pattern_lower) {
                        continue;
                    }
                }

                let metadata = match std::fs::metadata(&path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let size_bytes = metadata.len();
                let modified = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                results.push(SearchResult {
                    path: path.to_string_lossy().to_string(),
                    size_bytes,
                    modified: modified.to_string(),
                });
            }
            Err(_) => continue,
        }
    }

    // Sort by path for deterministic output
    results.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(results)
}

/// Fuzzy-match a list of filenames against a pattern using normalized Levenshtein distance.
///
/// Returns a list of `(filename, similarity_score)` pairs sorted by
/// descending similarity (higher = more similar).
pub fn fuzzy_match_pdfs(file_list: Vec<String>, pattern: &str) -> Vec<(String, f64)> {
    let mut scored: Vec<(String, f64)> = file_list
        .into_iter()
        .map(|f| {
            let filename = std::path::Path::new(&f)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&f);
            let score = normalized_levenshtein(filename, pattern);
            (f, score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

/// Find PDFs related to a target PDF by analyzing shared word content.
///
/// Reads the target PDF, extracts its text, and identifies significant words
/// (length >= 4, occurrence >= min_occurrences). Then searches for all PDFs
/// in `base_path` recursively and scores each one by what fraction of the
/// target's significant words appear in its text.
///
/// Returns PDFs sorted by relevance score (descending), only including those
/// with score > 0.
pub async fn find_related_pdfs(
    base_path: &str,
    target_filename: &str,
    min_occurrences: u32,
) -> PdfResult<Vec<SearchResult>> {
    // Resolve the target path
    let target_path = Path::new(target_filename);
    let target_path = if target_path.is_absolute() {
        target_path.to_path_buf()
    } else {
        Path::new(base_path).join(target_filename)
    };

    // Read and extract text from the target PDF
    let target_bytes = tokio::fs::read(&target_path)
        .await
        .map_err(PdfError::Io)?;
    let target_doc = Document::load_mem(&target_bytes)
        .map_err(|e| PdfError::Parse(format!("Failed to parse target PDF: {e}")))?;

    let total_target_pages = target_doc.get_pages().len() as u32;
    let target_text = if total_target_pages > 0 {
        let page_nums: Vec<u32> = (1..=total_target_pages).collect();
        target_doc
            .extract_text(&page_nums)
            .unwrap_or_default()
    } else {
        String::new()
    };

    // Tokenize and count words (only those with length >= 4)
    let target_words: Vec<String> = target_text
        .split_whitespace()
        .filter(|w| w.len() >= 4)
        .map(|w| w.to_lowercase())
        .collect();

    let mut word_counts: HashMap<String, u32> = HashMap::new();
    for w in target_words {
        *word_counts.entry(w).or_insert(0) += 1;
    }

    // Keep only words that appear at least min_occurrences times
    let significant_words: Vec<String> = word_counts
        .into_iter()
        .filter(|(_, count)| *count >= min_occurrences)
        .map(|(word, _)| word)
        .collect();

    let total_significant = significant_words.len() as f64;
    if total_significant == 0.0 {
        return Ok(Vec::new());
    }

    // Find all PDFs in base_path
    let all_pdfs = search_pdfs(base_path, "", true)?;

    // For each PDF, compute a relevance score
    let mut scored: Vec<(SearchResult, f64)> = Vec::new();

    for pdf in all_pdfs {
        let pdf_path = &pdf.path;

        // Skip the target itself
        let pdf_path_buf = Path::new(pdf_path);
        if same_file(&target_path, pdf_path_buf) {
            continue;
        }

        // Read and extract text
        let pdf_bytes = match tokio::fs::read(pdf_path_buf).await {
            Ok(b) => b,
            Err(_) => continue,
        };
        let pdf_doc = match Document::load_mem(&pdf_bytes) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let pdf_page_count = pdf_doc.get_pages().len() as u32;
        let pdf_text = if pdf_page_count > 0 {
            let page_nums: Vec<u32> = (1..=pdf_page_count).collect();
            pdf_doc.extract_text(&page_nums).unwrap_or_default()
        } else {
            String::new()
        };

        let pdf_words: HashSet<String> = pdf_text
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .collect();

        // Count how many significant words from target appear in this PDF
        let matching_count = significant_words
            .iter()
            .filter(|sw| pdf_words.contains(*sw))
            .count() as f64;

        let score = if total_significant > 0.0 {
            matching_count / total_significant
        } else {
            0.0
        };

        if score > 0.0 {
            scored.push((pdf, score));
        }
    }

    // Sort by score descending
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    Ok(scored.into_iter().map(|(r, _)| r).collect())
}

/// Check if two paths refer to the same file by canonicalizing them.
fn same_file(a: &Path, b: &Path) -> bool {
    // Fast path: compare raw paths first
    if a == b {
        return true;
    }
    // Canonicalize for symlinks, case-insensitive filesystems, etc.
    if let (Ok(a_canon), Ok(b_canon)) = (a.canonicalize(), b.canonicalize()) {
        return a_canon == b_canon;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{minimal_pdf_bytes, pdf_with_extractable_text};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempDir {
        path: std::path::PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let count = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir().join(format!("pdf_mcp_test_{}", count));
            fs::create_dir_all(&path).expect("Failed to create temp dir");
            TempDir { path }
        }

        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    // --- search_pdfs tests ---

    #[test]
    fn test_search_pdfs_finds_files() {
        let dir = TempDir::new();
        let path = dir.path().to_string_lossy().to_string();

        // Create a couple of PDF files
        let data = minimal_pdf_bytes();
        fs::write(dir.path().join("doc1.pdf"), &data).unwrap();
        fs::write(dir.path().join("doc2.pdf"), &data).unwrap();
        fs::write(dir.path().join("readme.txt"), b"not a pdf").unwrap();

        let results = search_pdfs(&path, "", false).unwrap();
        assert_eq!(results.len(), 2, "Should find exactly 2 PDFs");
        assert!(
            results.iter().any(|r| r.path.contains("doc1.pdf")),
            "Should contain doc1.pdf"
        );
        assert!(
            results.iter().any(|r| r.path.contains("doc2.pdf")),
            "Should contain doc2.pdf"
        );
    }

    #[test]
    fn test_search_pdfs_with_pattern() {
        let dir = TempDir::new();
        let path = dir.path().to_string_lossy().to_string();

        let data = minimal_pdf_bytes();
        fs::write(dir.path().join("report_q1.pdf"), &data).unwrap();
        fs::write(dir.path().join("report_q2.pdf"), &data).unwrap();
        fs::write(dir.path().join("invoice.pdf"), &data).unwrap();

        let results = search_pdfs(&path, "report", false).unwrap();
        assert_eq!(results.len(), 2, "Should find 2 report PDFs");
    }

    #[test]
    fn test_search_pdfs_recursive() {
        let dir = TempDir::new();
        let sub = dir.path().join("subdir");
        fs::create_dir(&sub).unwrap();

        let data = minimal_pdf_bytes();
        fs::write(dir.path().join("root.pdf"), &data).unwrap();
        fs::write(sub.join("nested.pdf"), &data).unwrap();

        // Non-recursive should only find root.pdf
        let non_rec = search_pdfs(
            dir.path().to_string_lossy().as_ref(),
            "",
            false,
        )
        .unwrap();
        assert_eq!(non_rec.len(), 1, "Non-recursive should find 1 PDF");

        // Recursive should find both
        let rec = search_pdfs(
            dir.path().to_string_lossy().as_ref(),
            "",
            true,
        )
        .unwrap();
        assert_eq!(rec.len(), 2, "Recursive should find 2 PDFs");
    }

    #[test]
    fn test_search_pdfs_nonexistent_dir() {
        let result = search_pdfs("C:\\nonexistent_path_12345", "", false);
        assert!(result.is_err());
    }

    // --- fuzzy_match_pdfs tests ---

    #[test]
    fn test_fuzzy_match_pdfs_basic() {
        let files = vec![
            "document.pdf".to_string(),
            "doc.pdf".to_string(),
            "invoice.pdf".to_string(),
            "report.pdf".to_string(),
        ];
        let results = fuzzy_match_pdfs(files, "doc");
        assert!(!results.is_empty());
        // "doc.pdf" should match "doc" best
        assert_eq!(results[0].0, "doc.pdf");
        // "doc" vs "doc.pdf" — Levenshtein distance is 4 (".pdf"), max len 7
        // normalized score ≈ 0.43, which is reasonable for a substring match
        assert!(results[0].1 > 0.3);
    }

    #[test]
    fn test_fuzzy_match_pdfs_empty_list() {
        let files: Vec<String> = vec![];
        let results = fuzzy_match_pdfs(files, "test");
        assert!(results.is_empty());
    }

    #[test]
    fn test_fuzzy_match_pdfs_ordering() {
        let files = vec![
            "aaaa.pdf".to_string(),
            "bbbb.pdf".to_string(),
            "test.pdf".to_string(),
        ];
        let results = fuzzy_match_pdfs(files, "test");
        // "test.pdf" should be first (highest similarity to "test")
        assert_eq!(results[0].0, "test.pdf");
        // Scores should be descending
        for i in 1..results.len() {
            assert!(
                results[i - 1].1 >= results[i].1,
                "Scores should be in descending order"
            );
        }
    }

    // --- find_related_pdfs tests ---

    #[tokio::test]
    async fn test_find_related_pdfs_basic() {
        let dir = TempDir::new();
        let base_path = dir.path().to_string_lossy().to_string();

        // Create target PDF with extractable text
        let target_data = pdf_with_extractable_text("Hello world this is a target document with several significant words hello again");
        let target_path = dir.path().join("target.pdf");
        fs::write(&target_path, &target_data).unwrap();

        // Create a related PDF sharing words with target
        let related_data = pdf_with_extractable_text("This document talks about the target and hello world");
        fs::write(dir.path().join("related.pdf"), &related_data).unwrap();

        // Create an unrelated PDF with different words
        let unrelated_data = pdf_with_extractable_text("Completely different topic nothing in common");
        fs::write(dir.path().join("unrelated.pdf"), &unrelated_data).unwrap();

        let results = find_related_pdfs(
            &base_path,
            &target_path.to_string_lossy(),
            1,
        )
        .await
        .unwrap();

        assert!(
            results.iter().all(|r| !r.path.contains("target.pdf")),
            "Should NOT include target itself"
        );
        // related.pdf shares "hello", "world", "target", "document" — should be found
        assert!(
            results.iter().any(|r| r.path.contains("related.pdf")),
            "Should include related.pdf but got: {results:?}"
        );
    }

    #[tokio::test]
    async fn test_find_related_pdfs_no_matches() {
        let dir = TempDir::new();
        let base_path = dir.path().to_string_lossy().to_string();

        let target_data = pdf_with_extractable_text("unique content nothing else has this");
        let target_path = dir.path().join("target.pdf");
        fs::write(&target_path, &target_data).unwrap();

        // Create a PDF with completely different short words (all < 4 chars)
        let other_data = pdf_with_extractable_text("a b c d e f g h i j k l m n o p");
        fs::write(dir.path().join("other.pdf"), &other_data).unwrap();

        let results = find_related_pdfs(
            &base_path,
            &target_path.to_string_lossy(),
            1,
        )
        .await
        .unwrap();

        // "other.pdf" may not match since all its words are < 4 chars and won't be significant
        // But we should still get valid results (possibly empty)
        assert!(results.is_empty() || results.iter().all(|r| r.path.contains("other.pdf")));
    }

    #[tokio::test]
    async fn test_find_related_pdfs_empty_dir() {
        let dir = TempDir::new();
        let base_path = dir.path().to_string_lossy().to_string();

        // Create a target PDF
        let target_data = pdf_with_extractable_text("test data with content");
        let target_path = dir.path().join("target.pdf");
        fs::write(&target_path, &target_data).unwrap();

        // Remove the only PDF and call find_related (will still find target itself and skip it)
        // Actually let's make a subdir or another case:
        // Just test with only the target in the directory
        let results = find_related_pdfs(
            &base_path,
            &target_path.to_string_lossy(),
            1,
        )
        .await
        .unwrap();
        // Should be empty since only target exists and it skips itself
        assert!(results.is_empty());
    }
}
