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
