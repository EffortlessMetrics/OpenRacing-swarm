#[cfg(target_os = "windows")]
use std::{env, path::PathBuf};

fn main() {
    #[cfg(target_os = "windows")]
    embed_windows_as_invoker_manifest();
}

#[cfg(target_os = "windows")]
fn embed_windows_as_invoker_manifest() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default())
        .join("windows-as-invoker.manifest");

    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
