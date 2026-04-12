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
    idx.index(note("foo.md", ""));
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
    idx.index(note("dir1/foo.md", ""));
    idx.index(note("dir2/foo.md", ""));
    assert!(matches!(idx.resolve("foo"), ResolvedLink::Ambiguous(_)));
}

// ── index / remove ───────────────────────────────────────────────────────────

#[test]
fn index_replaces_existing() {
    let mut idx = NoteIndex::default();
    idx.index(note("a.md", "[[b]]"));
    idx.index(note("a.md", "[[c]]")); // replace
    let n = idx.get_note(Path::new("a.md")).unwrap();
    assert_eq!(n.wiki_links.len(), 1);
    assert_eq!(n.wiki_links[0].stem, "c");
}

#[test]
fn remove_clears_all_maps() {
    let mut idx = NoteIndex::default();
    idx.index(note("a.md", ""));
    idx.remove(Path::new("a.md"));
    assert!(idx.get_note(Path::new("a.md")).is_none());
    assert!(matches!(idx.resolve("a"), ResolvedLink::Broken));
}

// ── links_to ─────────────────────────────────────────────────────────────────

#[test]
fn links_to_populated() {
    let mut idx = NoteIndex::default();
    idx.index(note("b.md", ""));
    idx.index(note("a.md", "[[b]]"));
    let links = idx.links_to(Path::new("b.md"));
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].source_path, pb("a.md"));
}

#[test]
fn broken_link_heals_on_add() {
    let mut idx = NoteIndex::default();
    idx.index(note("a.md", "[[b]]")); // b.md doesn't exist yet
    assert_eq!(idx.links_to(Path::new("b.md")).len(), 0);

    idx.index(note("b.md", ""));
    let links = idx.links_to(Path::new("b.md"));
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].source_path, pb("a.md"));
}

#[test]
fn link_breaks_on_remove() {
    let mut idx = NoteIndex::default();
    idx.index(note("b.md", ""));
    idx.index(note("a.md", "[[b]]"));
    idx.remove(Path::new("b.md"));
    assert_eq!(idx.links_to(Path::new("b.md")).len(), 0);
}

// ── IndexDelta ───────────────────────────────────────────────────────────────

#[test]
fn delta_includes_affected() {
    let mut idx = NoteIndex::default();
    idx.index(note("b.md", ""));
    // a.md links to b.md → indexing a.md should affect both a.md and b.md
    let delta = idx.index(note("a.md", "[[b]]"));
    assert!(delta.affected_paths.contains(Path::new("a.md")));
    assert!(delta.affected_paths.contains(Path::new("b.md")));
}

#[test]
fn remove_delta_includes_incoming() {
    let mut idx = NoteIndex::default();
    idx.index(note("b.md", ""));
    idx.index(note("a.md", "[[b]]"));
    let delta = idx.remove(Path::new("b.md"));
    // a.md linked to b.md and now has a broken link
    assert!(delta.affected_paths.contains(Path::new("a.md")));
    assert!(delta.affected_paths.contains(Path::new("b.md")));
}

#[test]
fn ambiguous_becomes_found() {
    let mut idx = NoteIndex::default();
    idx.index(note("dir1/foo.md", ""));
    idx.index(note("dir2/foo.md", ""));
    assert!(matches!(idx.resolve("foo"), ResolvedLink::Ambiguous(_)));

    idx.remove(Path::new("dir1/foo.md"));
    assert!(matches!(idx.resolve("foo"), ResolvedLink::Found(_)));
}
