use std::path::{Path, PathBuf};

use crate::index::{NoteIndex, ResolvedLink};
use crate::parser;

fn note(path: &str, content: &str) -> crate::parser::Note {
    parser::parse(Path::new(path), content)
}

fn pb(s: &str) -> PathBuf {
    PathBuf::from(s)
}

// ── resolve ──────────────────────────────────────────────────────────────────

#[test]
fn resolve_found() {
    let mut idx = NoteIndex::default();
    idx.seed(note("foo.md", ""));
    assert!(matches!(idx.resolve("foo"), ResolvedLink::Found(_)));
}

#[test]
fn resolve_broken() {
    let idx = NoteIndex::default();
    assert!(matches!(idx.resolve("missing"), ResolvedLink::Broken));
}

#[test]
fn resolve_ambiguous() {
    let mut idx = NoteIndex::default();
    idx.seed(note("dir1/foo.md", ""));
    idx.seed(note("dir2/foo.md", ""));
    assert!(matches!(idx.resolve("foo"), ResolvedLink::Ambiguous(_)));
}

// ── index / remove ───────────────────────────────────────────────────────────

#[test]
fn index_replaces_existing() {
    let mut idx = NoteIndex::default();
    idx.seed(note("a.md", "[[b]]"));
    idx.seed(note("a.md", "[[c]]")); // replace
    let n = idx.get_note(Path::new("a.md")).unwrap();
    assert_eq!(n.wiki_links.len(), 1);
    assert_eq!(n.wiki_links[0].stem, "c");
}

#[test]
fn remove_clears_all_maps() {
    let mut idx = NoteIndex::default();
    idx.seed(note("a.md", ""));
    let _ = idx.remove(Path::new("a.md"));
    assert!(idx.get_note(Path::new("a.md")).is_none());
    assert!(matches!(idx.resolve("a"), ResolvedLink::Broken));
}

// ── links_to ─────────────────────────────────────────────────────────────────

#[test]
fn links_to_populated() {
    let mut idx = NoteIndex::default();
    idx.seed(note("b.md", ""));
    idx.seed(note("a.md", "[[b]]"));
    let links = idx.links_to(Path::new("b.md"));
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].source_path, pb("a.md"));
}

#[test]
fn broken_link_heals_on_add() {
    let mut idx = NoteIndex::default();
    idx.seed(note("a.md", "[[b]]")); // b.md doesn't exist yet
    assert_eq!(idx.links_to(Path::new("b.md")).len(), 0);

    idx.seed(note("b.md", ""));
    let links = idx.links_to(Path::new("b.md"));
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].source_path, pb("a.md"));
}

#[test]
fn link_breaks_on_remove() {
    let mut idx = NoteIndex::default();
    idx.seed(note("b.md", ""));
    idx.seed(note("a.md", "[[b]]"));
    let _ = idx.remove(Path::new("b.md"));
    assert_eq!(idx.links_to(Path::new("b.md")).len(), 0);
}

// ── IndexDelta ───────────────────────────────────────────────────────────────

#[test]
fn delta_includes_affected() {
    let mut idx = NoteIndex::default();
    idx.seed(note("b.md", ""));
    // a.md links to b.md → indexing a.md should affect both a.md and b.md
    let delta = idx.index(note("a.md", "[[b]]"));
    assert!(delta.affected_paths.contains(Path::new("a.md")));
    assert!(delta.affected_paths.contains(Path::new("b.md")));
}

#[test]
fn remove_delta_includes_incoming() {
    let mut idx = NoteIndex::default();
    idx.seed(note("b.md", ""));
    idx.seed(note("a.md", "[[b]]"));
    let delta = idx.remove(Path::new("b.md"));
    // a.md linked to b.md and now has a broken link
    assert!(delta.affected_paths.contains(Path::new("a.md")));
    assert!(delta.affected_paths.contains(Path::new("b.md")));
}

#[test]
fn ambiguous_becomes_found() {
    let mut idx = NoteIndex::default();
    idx.seed(note("dir1/foo.md", ""));
    idx.seed(note("dir2/foo.md", ""));
    assert!(matches!(idx.resolve("foo"), ResolvedLink::Ambiguous(_)));

    let _ = idx.remove(Path::new("dir1/foo.md"));
    assert!(matches!(idx.resolve("foo"), ResolvedLink::Found(_)));
}

// ── by_filename ───────────────────────────────────────────────────────────────

#[test]
fn by_filename_populated_for_note() {
    // index(note) must also register the full filename so [[foo.md]] resolves.
    let mut idx = NoteIndex::default();
    idx.seed(note("foo.md", ""));
    // "foo" resolves via by_stem; "foo.md" must resolve via by_filename.
    assert!(matches!(idx.resolve("foo.md"), ResolvedLink::Found(_)));
}

#[test]
fn by_filename_cleared_on_remove() {
    let mut idx = NoteIndex::default();
    idx.seed(note("foo.md", ""));
    let _ = idx.remove(Path::new("foo.md"));
    assert!(matches!(idx.resolve("foo.md"), ResolvedLink::Broken));
}

#[test]
fn resolve_falls_through_to_filename() {
    // A non-note attachment registered via add_attachment must be Found.
    let mut idx = NoteIndex::default();
    let _ = idx.add_attachment(PathBuf::from("/workspace/image.png"));
    assert!(matches!(idx.resolve("image.png"), ResolvedLink::Found(_)));
}

#[test]
fn resolve_prefers_stem_over_filename() {
    // A note "foo.md" is in by_stem["foo"]. An attachment named "foo" (no
    // extension) is in by_filename["foo"]. by_stem must win.
    let mut idx = NoteIndex::default();
    idx.seed(note("foo.md", ""));
    let _ = idx.add_attachment(PathBuf::from("/workspace/foo")); // no extension
    match idx.resolve("foo") {
        ResolvedLink::Found(p) => assert!(p.to_string_lossy().ends_with("foo.md")),
        other => panic!("expected Found(foo.md), got {other:?}"),
    }
}

#[test]
fn resolve_broken_in_both_maps() {
    let idx = NoteIndex::default();
    assert!(matches!(idx.resolve("nonexistent.png"), ResolvedLink::Broken));
}

#[test]
fn non_note_file_registered() {
    let mut idx = NoteIndex::default();
    let _ = idx.add_attachment(PathBuf::from("/workspace/diagram.png"));
    assert!(matches!(idx.resolve("diagram.png"), ResolvedLink::Found(_)));
}

// ── tag index ─────────────────────────────────────────────────────────────────

#[test]
fn index_by_tag_populated() {
    let mut idx = NoteIndex::default();
    idx.seed(note("a.md", "---\ntags: [rust, lsp]\n---\n"));
    let tags: Vec<&str> = idx.all_tags().collect();
    assert!(tags.contains(&"rust"), "expected 'rust' in tags");
    assert!(tags.contains(&"lsp"), "expected 'lsp' in tags");
}

#[test]
fn index_by_tag_removed() {
    let mut idx = NoteIndex::default();
    idx.seed(note("a.md", "---\ntags: [rust]\n---\n"));
    let _ = idx.remove(Path::new("a.md"));
    assert!(idx.all_tags().next().is_none(), "expected no tags after removal");
}

#[test]
fn notes_by_tag_case_insensitive() {
    let mut idx = NoteIndex::default();
    idx.seed(note("a.md", "---\ntags: [Rust]\n---\n"));
    let notes = idx.notes_by_tag("rust");
    assert_eq!(notes.len(), 1);
    let notes_upper = idx.notes_by_tag("RUST");
    assert_eq!(notes_upper.len(), 1);
}

#[test]
fn all_tags_distinct() {
    let mut idx = NoteIndex::default();
    idx.seed(note("a.md", "---\ntags: [rust, lsp]\n---\n"));
    idx.seed(note("b.md", "---\ntags: [rust, tools]\n---\n"));
    let mut tags: Vec<&str> = idx.all_tags().collect();
    tags.sort();
    assert_eq!(tags, vec!["lsp", "rust", "tools"]);
}

#[test]
fn duplicate_tags_within_note_not_double_counted() {
    let mut idx = NoteIndex::default();
    idx.seed(note("a.md", "---\ntags: [rust, rust]\n---\n"));
    let notes = idx.notes_by_tag("rust");
    assert_eq!(notes.len(), 1, "duplicate tag should only produce one entry");
}

#[test]
fn index_replace_updates_tags() {
    let mut idx = NoteIndex::default();
    idx.seed(note("a.md", "---\ntags: [old]\n---\n"));
    idx.seed(note("a.md", "---\ntags: [new]\n---\n")); // replace
    let tags: Vec<&str> = idx.all_tags().collect();
    assert!(!tags.contains(&"old"), "old tag should be removed");
    assert!(tags.contains(&"new"), "new tag should be present");
}
