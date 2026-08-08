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

/// Runs `git` with `args` in `dir`, panicking with stderr on failure.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Initializes a git repo in `dir` and commits everything currently there,
/// returning the commit hash.
fn git_init_and_commit_all(dir: &Path) -> String {
    git(dir, &["init"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
    git(dir, &["add", "."]);
    git(dir, &["commit", "-m", "initial"]);
    git(dir, &["rev-parse", "HEAD"])
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
fn lint_json_diagnostics_include_stable_code() {
    let output = knap()
        .args(["lint", "tests/fixtures/lint_basic", "--json"])
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");
    let codes: Vec<&str> = value["diagnostics"]
        .as_array()
        .expect("diagnostics present")
        .iter()
        .flat_map(|f| f["diagnostics"].as_array().expect("diagnostics present"))
        .map(|d| d["code"].as_str().expect("code present"))
        .collect();
    assert!(codes.contains(&"broken-link"), "codes were: {codes:?}");
    assert!(codes.contains(&"broken-anchor"), "codes were: {codes:?}");
}

#[test]
fn lint_fail_on_error_passes_when_only_warnings_present() {
    let output = knap()
        .args(["lint", "tests/fixtures/lint_basic", "--fail-on", "error"])
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn lint_fail_on_warning_matches_default_behavior() {
    let default_output = knap()
        .args(["lint", "tests/fixtures/lint_basic"])
        .output()
        .expect("failed to run knap");
    let explicit_output = knap()
        .args(["lint", "tests/fixtures/lint_basic", "--fail-on", "warning"])
        .output()
        .expect("failed to run knap");
    assert!(!default_output.status.success());
    assert!(!explicit_output.status.success());
    assert_eq!(default_output.status.code(), explicit_output.status.code());
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

#[test]
fn index_file_path_prints_single_note_neighborhood() {
    let output = knap()
        .args(["index", "alpha.md", "--json"])
        .current_dir("tests/fixtures/index_basic")
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");

    // A single `NoteSummary` object, not an `IndexReport` envelope: no
    // `notes` wrapper, but the note's own fields (including `backlinks`,
    // which `IndexReport` doesn't carry) are present at the top level.
    assert!(value["notes"].is_null(), "stdout was: {stdout}");
    assert!(
        value["path"].as_str().unwrap().ends_with("alpha.md"),
        "stdout was: {stdout}"
    );
    let headings = value["headings"].as_array().expect("headings present");
    assert!(
        headings.iter().any(|h| h["text"] == "Alpha"),
        "stdout was: {stdout}"
    );
    let backlinks = value["backlinks"].as_array().expect("backlinks present");
    assert!(
        backlinks
            .iter()
            .any(|p| p.as_str().unwrap().ends_with("beta.md")),
        "stdout was: {stdout}"
    );
}

#[test]
fn index_file_path_includes_backlinks_from_outside_its_directory() {
    let output = knap()
        .args(["index", "sub/target.md", "--json"])
        .current_dir("tests/fixtures/index_scoped")
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");

    let backlinks = value["backlinks"].as_array().expect("backlinks present");
    assert!(
        backlinks
            .iter()
            .any(|p| p.as_str().unwrap().ends_with("linker.md")),
        "backlink from a sibling directory was dropped: {stdout}"
    );
}

#[test]
fn index_unindexed_file_path_errors() {
    let output = knap()
        .args(["index", "ignored.txt", "--json"])
        .current_dir("tests/fixtures/index_basic")
        .output()
        .expect("failed to run knap");
    assert!(
        !output.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
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

#[test]
fn lint_since_scopes_to_git_changed_files() {
    let dir = copy_fixture("lint_clean");
    let commit = git_init_and_commit_all(dir.path());

    // Break a link in note.md only; target.md is untouched.
    let note_path = dir.path().join("note.md");
    let note = std::fs::read_to_string(&note_path).expect("read note.md");
    std::fs::write(&note_path, note.replace("target.md", "missing.md")).expect("write note.md");

    let output = knap()
        .args(["lint", ".", "--json", "--since", &commit])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");
    let paths: Vec<&str> = value["diagnostics"]
        .as_array()
        .expect("diagnostics present")
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths.len(), 1, "paths were: {paths:?}");
    assert!(paths[0].ends_with("note.md"), "paths were: {paths:?}");
}

#[test]
fn lint_since_includes_untracked_new_files() {
    let dir = copy_fixture("lint_clean");
    let commit = git_init_and_commit_all(dir.path());

    // A brand-new, never-committed note with a broken link.
    std::fs::write(
        dir.path().join("new.md"),
        "# New\n\nA [broken link](missing.md).\n",
    )
    .expect("write new.md");

    let output = knap()
        .args(["lint", ".", "--json", "--since", &commit])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");
    let paths: Vec<&str> = value["diagnostics"]
        .as_array()
        .expect("diagnostics present")
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths.len(), 1, "paths were: {paths:?}");
    assert!(paths[0].ends_with("new.md"), "paths were: {paths:?}");
}

#[test]
fn lint_since_no_changes_is_clean() {
    let dir = copy_fixture("lint_clean");
    let commit = git_init_and_commit_all(dir.path());

    let output = knap()
        .args(["lint", ".", "--json", "--since", &commit])
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
    assert_eq!(value["problem_count"].as_u64(), Some(0), "stdout was: {stdout}");
    assert_eq!(value["file_count"].as_u64(), Some(0), "stdout was: {stdout}");
}

#[test]
fn lint_since_outside_git_repo_errors() {
    let dir = copy_fixture("lint_clean");

    let output = knap()
        .args(["lint", ".", "--since", "HEAD"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("git"), "stderr was: {stderr}");
}

#[test]
fn fix_creates_missing_file() {
    let dir = copy_fixture("fix_broken_link");

    let output = knap()
        .args(["fix", "."])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(dir.path().join("missing.md").exists());

    let lint = knap()
        .args(["lint", "."])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        lint.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&lint.stdout)
    );
}

#[test]
fn fix_replaces_unambiguous_broken_anchor() {
    let dir = copy_fixture("fix_unambiguous_anchor");

    let output = knap()
        .args(["fix", "."])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let note = std::fs::read_to_string(dir.path().join("note.md")).unwrap();
    assert!(
        note.contains("target.md#target"),
        "anchor not rewritten: {note}"
    );

    let lint = knap()
        .args(["lint", "."])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        lint.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&lint.stdout)
    );
}

#[test]
fn fix_skips_ambiguous_anchor() {
    let dir = copy_fixture("fix_ambiguous_anchor");
    let before = std::fs::read_to_string(dir.path().join("note.md")).unwrap();

    let output = knap()
        .args(["fix", "."])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = std::fs::read_to_string(dir.path().join("note.md")).unwrap();
    assert_eq!(before, after, "ambiguous anchor should be left alone");

    let lint = knap()
        .args(["lint", "."])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        !lint.status.success(),
        "ambiguous anchor should still be flagged"
    );
}

#[test]
fn fix_dry_run_makes_no_changes() {
    let dir = copy_fixture("fix_broken_link");

    let output = knap()
        .args(["fix", ".", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("would"), "stdout was: {stdout}");

    assert!(!dir.path().join("missing.md").exists());
    let note = std::fs::read_to_string(dir.path().join("note.md")).unwrap();
    assert_eq!(note, "# Note\n\nA [broken link](missing.md) to nowhere.\n");
}
