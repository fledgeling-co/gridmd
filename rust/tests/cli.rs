//! Drives the built `gridmd` binary through every verb and exit path so the CLI
//! surface (main.rs) is exercised end to end.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_gridmd");

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join(rel)
}

fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("gridmd-cli-{}-{name}", std::process::id()));
    p
}

fn run(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(BIN).args(args).output().expect("spawn");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn cli_dump() {
    let fixture = repo("conformance/fixtures/01-cells.gmd");
    let (code, stdout, _) = run(&["dump", fixture.to_str().unwrap()]);
    assert_eq!(code, 0);
    let expected = std::fs::read_to_string(repo("conformance/expected/01-cells.json")).unwrap();
    assert_eq!(stdout, expected);

    // invalid → exit 1 + stderr
    let bad = repo("conformance/invalid/duplicate-cell.gmd");
    let (code, _, stderr) = run(&["dump", bad.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(stderr.contains("error:"));

    // usage (no file) → exit 2
    assert_eq!(run(&["dump"]).0, 2);
    // missing file → exit 2
    assert_eq!(run(&["dump", "/no/such/file.gmd"]).0, 2);
    // unknown verb → exit 2
    assert_eq!(run(&["frobnicate"]).0, 2);
    assert_eq!(run(&[]).0, 2);
}

#[test]
fn cli_to_and_from_xlsx() {
    let fixture = repo("examples/quarterly-report.gmd");
    let xlsx = tmp("qr.xlsx");
    let (code, stdout, _) = run(&["to-xlsx", fixture.to_str().unwrap(), "-o", xlsx.to_str().unwrap()]);
    assert_eq!(code, 0, "to-xlsx should succeed");
    assert!(stdout.contains("native cell"));
    assert!(xlsx.exists());

    let back = tmp("qr.gmd");
    let (code, stdout, _) = run(&["from-xlsx", xlsx.to_str().unwrap(), "-o", back.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(stdout.contains("self-check clean"));

    // dumps must match → round-trip through the CLI
    let d1 = run(&["dump", fixture.to_str().unwrap()]).1;
    let d2 = run(&["dump", back.to_str().unwrap()]).1;
    assert_eq!(d1, d2);

    // to-xlsx on an invalid document → exit 1
    let bad = repo("conformance/invalid/orphan-spill-cache.gmd");
    assert_eq!(run(&["to-xlsx", bad.to_str().unwrap(), "-o", tmp("x.xlsx").to_str().unwrap()]).0, 1);

    // usage + missing-file paths
    assert_eq!(run(&["to-xlsx"]).0, 2);
    assert_eq!(run(&["to-xlsx", "/no/such.gmd"]).0, 2);
    assert_eq!(run(&["from-xlsx"]).0, 2);
    assert_eq!(run(&["from-xlsx", "/no/such.xlsx"]).0, 2);

    // from-xlsx on a non-zip → exit 1
    let junk = tmp("junk.xlsx");
    std::fs::write(&junk, b"not a zip").unwrap();
    assert_eq!(run(&["from-xlsx", junk.to_str().unwrap()]).0, 1);

    // default output path derivation (no -o)
    let copy = tmp("deriv.gmd");
    std::fs::copy(&fixture, &copy).unwrap();
    let (code, _, _) = run(&["to-xlsx", copy.to_str().unwrap()]);
    assert_eq!(code, 0);
    let derived_xlsx = tmp("deriv.xlsx");
    assert!(derived_xlsx.exists());
    let (code, _, _) = run(&["from-xlsx", derived_xlsx.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(tmp("deriv.gmd").exists());
}
