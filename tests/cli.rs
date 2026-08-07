use std::path::Path;
use std::process::Command;

fn knap() -> Command {
    Command::new(env!("CARGO_BIN_EXE_knap"))
}

/// Copies a fixture directory into a fresh `tempfile` dir so a mutating test
/// (rename, etc.) never touches the checked-in fixture.
fn copy_fixture(name: &str) -> tempfile::TempDir {
    let src = Path::new("tests/fixtures").join(name);
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    copy_dir(&src, dir.path());
    dir
}

fn copy_dir(src: &Path, dst: &Path) {
    for entry in std::fs::read_dir(src).expect("failed to read fixture dir") {
        let entry = entry.expect("failed to read fixture entry");
        let dest = dst.join(entry.file_name());
        if entry.file_type().expect("failed to stat entry").is_dir() {
            std::fs::create_dir(&dest).expect("failed to create subdir");
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), &dest).expect("failed to copy fixture file");
        }
    }
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
    assert!(
        stdout.contains("title: Sample Note"),
        "stdout was: {stdout}"
    );
    assert!(stdout.contains("headings: 1"), "stdout was: {stdout}");
}

#[test]
fn check_subcommand_still_works() {
    let output = knap().arg("check").output().expect("failed to run knap");
    assert!(
        output.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("11 passed, 0 failed"),
        "stdout was: {stdout}"
    );
}

#[test]
fn lint_text_output_reports_broken_link() {
    let output = knap()
        .args(["lint", "tests/fixtures/lint_basic"])
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Link target not found: 'missing.md'"),
        "stdout was: {stdout}"
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn lint_json_output_parses_and_matches_shape() {
    let output = knap()
        .args(["lint", "tests/fixtures/lint_basic", "--json"])
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");
    let problem_count = value["problem_count"]
        .as_u64()
        .expect("problem_count present");
    assert!(problem_count > 0);
    assert!(value["diagnostics"].as_array().is_some());
    assert!(value["file_count"].as_u64().is_some());
}

#[test]
fn lint_clean_dir_exits_zero() {
    let output = knap()
        .args(["lint", "tests/fixtures/lint_clean", "--json"])
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");
    assert_eq!(value["problem_count"].as_u64(), Some(0));
    assert_eq!(value["diagnostics"].as_array().map(|a| a.len()), Some(0));
}

#[test]
fn lint_respects_knap_toml_extensions() {
    let output = knap()
        .args(["lint", "tests/fixtures/knap_toml", "--json"])
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");
    assert!(value["problem_count"].as_u64().unwrap_or(0) > 0);
    let paths: Vec<&str> = value["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert!(
        paths.iter().any(|p| p.ends_with("note.knap")),
        "paths were: {paths:?}"
    );
}

#[test]
fn index_json_output_shape() {
    let output = knap()
        .args(["index", "tests/fixtures/index_basic", "--json"])
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
    assert_eq!(notes.len(), 2, "notes were: {notes:?}");

    let alpha = notes
        .iter()
        .find(|n| n["path"].as_str().unwrap().ends_with("alpha.md"))
        .expect("alpha.md present");
    let headings = alpha["headings"].as_array().expect("headings present");
    assert!(
        headings.iter().any(|h| h["text"] == "Alpha"),
        "headings were: {headings:?}"
    );

    let links = alpha["links"].as_array().expect("links present");
    assert!(
        links.iter().any(|l| l["resolved"]
            .as_str()
            .is_some_and(|p| p.ends_with("beta.md"))),
        "links were: {links:?}"
    );

    let tags = value["tags"].as_object().expect("tags present");
    assert!(tags.contains_key("demo"), "tags were: {tags:?}");
    assert_eq!(tags["demo"].as_array().map(|a| a.len()), Some(2));
}

#[test]
fn index_text_output_unchanged_format() {
    let output = knap()
        .args(["index", "tests/fixtures/index_basic"])
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("2 note(s) indexed"), "stdout was: {stdout}");
    assert!(stdout.contains("alpha.md"), "stdout was: {stdout}");
    assert!(stdout.contains("beta.md"), "stdout was: {stdout}");
    assert!(stdout.contains("referenced by:"), "stdout was: {stdout}");
}

#[test]
fn lint_relative_dot_root_does_not_false_positive_on_valid_links() {
    let output = knap()
        .args(["lint", ".", "--json"])
        .current_dir("tests/fixtures/lint_clean")
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
        "stdout was: {stdout}"
    );
}

#[test]
fn index_text_output_resolves_same_file_anchor_link() {
    let output = knap()
        .args(["index", "tests/fixtures/index_anchor"])
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains('→'), "stdout was: {stdout}");
    assert!(!stdout.contains("broken"), "stdout was: {stdout}");
}

// ── rename-file ──────────────────────────────────────────────────────────

#[test]
fn rename_file_updates_incoming_and_outgoing() {
    let dir = copy_fixture("rename_file");

    let output = knap()
        .args(["rename-file", "sub/old.md", "new.md"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(!dir.path().join("sub/old.md").exists());
    let moved = std::fs::read_to_string(dir.path().join("new.md")).unwrap();
    assert!(
        moved.contains("(sibling.md)"),
        "outgoing link not rewritten: {moved}"
    );

    let linker = std::fs::read_to_string(dir.path().join("linker.md")).unwrap();
    assert!(
        linker.contains("(new.md)"),
        "incoming link not rewritten: {linker}"
    );
}

#[test]
fn rename_file_new_path_exists_errors() {
    let dir = copy_fixture("rename_file");
    let before = std::fs::read_to_string(dir.path().join("sub/old.md")).unwrap();

    let output = knap()
        .args(["rename-file", "sub/old.md", "linker.md"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());

    assert!(dir.path().join("sub/old.md").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("sub/old.md")).unwrap(),
        before
    );
}

// ── rename-heading ───────────────────────────────────────────────────────

#[test]
fn rename_heading_updates_same_and_cross_file() {
    let dir = copy_fixture("rename_heading");

    let output = knap()
        .args(["rename-heading", "a.md", "Old Section", "New Section"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let a = std::fs::read_to_string(dir.path().join("a.md")).unwrap();
    assert!(a.contains("# New Section"), "heading not renamed: {a}");
    assert!(
        a.contains("(#new-section)"),
        "same-file anchor not rewritten: {a}"
    );

    let b = std::fs::read_to_string(dir.path().join("b.md")).unwrap();
    assert!(
        b.contains("(a.md#new-section)"),
        "cross-file anchor not rewritten: {b}"
    );
}

#[test]
fn rename_heading_accepts_slug_or_text() {
    for old in ["Old Section", "old-section"] {
        let dir = copy_fixture("rename_heading");
        let output = knap()
            .args(["rename-heading", "a.md", old, "New Section"])
            .current_dir(dir.path())
            .output()
            .expect("failed to run knap");
        assert!(
            output.status.success(),
            "old={old:?} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let a = std::fs::read_to_string(dir.path().join("a.md")).unwrap();
        assert!(a.contains("# New Section"), "old={old:?} content: {a}");
    }
}

#[test]
fn rename_heading_not_found_errors() {
    let dir = copy_fixture("rename_heading");
    let before = std::fs::read_to_string(dir.path().join("a.md")).unwrap();

    let output = knap()
        .args(["rename-heading", "a.md", "No Such Heading", "New Section"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());

    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        before
    );
}

// ── rename-tag ───────────────────────────────────────────────────────────

#[test]
fn rename_tag_updates_all_frontmatter_forms() {
    let dir = copy_fixture("rename_tag");

    let output = knap()
        .args(["rename-tag", "draft", "published"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let a = std::fs::read_to_string(dir.path().join("a.md")).unwrap();
    assert!(a.contains("tags: published"), "bare scalar: {a}");

    let b = std::fs::read_to_string(dir.path().join("b.md")).unwrap();
    assert!(b.contains("tags: [published, rust]"), "inline list: {b}");

    let c = std::fs::read_to_string(dir.path().join("c.md")).unwrap();
    assert!(c.contains("- published"), "block list: {c}");
}

#[test]
fn rename_tag_not_used_errors() {
    let dir = copy_fixture("rename_tag");
    let before = std::fs::read_to_string(dir.path().join("a.md")).unwrap();

    let output = knap()
        .args(["rename-tag", "nonexistent", "published"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());

    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        before
    );
}

#[test]
fn rename_respects_knap_toml_extensions() {
    let dir = copy_fixture("rename_knap_toml");

    let output = knap()
        .args(["rename-tag", "draft", "published"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let note = std::fs::read_to_string(dir.path().join("note.knap")).unwrap();
    assert!(note.contains("tags: published"), "note was: {note}");
}

#[test]
fn lint_malformed_knap_toml_fails_loudly() {
    let output = knap()
        .args(["lint", "tests/fixtures/knap_toml_malformed"])
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("parsing"), "stderr was: {stderr}");
}
