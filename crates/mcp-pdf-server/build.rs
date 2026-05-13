fn main() {
    if let Err(e) = vergen::EmitBuilder::builder()
        .rustc_semver()
        .cargo_features()
        .cargo_target_triple()
        .git_sha(false)
        .emit()
    {
        eprintln!("Warning: vergen failed: {e}");
    }
}
