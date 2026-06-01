//! Stamp the binary with build-time metadata (version + commit).

use std::process::Command;

fn main() {
    let version = std::env::var("KNOT_VERSION").unwrap_or_else(|_| "dev".to_string());
    let commit = std::env::var("KNOT_COMMIT").unwrap_or_else(|_| {
        Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "unknown".into())
    });
    println!("cargo:rustc-env=KNOT_BUILD_VERSION={version}");
    println!("cargo:rustc-env=KNOT_BUILD_COMMIT={commit}");
    println!("cargo:rerun-if-env-changed=KNOT_VERSION");
    println!("cargo:rerun-if-env-changed=KNOT_COMMIT");
}
