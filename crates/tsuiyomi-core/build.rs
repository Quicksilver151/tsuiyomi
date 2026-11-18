use std::path::Path;

/// Embed `runtime/host.zip` when the distribution build has produced it.
///
/// Keeping this optional means a plain `cargo build` works without a 100 MB archive in
/// the tree, while release builds get a genuinely single-file executable.
fn main() {
    println!("cargo::rustc-check-cfg=cfg(embedded_host)");
    println!("cargo::rerun-if-changed=runtime/host.zip");

    let archive = Path::new("runtime/host.zip");
    if archive.is_file() {
        println!("cargo::rustc-cfg=embedded_host");
        println!(
            "cargo::rustc-env=TSUIYOMI_HOST_ARCHIVE={}",
            archive
                .canonicalize()
                .unwrap_or_else(|_| archive.to_path_buf())
                .display()
        );
    }
}
