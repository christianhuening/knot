//! Golden-file test for the TypeScript emitter.

use std::{fs, path::PathBuf, process::Command};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn ts_output_matches_golden() {
    let golden = manifest_dir().join("tests/fixtures/expected.ts.golden");
    let workspace_root = manifest_dir().ancestors().nth(2).unwrap().to_path_buf();
    let schema = workspace_root.join("tools/schema.json");

    let output = Command::new(env!("CARGO_BIN_EXE_schemagen"))
        .args(["--lang", "ts", "--in"])
        .arg(&schema)
        .output()
        .expect("run schemagen");
    assert!(
        output.status.success(),
        "schemagen exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let got = String::from_utf8(output.stdout).expect("utf-8");

    if std::env::var("UPDATE").is_ok() {
        if let Some(parent) = golden.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&golden, &got).expect("write golden");
        return;
    }
    let want = fs::read_to_string(&golden).expect("read golden");
    assert_eq!(got, want);
}
