// Bind exported fixture provenance to this executable's actual producing checkout.
use std::{env, path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let result = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    result
        .status
        .success()
        .then(|| String::from_utf8_lossy(&result.stdout).trim().to_owned())
}
fn main() {
    let root =
        std::path::PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../../..");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
    for path in [
        "../core/src",
        "../core/data",
        "Cargo.toml",
        "../core/Cargo.toml",
        "../../Cargo.toml",
        "../../Cargo.lock",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    for name in [
        Some("HEAD".to_owned()),
        git(&root, &["symbolic-ref", "--quiet", "HEAD"]),
        Some("packed-refs".to_owned()),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(path) = git(&root, &["rev-parse", "--git-path", &name]) {
            println!("cargo:rerun-if-changed={}", root.join(path).display());
        }
    }
    let commit = git(&root, &["rev-parse", "HEAD"]).unwrap_or_else(|| "unavailable".into());
    let clean = git(&root, &["status", "--porcelain", "--untracked-files=no"])
        .is_some_and(|status| status.is_empty())
        && git(
            &root,
            &[
                "status",
                "--porcelain",
                "--untracked-files=all",
                "--",
                "rust/crates",
            ],
        )
        .is_some_and(|status| status.is_empty());
    println!("cargo:rustc-env=WW2_TERRAIN_COMPILED_COMMIT={commit}");
    println!("cargo:rustc-env=WW2_TERRAIN_COMPILED_CLEAN={clean}");
}
