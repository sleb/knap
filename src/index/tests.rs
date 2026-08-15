use std::path::{Path, PathBuf};

use crate::index::{NoteIndex, ResolvedLink, build, walk_files};
use crate::test_helpers::note;

fn pb(s: &str) -> PathBuf {
    PathBuf::from(s)
}

// ── resolve ──────────────────────────────────────────────────────────────────

#[test]
fn test_resolve_relative() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/b.md", ""));
    // Source at /vault/a.md links to "b.md" → resolves to /vault/b.md
    assert!(matches!(
        idx.resolve(Path::new("/vault/a.md"), "b.md"),
        ResolvedLink::Found(_)
    ));
}

#[test]
fn test_resolve_parent_dir() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/other/note.md", ""));
    // Source at /vault/sub/a.md links to "../other/note.md"
    assert!(matches!(
        idx.resolve(Path::new("/vault/sub/a.md"), "../other/note.md"),
        ResolvedLink::Found(_)
    ));
}

#[test]
fn test_resolve_broken() {
    let idx = NoteIndex::default();
    assert!(matches!(
        idx.resolve(Path::new("/vault/a.md"), "missing.md"),
        ResolvedLink::Broken
    ));
}

#[test]
fn resolve_empty_target_resolves_to_source() {
    let idx = NoteIndex::default();
    // A same-file link (empty target, e.g. `[§1](#section-one)`) resolves to
    // the source note itself, without any filesystem lookup.
    assert!(matches!(
        idx.resolve(Path::new("/vault/a.md"), ""),
        ResolvedLink::Found(p) if p == Path::new("/vault/a.md")
    ));
}

#[test]
fn resolve_empty_target_with_anchor_resolves_to_source() {
    let idx = NoteIndex::default();
    // The anchor itself is carried separately from `target` (see
    // `MarkdownLink::anchor`); `resolve()` only ever sees the empty target.
    assert!(matches!(
        idx.resolve(Path::new("/vault/a.md"), ""),
        ResolvedLink::Found(p) if p == Path::new("/vault/a.md")
    ));
}

#[test]
fn test_resolve_url() {
    let idx = NoteIndex::default();
    // External URLs resolve Found without any filesystem check.
    assert!(matches!(
        idx.resolve(Path::new("/vault/a.md"), "https://example.com"),
        ResolvedLink::Found(_)
    ));
}

#[test]
fn test_resolve_escaped_target_with_space() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/My File.md", ""));
    // A completion-inserted, angle-bracket-wrapped target must resolve to the
    // real file, not to a literal "<My File.md>" path.
    assert!(matches!(
        idx.resolve(Path::new("/vault/a.md"), "<My File.md>"),
        ResolvedLink::Found(_)
    ));
}

#[test]
fn test_resolve_escaped_target_with_inner_angle_bracket() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/My <File>.md", ""));
    assert!(matches!(
        idx.resolve(Path::new("/vault/a.md"), "<My \\<File\\>.md>"),
        ResolvedLink::Found(_)
    ));
}

// ── index / remove ───────────────────────────────────────────────────────────

#[test]
fn index_replaces_existing() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/b.md", ""));
    idx.seed(note("/vault/a.md", "[link](b.md)"));
    idx.seed(note("/vault/a.md", "[link](c.md)")); // replace
    let n = idx.get_note(Path::new("/vault/a.md")).unwrap();
    assert_eq!(n.md_links.len(), 1);
    assert_eq!(n.md_links[0].target, "c.md");
}

#[test]
fn remove_clears_note() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/a.md", ""));
    let _ = idx.remove(Path::new("/vault/a.md"));
    assert!(idx.get_note(Path::new("/vault/a.md")).is_none());
    assert!(matches!(
        idx.resolve(Path::new("/vault/other.md"), "a.md"),
        ResolvedLink::Broken
    ));
}

// ── links_to ─────────────────────────────────────────────────────────────────

#[test]
fn test_index_populates_links_to() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/b.md", ""));
    idx.seed(note("/vault/a.md", "[link](b.md)"));
    let links = idx.links_to(Path::new("/vault/b.md"));
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].source_path, pb("/vault/a.md"));
}

#[test]
fn test_index_populates_links_to_with_escaped_target() {
    // A link target wrapped in `<...>` (produced by escape_link_target for
    // paths containing spaces/special chars) must still be unescaped before
    // being recorded in links_to, matching what resolve() does.
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/My File.md", ""));
    idx.seed(note("/vault/a.md", "[link](<My File.md>)"));
    let links = idx.links_to(Path::new("/vault/My File.md"));
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].source_path, pb("/vault/a.md"));
}

#[test]
fn test_recheck_incoming_with_escaped_target() {
    // a.md links to an escaped target for b.md, but b.md doesn't exist yet
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/a.md", "[link](<My File.md>)"));
    assert_eq!(idx.links_to(Path::new("/vault/My File.md")).len(), 0);

    // Now add the target — recheck_incoming should pick up a.md's link
    // after unescaping it.
    idx.seed(note("/vault/My File.md", ""));
    let links = idx.links_to(Path::new("/vault/My File.md"));
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].source_path, pb("/vault/a.md"));
}

#[test]
fn test_recheck_incoming() {
    // a.md links to b.md, but b.md doesn't exist yet
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/a.md", "[link](b.md)"));
    assert_eq!(idx.links_to(Path::new("/vault/b.md")).len(), 0);

    // Now add b.md — recheck_incoming should pick up a.md's link
    idx.seed(note("/vault/b.md", ""));
    let links = idx.links_to(Path::new("/vault/b.md"));
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].source_path, pb("/vault/a.md"));
}

#[test]
fn test_remove_breaks_incoming() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/b.md", ""));
    idx.seed(note("/vault/a.md", "[link](b.md)"));

    let delta = idx.remove(Path::new("/vault/b.md"));
    // a.md linked to b.md and now has a broken link
    assert!(delta.affected_paths.contains(Path::new("/vault/a.md")));
    assert!(delta.affected_paths.contains(Path::new("/vault/b.md")));
    assert_eq!(idx.links_to(Path::new("/vault/b.md")).len(), 0);
}

// ── IndexDelta ───────────────────────────────────────────────────────────────

#[test]
fn delta_includes_affected() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/b.md", ""));
    // a.md links to b.md → indexing a.md should affect both a.md and b.md
    let delta = idx.index(note("/vault/a.md", "[link](b.md)"));
    assert!(delta.affected_paths.contains(Path::new("/vault/a.md")));
    assert!(delta.affected_paths.contains(Path::new("/vault/b.md")));
}

#[test]
fn remove_delta_includes_incoming() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/b.md", ""));
    idx.seed(note("/vault/a.md", "[link](b.md)"));
    let delta = idx.remove(Path::new("/vault/b.md"));
    assert!(delta.affected_paths.contains(Path::new("/vault/a.md")));
    assert!(delta.affected_paths.contains(Path::new("/vault/b.md")));
}

// ── attachments ───────────────────────────────────────────────────────────────

#[test]
fn test_add_attachment_resolves() {
    let mut idx = NoteIndex::default();
    // Note with an image link
    idx.seed(note("/vault/a.md", "![img](assets/image.png)"));
    // Initially broken (image not in all_files)
    assert!(matches!(
        idx.resolve(Path::new("/vault/a.md"), "assets/image.png"),
        ResolvedLink::Broken
    ));

    // Register the attachment
    let _ = idx.add_attachment(PathBuf::from("/vault/assets/image.png"));
    assert!(matches!(
        idx.resolve(Path::new("/vault/a.md"), "assets/image.png"),
        ResolvedLink::Found(_)
    ));
}

#[test]
fn attachment_recheck_heals_link() {
    let mut idx = NoteIndex::default();
    // Note with a broken attachment link
    idx.seed(note("/vault/a.md", "[img](logo.png)"));
    assert_eq!(idx.links_to(Path::new("/vault/logo.png")).len(), 0);

    // Add attachment — recheck_incoming should register the link
    let delta = idx.add_attachment(PathBuf::from("/vault/logo.png"));
    assert!(delta.affected_paths.contains(Path::new("/vault/a.md")));
    assert_eq!(idx.links_to(Path::new("/vault/logo.png")).len(), 1);
}

#[test]
fn attachment_remove_breaks_links() {
    let mut idx = NoteIndex::default();
    let _ = idx.add_attachment(PathBuf::from("/vault/logo.png"));
    idx.seed(note("/vault/a.md", "[img](logo.png)"));
    assert_eq!(idx.links_to(Path::new("/vault/logo.png")).len(), 1);

    let delta = idx.remove_attachment(Path::new("/vault/logo.png"));
    assert!(delta.affected_paths.contains(Path::new("/vault/a.md")));
    assert_eq!(idx.links_to(Path::new("/vault/logo.png")).len(), 0);
}

// ── all_attachment_paths ──────────────────────────────────────────────────────

#[test]
fn all_attachment_paths_excludes_notes() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/a.md", ""));
    let _ = idx.add_attachment(pb("/vault/img.png"));
    let _ = idx.add_attachment(pb("/vault/doc.pdf"));

    let attachments: Vec<PathBuf> = idx.all_attachment_paths().map(Path::to_path_buf).collect();
    assert_eq!(attachments.len(), 2);
    assert!(!attachments.contains(&pb("/vault/a.md")));
    assert!(attachments.contains(&pb("/vault/img.png")));
    assert!(attachments.contains(&pb("/vault/doc.pdf")));
}

// ── tag index ─────────────────────────────────────────────────────────────────

#[test]
fn index_by_tag_populated() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/a.md", "---\ntags: [rust, lsp]\n---\n"));
    let tags: Vec<&str> = idx.all_tags().collect();
    assert!(tags.contains(&"rust"), "expected 'rust' in tags");
    assert!(tags.contains(&"lsp"), "expected 'lsp' in tags");
}

#[test]
fn index_by_tag_removed() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/a.md", "---\ntags: [rust]\n---\n"));
    let _ = idx.remove(Path::new("/vault/a.md"));
    assert!(
        idx.all_tags().next().is_none(),
        "expected no tags after removal"
    );
}

#[test]
fn notes_by_tag_case_insensitive() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/a.md", "---\ntags: [Rust]\n---\n"));
    assert_eq!(idx.notes_by_tag("rust").count(), 1);
    assert_eq!(idx.notes_by_tag("RUST").count(), 1);
}

#[test]
fn all_tags_distinct() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/a.md", "---\ntags: [rust, lsp]\n---\n"));
    idx.seed(note("/vault/b.md", "---\ntags: [rust, tools]\n---\n"));
    let mut tags: Vec<&str> = idx.all_tags().collect();
    tags.sort();
    assert_eq!(tags, vec!["lsp", "rust", "tools"]);
}

#[test]
fn duplicate_tags_within_note_not_double_counted() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/a.md", "---\ntags: [rust, rust]\n---\n"));
    assert_eq!(
        idx.notes_by_tag("rust").count(),
        1,
        "duplicate tag should only produce one entry"
    );
}

#[test]
fn index_replace_updates_tags() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/a.md", "---\ntags: [old]\n---\n"));
    idx.seed(note("/vault/a.md", "---\ntags: [new]\n---\n")); // replace
    let tags: Vec<&str> = idx.all_tags().collect();
    assert!(!tags.contains(&"old"), "old tag should be removed");
    assert!(tags.contains(&"new"), "new tag should be present");
}

// ── report ───────────────────────────────────────────────────────────────────

#[test]
fn report_includes_all_notes_sorted_by_path() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/z.md", ""));
    idx.seed(note("/vault/a.md", ""));
    idx.seed(note("/vault/m.md", ""));

    let report = idx.report();
    let paths: Vec<&PathBuf> = report.notes.iter().map(|n| &n.path).collect();
    assert_eq!(
        paths,
        vec![&pb("/vault/a.md"), &pb("/vault/m.md"), &pb("/vault/z.md")]
    );
}

#[test]
fn report_link_summary_marks_broken_links() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/a.md", "[missing](missing.md)\n"));

    let report = idx.report();
    let note = report
        .notes
        .iter()
        .find(|n| n.path == pb("/vault/a.md"))
        .unwrap();
    assert_eq!(note.links.len(), 1);
    assert_eq!(note.links[0].target, "missing.md");
    assert_eq!(note.links[0].resolved, None);
}

#[test]
fn report_link_summary_marks_resolved_links() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/b.md", ""));
    idx.seed(note("/vault/a.md", "[real](b.md)\n"));

    let report = idx.report();
    let note = report
        .notes
        .iter()
        .find(|n| n.path == pb("/vault/a.md"))
        .unwrap();
    assert_eq!(note.links.len(), 1);
    assert_eq!(note.links[0].target, "b.md");
    assert_eq!(note.links[0].resolved, Some(pb("/vault/b.md")));
}

#[test]
fn report_tags_map_groups_by_tag() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/a.md", "---\ntags: [rust]\n---\n"));
    idx.seed(note("/vault/b.md", "---\ntags: [rust]\n---\n"));

    let report = idx.report();
    let rust_paths = report.tags.get("rust").expect("rust tag should be present");
    assert_eq!(rust_paths, &vec![pb("/vault/a.md"), pb("/vault/b.md")]);
}

// ── note_report ──────────────────────────────────────────────────────────────

#[test]
fn note_report_matches_report_entry() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/b.md", ""));
    idx.seed(note("/vault/a.md", "[real](b.md)\n[missing](missing.md)\n"));

    let report = idx.report();
    let expected = report
        .notes
        .iter()
        .find(|n| n.path == pb("/vault/a.md"))
        .unwrap();

    let actual = idx.note_report(Path::new("/vault/a.md")).unwrap();
    assert_eq!(&actual, expected);
}

#[test]
fn note_report_none_for_unindexed_path() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/a.md", ""));

    assert!(idx.note_report(Path::new("/vault/missing.md")).is_none());
}

// ── walk_dir / build ─────────────────────────────────────────────────────────

#[test]
fn walk_files_strips_leading_curdir_from_root() {
    let root = Path::new("./tests/fixtures/lint_clean");
    let files = walk_files(root, &[]);
    assert!(
        !files.is_empty(),
        "expected walk_files to find fixture files"
    );
    for path in &files {
        assert_ne!(
            path.components().next(),
            Some(std::path::Component::CurDir),
            "path {path:?} should not have a leading './' component"
        );
    }
}

#[test]
fn build_with_leading_curdir_root_resolves_relative_links() {
    let roots = vec![PathBuf::from("./tests/fixtures/lint_clean")];
    let (idx, _) = build(&roots, &["md"], &[]).unwrap();
    let note_path = PathBuf::from("tests/fixtures/lint_clean/note.md");
    assert!(
        matches!(idx.resolve(&note_path, "target.md"), ResolvedLink::Found(_)),
        "expected note.md's link to target.md to resolve as Found"
    );
}

// ── exclude ──────────────────────────────────────────────────────────────────

fn write_file(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

#[test]
fn build_excludes_directory_by_exact_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_file(&root.join("tests/fixtures/note.md"), "# excluded\n");
    write_file(&root.join("other.md"), "# kept\n");

    let exclude = vec!["tests/fixtures".to_string()];
    let (idx, _) = build(std::slice::from_ref(&root), &["md"], &exclude).unwrap();

    assert!(idx.get_note(&root.join("tests/fixtures/note.md")).is_none());
    assert!(idx.get_note(&root.join("other.md")).is_some());
}

#[test]
fn build_excludes_directory_by_glob() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_file(&root.join("tests/fixtures/note.md"), "# excluded\n");
    write_file(&root.join("other.md"), "# kept\n");

    let exclude = vec!["tests/fixtures/**".to_string()];
    let (idx, _) = build(std::slice::from_ref(&root), &["md"], &exclude).unwrap();

    assert!(idx.get_note(&root.join("tests/fixtures/note.md")).is_none());
    assert!(idx.get_note(&root.join("other.md")).is_some());
}

#[test]
fn build_excludes_directory_by_glob_without_opening_it() {
    // Distinguishes "directory never read_dir'd" from "directory read_dir'd
    // then its contents filtered" — build_excludes_directory_by_glob above
    // already proves the output is correct either way; this proves the
    // `dir/**` form gets the same true skip as the exact-path form.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_file(&root.join("tests/fixtures/note.md"), "# excluded\n");
    write_file(&root.join("other.md"), "# kept\n");

    super::DIR_READS.with(|c| c.set(0));
    let exclude = vec!["tests/fixtures/**".to_string()];
    let (idx, _) = build(std::slice::from_ref(&root), &["md"], &exclude).unwrap();
    let reads = super::DIR_READS.with(|c| c.get());

    assert!(idx.get_note(&root.join("tests/fixtures/note.md")).is_none());
    assert!(idx.get_note(&root.join("other.md")).is_some());
    // Only `root` and `root/tests` should ever be opened — `root/tests/fixtures`
    // must be recognized as excluded and skipped without a `read_dir` call.
    assert_eq!(
        reads, 2,
        "expected tests/fixtures itself to be skipped, not opened and filtered"
    );
}

#[test]
fn build_excludes_file_by_glob() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_file(&root.join("a.draft.md"), "# draft\n");
    write_file(&root.join("a.md"), "# kept\n");

    let exclude = vec!["**/*.draft.md".to_string()];
    let (idx, _) = build(std::slice::from_ref(&root), &["md"], &exclude).unwrap();

    assert!(idx.get_note(&root.join("a.draft.md")).is_none());
    assert!(idx.get_note(&root.join("a.md")).is_some());
}

#[test]
fn build_excluded_file_not_registered_as_attachment() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_file(&root.join("tests/fixtures/image.png"), "not really png");
    write_file(&root.join("kept.png"), "not really png");

    let exclude = vec!["tests/fixtures".to_string()];
    let (idx, _) = build(std::slice::from_ref(&root), &["md"], &exclude).unwrap();

    let attachments: Vec<_> = idx.all_attachment_paths().collect();
    assert!(!attachments.contains(&root.join("tests/fixtures/image.png").as_path()));
    assert!(attachments.contains(&root.join("kept.png").as_path()));
}

#[test]
fn build_no_excludes_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_file(&root.join("tests/fixtures/note.md"), "# note\n");
    write_file(&root.join("other.md"), "# other\n");

    let (idx, _) = build(std::slice::from_ref(&root), &["md"], &[]).unwrap();

    assert!(idx.get_note(&root.join("tests/fixtures/note.md")).is_some());
    assert!(idx.get_note(&root.join("other.md")).is_some());
}

#[test]
fn build_malformed_pattern_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();

    let exclude = vec!["[".to_string()];
    let result = build(&[root], &["md"], &exclude);

    assert!(result.is_err());
}
