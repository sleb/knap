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
fn test_resolve_url() {
    let idx = NoteIndex::default();
    // External URLs resolve Found without any filesystem check.
    assert!(matches!(
        idx.resolve(Path::new("/vault/a.md"), "https://example.com"),
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

    let attachments: Vec<&PathBuf> = idx.all_attachment_paths().collect();
    assert_eq!(attachments.len(), 2);
    assert!(!attachments.contains(&&pb("/vault/a.md")));
    assert!(attachments.contains(&&pb("/vault/img.png")));
    assert!(attachments.contains(&&pb("/vault/doc.pdf")));
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
    assert!(idx.all_tags().next().is_none(), "expected no tags after removal");
}

#[test]
fn notes_by_tag_case_insensitive() {
    let mut idx = NoteIndex::default();
    idx.seed(note("/vault/a.md", "---\ntags: [Rust]\n---\n"));
    let notes = idx.notes_by_tag("rust");
    assert_eq!(notes.len(), 1);
    let notes_upper = idx.notes_by_tag("RUST");
    assert_eq!(notes_upper.len(), 1);
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
    let notes = idx.notes_by_tag("rust");
    assert_eq!(notes.len(), 1, "duplicate tag should only produce one entry");
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
