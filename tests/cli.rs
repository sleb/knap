use std::process::Command;

fn knap() -> Command {
    Command::new(env!("CARGO_BIN_EXE_knap"))
}

#[test]
fn version_subcommand_prints_version() {
    let output = knap().arg("version").output().expect("failed to run knap");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), format!("knap {}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn no_args_exits_nonzero_and_prints_usage() {
    let output = knap().output().expect("failed to run knap");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage: knap"), "stderr was: {stderr}");
}

#[test]
fn parse_subcommand_still_works() {
    let output = knap()
        .args(["parse", "tests/fixtures/parse_basic/note.md"])
        .output()
        .expect("failed to run knap");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("title: Sample Note"), "stdout was: {stdout}");
    assert!(stdout.contains("headings: 1"), "stdout was: {stdout}");
}

#[test]
fn check_subcommand_still_works() {
    let output = knap().arg("check").output().expect("failed to run knap");
    assert!(output.status.success(), "stdout: {}", String::from_utf8_lossy(&output.stdout));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("11 passed, 0 failed"), "stdout was: {stdout}");
}
