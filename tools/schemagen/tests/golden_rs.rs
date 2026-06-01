//! Golden-file test for the Rust emitter.
//!
//! Run with `UPDATE=1 cargo nextest run -p schemagen` to regenerate.

use std::{fs, path::PathBuf, process::Command};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn rust_output_matches_golden() {
    let golden = manifest_dir().join("tests/fixtures/expected.rs.golden");
    let workspace_root = manifest_dir()
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf();
    let schema_path = workspace_root.join("tools/schema.json");

    let output = Command::new(env!("CARGO_BIN_EXE_schemagen"))
        .args(["--lang", "rust", "--in"])
        .arg(&schema_path)
        .output()
        .expect("run schemagen");
    assert!(
        output.status.success(),
        "schemagen exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let got = String::from_utf8(output.stdout).expect("utf-8 stdout");

    if std::env::var("UPDATE").is_ok() {
        if let Some(parent) = golden.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&golden, &got).expect("write golden");
        eprintln!("golden updated: {}", golden.display());
        return;
    }

    let want = fs::read_to_string(&golden).expect("read golden");
    assert_eq!(got, want, "generated Rust does not match golden");
}
