use std::path::{Path, PathBuf};
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

/// Runs `knap` with `args` in `dir`, piping `stdin` to the process and
/// waiting for it to finish — the plumbing every `apply` test needs to feed
/// a batch in without a temp file of its own.
fn knap_with_stdin(args: &[&str], dir: &Path, stdin: &str) -> std::process::Output {
    use std::io::Write as _;
    use std::process::Stdio;

    let mut child = knap()
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn knap");
    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(stdin.as_bytes())
        .expect("failed to write to knap's stdin");
    child.wait_with_output().expect("failed to run knap")
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

/// Reads every file under `dir` into a path→content map, for asserting a
/// directory is byte-for-byte unchanged after an `apply` run that's supposed
/// to have failed, been a dry-run, or been a no-op.
fn snapshot_dir(dir: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn walk(dir: &Path, root: &Path, out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("failed to read dir") {
            let entry = entry.expect("failed to read entry");
            let path = entry.path();
            if entry.file_type().expect("failed to stat entry").is_dir() {
                walk(&path, root, out);
            } else {
                let rel = path.strip_prefix(root).unwrap().to_path_buf();
                out.insert(rel, std::fs::read(&path).expect("failed to read file"));
            }
        }
    }

    let mut out = std::collections::BTreeMap::new();
    walk(dir, dir, &mut out);
    out
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
fn fix_subcommand_no_longer_exists() {
    let output = knap()
        .args(["fix", "."])
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unrecognized subcommand") && stderr.contains("fix"),
        "stderr was: {stderr}"
    );
}

#[test]
fn lint_fix_flag_no_longer_exists() {
    let output = knap()
        .args(["lint", ".", "--fix"])
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected argument") && stderr.contains("--fix"),
        "stderr was: {stderr}"
    );
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
fn lint_suggest_attaches_ranked_candidates_to_broken_link() {
    let dir = copy_fixture("fix_repoint_broken_link");
    let output = knap()
        .args(["lint", ".", "--json", "--suggest"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");
    let diag = &value["diagnostics"][0]["diagnostics"][0];
    assert_eq!(diag["code"], "broken-link");
    assert_eq!(diag["data"]["suggestions"][0]["target"], "config.md");
}

#[test]
fn lint_without_suggest_omits_data() {
    let dir = copy_fixture("fix_repoint_broken_link");
    let output = knap()
        .args(["lint", ".", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");
    let diag = &value["diagnostics"][0]["diagnostics"][0];
    assert!(diag.get("data").is_none(), "diag was: {diag}");
}

#[test]
fn lint_suggest_reports_text_mismatch_for_decoy_and_correct_candidate() {
    // Trial 4 regression shape: "sync-800.md" is a strong raw path-distance
    // decoy for broken target "sync-830.md", but the link's own text
    // "Sync 835" matches "archive/sync-835.md"'s file stem instead — so the
    // top-combined candidate and the top-text-distance candidate disagree.
    let dir = copy_fixture("fix_text_mismatch_link");

    let output = knap()
        .args(["lint", ".", "--json", "--suggest"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success(), "should still be flagged");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");

    let diag = &value["diagnostics"][0]["diagnostics"][0];
    assert_eq!(diag["code"], "broken-link");
    assert_eq!(diag["data"]["text_mismatch"], serde_json::json!(true));
    let suggestions = diag["data"]["suggestions"]
        .as_array()
        .expect("suggestions present");
    assert!(!suggestions.is_empty());
    for suggestion in suggestions {
        assert!(
            suggestion.get("text_distance").is_some(),
            "expected text_distance on every suggestion: {suggestion:?}"
        );
    }
}

#[test]
fn lint_suggest_prints_candidates_in_text_mode() {
    let dir = copy_fixture("fix_repoint_broken_link");
    let output = knap()
        .args(["lint", ".", "--suggest"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("    -> config.md (distance")),
        "stdout was: {stdout}"
    );
}

#[test]
fn lint_suggest_text_mode_notes_text_mismatch() {
    let dir = copy_fixture("fix_text_mismatch_link");
    let output = knap()
        .args(["lint", ".", "--suggest"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.lines().any(|line| line
            == "    (top match by distance differs from best text match — verify before applying)"),
        "stdout was: {stdout}"
    );
}

#[test]
fn lint_without_suggest_prints_no_candidate_lines() {
    let dir = copy_fixture("fix_repoint_broken_link");
    let output = knap()
        .args(["lint", "."])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.lines().any(|line| line.starts_with("    -> ")),
        "stdout was: {stdout}"
    );
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

#[test]
fn move_file_is_alias_for_rename_file() {
    let dir = copy_fixture("rename_file");

    let output = knap()
        .args(["move-file", "sub/old.md", "new.md"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(!dir.path().join("sub/old.md").exists());
    assert!(dir.path().join("new.md").exists());
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
    assert_eq!(
        value["problem_count"].as_u64(),
        Some(0),
        "stdout was: {stdout}"
    );
    assert_eq!(
        value["file_count"].as_u64(),
        Some(0),
        "stdout was: {stdout}"
    );
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

// ── apply ────────────────────────────────────────────────────────────────

#[test]
fn apply_runs_rename_file_then_rename_heading_in_sequence() {
    let dir = copy_fixture("apply_batch");
    let batch = r#"[
        {"op":"rename-file","old":"sub/note.md","new":"note.md"},
        {"op":"rename-heading","file":"note.md","old":"Old Section","new":"New Section"}
    ]"#;

    let output = knap_with_stdin(&["apply"], dir.path(), batch);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The file-rename ran first, so the heading-rename saw it at its new
    // location — proves later operations see earlier operations' effects.
    assert!(!dir.path().join("sub/note.md").exists());
    let note = std::fs::read_to_string(dir.path().join("note.md")).unwrap();
    assert!(note.contains("# New Section"), "note was: {note}");

    let linker = std::fs::read_to_string(dir.path().join("linker.md")).unwrap();
    assert!(
        linker.contains("(note.md#new-section)"),
        "linker was: {linker}"
    );
}

#[test]
fn apply_all_or_nothing_rolls_back_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.md"), "# A\n").unwrap();
    std::fs::write(dir.path().join("b.md"), "# B\n").unwrap();
    let before = snapshot_dir(dir.path());

    // The second op fails: by the time it runs, the first op has already
    // taken "c.md" in the scratch copy.
    let batch = r#"[
        {"op":"rename-file","old":"a.md","new":"c.md"},
        {"op":"rename-file","old":"b.md","new":"c.md"}
    ]"#;

    let output = knap_with_stdin(&["apply"], dir.path(), batch);
    assert!(!output.status.success());

    assert_eq!(
        snapshot_dir(dir.path()),
        before,
        "real workspace should be byte-for-byte unchanged, including the first op's would-be effect"
    );
}

#[test]
fn apply_dry_run_reports_plan_without_touching_disk() {
    let dir = copy_fixture("apply_batch");
    let before = snapshot_dir(dir.path());
    let batch = r#"[
        {"op":"rename-file","old":"sub/note.md","new":"note.md"},
        {"op":"rename-heading","file":"note.md","old":"Old Section","new":"New Section"}
    ]"#;

    let output = knap_with_stdin(&["apply", "--dry-run"], dir.path(), batch);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("would apply rename-file"),
        "stdout was: {stdout}"
    );
    assert!(
        stdout.contains("would apply rename-heading"),
        "stdout was: {stdout}"
    );

    assert_eq!(
        snapshot_dir(dir.path()),
        before,
        "--dry-run must not touch disk"
    );
}

#[test]
fn apply_json_output_shape() {
    let batch = r#"[
        {"op":"rename-file","old":"sub/note.md","new":"note.md"},
        {"op":"rename-heading","file":"note.md","old":"Old Section","new":"New Section"}
    ]"#;

    let dir = copy_fixture("apply_batch");
    let output = knap_with_stdin(&["apply", "--dry-run", "--json"], dir.path(), batch);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");

    assert_eq!(value["dry_run"], true);
    let operations = value["operations"].as_array().expect("operations present");
    assert_eq!(operations.len(), 2, "operations were: {operations:?}");
    let files_touched = value["files_touched"]
        .as_u64()
        .expect("files_touched present");

    // Same batch, same starting fixture, non-JSON output this time — the
    // total in the summary line must match the JSON report's count.
    let dir2 = copy_fixture("apply_batch");
    let text_output = knap_with_stdin(&["apply", "--dry-run"], dir2.path(), batch);
    assert!(text_output.status.success());
    let text_stdout = String::from_utf8_lossy(&text_output.stdout);
    let total_line = text_stdout.lines().last().expect("summary line present");
    assert!(
        total_line.contains(&format!("{files_touched} file(s)")),
        "summary line was: {total_line}"
    );
}

#[test]
fn apply_invalid_json_input_errors() {
    let dir = copy_fixture("lint_clean");
    let before = snapshot_dir(dir.path());

    let output = knap_with_stdin(&["apply"], dir.path(), "not valid json");
    assert!(!output.status.success());

    assert_eq!(
        snapshot_dir(dir.path()),
        before,
        "fixture dir should be unchanged"
    );
}

#[test]
fn apply_rejects_path_outside_workspace_root() {
    let dir = copy_fixture("lint_clean");
    let before = snapshot_dir(dir.path());
    let outside = tempfile::tempdir().unwrap();
    let outside_path = outside.path().join("ghost.md");

    let batch = format!(
        r#"[{{"op":"rename-file","old":"{}","new":"new.md"}}]"#,
        outside_path.display()
    );

    let output = knap_with_stdin(&["apply"], dir.path(), &batch);
    assert!(!output.status.success());

    assert!(
        !outside_path.exists(),
        "no file should have been created outside the workspace root"
    );
    assert_eq!(
        snapshot_dir(dir.path()),
        before,
        "fixture dir should be unchanged"
    );
}

#[test]
fn apply_empty_batch_is_a_noop() {
    let dir = copy_fixture("lint_clean");
    let before = snapshot_dir(dir.path());

    let output = knap_with_stdin(&["apply"], dir.path(), "[]");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0 operation(s)"), "stdout was: {stdout}");

    assert_eq!(
        snapshot_dir(dir.path()),
        before,
        "fixture dir should be unchanged"
    );
}

#[test]
fn apply_batch_repoints_broken_link_at_diagnostic_range() {
    let dir = copy_fixture("fix_repoint_broken_link");

    let lint = knap()
        .args(["lint", ".", "--json", "--suggest"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    let lint_stdout = String::from_utf8_lossy(&lint.stdout);
    let lint_value: serde_json::Value =
        serde_json::from_str(&lint_stdout).expect("lint stdout was not JSON");
    let file = lint_value["diagnostics"][0]["path"]
        .as_str()
        .expect("diagnostic path present");
    let diag = &lint_value["diagnostics"][0]["diagnostics"][0];
    let range = &diag["range"];
    let target = diag["data"]["suggestions"][0]["target"]
        .as_str()
        .expect("suggestion target present");

    let batch = serde_json::json!([{
        "op": "repoint-link",
        "file": file,
        "range": range,
        "target": target,
    }])
    .to_string();

    let output = knap_with_stdin(&["apply"], dir.path(), &batch);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let note = std::fs::read_to_string(dir.path().join(file)).unwrap();
    assert!(note.contains("(config.md)"), "note was: {note}");

    let relint = knap()
        .args(["lint", "."])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        relint.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&relint.stdout)
    );
}

#[test]
fn apply_batch_repoints_broken_anchor_at_diagnostic_range() {
    let dir = copy_fixture("fix_unambiguous_anchor");

    let lint = knap()
        .args(["lint", ".", "--json", "--suggest"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    let lint_stdout = String::from_utf8_lossy(&lint.stdout);
    let lint_value: serde_json::Value =
        serde_json::from_str(&lint_stdout).expect("lint stdout was not JSON");
    let file = lint_value["diagnostics"][0]["path"]
        .as_str()
        .expect("diagnostic path present");
    let diag = &lint_value["diagnostics"][0]["diagnostics"][0];
    let range = &diag["range"];
    let anchor = diag["data"]["suggestions"][0]["target"]
        .as_str()
        .expect("suggestion target present");

    let batch = serde_json::json!([{
        "op": "repoint-anchor",
        "file": file,
        "range": range,
        "anchor": anchor,
    }])
    .to_string();

    let output = knap_with_stdin(&["apply"], dir.path(), &batch);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let note = std::fs::read_to_string(dir.path().join(file)).unwrap();
    assert!(note.contains("target.md#target"), "note was: {note}");

    let relint = knap()
        .args(["lint", "."])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        relint.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&relint.stdout)
    );
}

#[test]
fn apply_batch_mixes_rename_file_and_repoint_link_atomically() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.md"), "[text](old.md)\n").unwrap();
    std::fs::write(dir.path().join("b.md"), "# B\n").unwrap();
    let before = snapshot_dir(dir.path());

    // The rename-file op succeeds on its own, but the repoint-link op
    // targets a file that doesn't exist — the whole batch must roll back,
    // including the rename, matching
    // `apply_all_or_nothing_rolls_back_on_failure`'s guarantee across
    // operation types, not just within one.
    let batch = r#"[
        {"op":"rename-file","old":"a.md","new":"c.md"},
        {"op":"repoint-link","file":"missing.md","range":{"start":{"line":0,"character":7},"end":{"line":0,"character":13}},"target":"real.md"}
    ]"#;

    let output = knap_with_stdin(&["apply"], dir.path(), batch);
    assert!(!output.status.success());

    assert_eq!(
        snapshot_dir(dir.path()),
        before,
        "real workspace should be byte-for-byte unchanged, including the rename's would-be effect"
    );
}

#[test]
fn apply_repoint_link_rejects_path_outside_workspace_root() {
    let dir = copy_fixture("lint_clean");
    let before = snapshot_dir(dir.path());
    let outside = tempfile::tempdir().unwrap();
    let outside_path = outside.path().join("ghost.md");
    std::fs::write(&outside_path, "[text](old.md)\n").unwrap();

    let batch = format!(
        r#"[{{"op":"repoint-link","file":"{}","range":{{"start":{{"line":0,"character":7}},"end":{{"line":0,"character":13}}}},"target":"real.md"}}]"#,
        outside_path.display()
    );

    let output = knap_with_stdin(&["apply"], dir.path(), &batch);
    assert!(!output.status.success());

    assert_eq!(
        std::fs::read_to_string(&outside_path).unwrap(),
        "[text](old.md)\n",
        "file outside the workspace root should not have been touched"
    );
    assert_eq!(
        snapshot_dir(dir.path()),
        before,
        "fixture dir should be unchanged"
    );
}

#[test]
fn apply_cli_rejects_corrupt_repoint_anchor_and_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.md"), "[text](#old)\n").unwrap();
    let before = snapshot_dir(dir.path());

    // A correct range is 8..11 (just "old"); extending `end` to 12 eats the
    // closing `)` too, mirroring Trial 6's `+1`/`+3` skew.
    let batch = r#"[
        {"op":"repoint-anchor","file":"a.md","range":{"start":{"line":0,"character":8},"end":{"line":0,"character":12}},"anchor":"new"}
    ]"#;

    let output = knap_with_stdin(&["apply"], dir.path(), batch);
    assert!(!output.status.success());

    assert_eq!(
        snapshot_dir(dir.path()),
        before,
        "file on disk should be byte-for-byte unchanged after a rejected batch"
    );
}

// ── skill ────────────────────────────────────────────────────────────────

#[test]
fn skill_path_subcommand_writes_skill_md() {
    let dir = tempfile::tempdir().unwrap();

    let output = knap()
        .args(["skill", "--path"])
        .arg(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let written = std::fs::read_to_string(dir.path().join("SKILL.md")).unwrap();
    let source = std::fs::read_to_string("skill/knap/SKILL.md").unwrap();
    assert_eq!(written, source);
}

#[test]
fn skill_path_subcommand_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();

    let first = knap()
        .args(["skill", "--path"])
        .arg(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_bytes = std::fs::read(dir.path().join("SKILL.md")).unwrap();

    let second = knap()
        .args(["skill", "--path"])
        .arg(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        second.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_bytes = std::fs::read(dir.path().join("SKILL.md")).unwrap();

    assert_eq!(first_bytes, second_bytes);
    let stdout = String::from_utf8_lossy(&second.stdout);
    assert!(
        stdout.contains("already up to date"),
        "stdout was: {stdout}"
    );
}

#[test]
fn apply_cli_reports_which_operation_failed_in_stderr() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.md"), "[text](old.md)\n").unwrap();
    std::fs::write(dir.path().join("b.md"), "[text](old.md)\n").unwrap();

    // Op 1 is a valid repoint-link; op 2 is a repoint-link whose range eats
    // the closing `)`, so it should fail and be named by position and kind.
    let batch = r#"[
        {"op":"repoint-link","file":"a.md","range":{"start":{"line":0,"character":7},"end":{"line":0,"character":13}},"target":"real.md"},
        {"op":"repoint-link","file":"b.md","range":{"start":{"line":0,"character":7},"end":{"line":0,"character":14}},"target":"real.md"}
    ]"#;

    let output = knap_with_stdin(&["apply"], dir.path(), batch);
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("operation 2 (repoint-link)"),
        "stderr was: {stderr}"
    );
}
