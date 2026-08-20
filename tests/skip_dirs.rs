// v0.20 integration tests — configurable `skip_dirs` (`knap.toml` field)
// over `knap lint`/`knap index` at the CLI process boundary. See
// docs/design/releases/v0.20/configurable-skip-dirs/plan.md Step 4.

use std::process::Command;

fn knap() -> Command {
    Command::new(env!("CARGO_BIN_EXE_knap"))
}

#[test]
fn index_json_indexes_dotted_dir_when_opted_out() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join("knap.toml"), "skip_dirs = []\n").expect("write knap.toml");
    std::fs::create_dir(dir.path().join(".notes")).expect("create .notes dir");
    std::fs::write(dir.path().join(".notes").join("hidden.md"), "# Hidden\n")
        .expect("write .notes/hidden.md");

    let output = knap()
        .args(["index", ".", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");

    let notes = value["notes"].as_array().expect("notes present");
    assert!(
        notes
            .iter()
            .any(|n| n["path"].as_str().unwrap_or("").ends_with("hidden.md")),
        "note under opted-out .notes/ should be indexed: {notes:?}"
    );
}

#[test]
fn lint_default_skip_dirs_prunes_node_modules() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::create_dir(dir.path().join("node_modules")).expect("create node_modules dir");
    std::fs::write(
        dir.path().join("node_modules").join("broken.md"),
        "[broken link](missing.md)\n",
    )
    .expect("write node_modules/broken.md");

    let output = knap()
        .args(["lint", ".", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");
    assert_eq!(
        value["problem_count"].as_u64(),
        Some(0),
        "node_modules/broken.md's broken link should be pruned by the built-in default: {stdout}"
    );
}

#[test]
fn index_custom_skip_dirs_replaces_default() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join("knap.toml"), "skip_dirs = [\"vendor\"]\n")
        .expect("write knap.toml");
    std::fs::create_dir(dir.path().join("vendor")).expect("create vendor dir");
    std::fs::write(dir.path().join("vendor").join("pruned.md"), "# Pruned\n")
        .expect("write vendor/pruned.md");
    std::fs::create_dir(dir.path().join("node_modules")).expect("create node_modules dir");
    std::fs::write(
        dir.path().join("node_modules").join("kept.md"),
        "# Kept\n",
    )
    .expect("write node_modules/kept.md");

    let output = knap()
        .args(["index", ".", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");

    let notes = value["notes"].as_array().expect("notes present");
    assert!(
        notes
            .iter()
            .all(|n| !n["path"].as_str().unwrap_or("").ends_with("pruned.md")),
        "custom skip_dirs should prune vendor/: {notes:?}"
    );
    assert!(
        notes
            .iter()
            .any(|n| n["path"].as_str().unwrap_or("").ends_with("kept.md")),
        "custom skip_dirs replaces (not unions with) the default, so \
         node_modules/ should still be indexed: {notes:?}"
    );
}
