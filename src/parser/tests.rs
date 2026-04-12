use std::path::Path;

use lsp_types::{Position, Range};
use super::{LineIndex, parse, WikiLink};

// ── helpers ──────────────────────────────────────────────────────────────────

fn pos(line: u32, character: u32) -> Position {
    Position { line, character }
}

fn range(start: (u32, u32), end: (u32, u32)) -> Range {
    Range { start: pos(start.0, start.1), end: pos(end.0, end.1) }
}

fn links(content: &str) -> Vec<WikiLink> {
    parse(Path::new("note.md"), content).wiki_links
}

// ── parse() ──────────────────────────────────────────────────────────────────

#[test]
fn stem_from_path() {
    let note = parse(Path::new("/vault/my-note.md"), "");
    assert_eq!(note.stem, "my-note");
}

#[test]
fn content_stored_verbatim() {
    let note = parse(Path::new("note.md"), "hello [[world]]");
    assert_eq!(note.content, "hello [[world]]");
}

// ── link extraction ───────────────────────────────────────────────────────────

#[test]
fn basic_link() {
    let result = links("[[my-note]]");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].stem, "my-note");
}

#[test]
fn multiple_links() {
    let result = links("See [[alpha]] and [[beta]] for details.");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].stem, "alpha");
    assert_eq!(result[1].stem, "beta");
}

#[test]
fn link_in_fenced_code_block() {
    let content = "```\n[[hidden]]\n```";
    assert!(links(content).is_empty());
}

#[test]
fn link_in_inline_code() {
    let content = "`[[hidden]]`";
    assert!(links(content).is_empty());
}

#[test]
fn aliased_link_ignored() {
    assert!(links("[[note|display text]]").is_empty());
}

#[test]
fn heading_anchor_ignored() {
    assert!(links("[[note#section]]").is_empty());
}

#[test]
fn empty_link_ignored() {
    assert!(links("[[]]").is_empty());
}

#[test]
fn unclosed_link_ignored() {
    assert!(links("[[note without closing").is_empty());
}

#[test]
fn whitespace_only_link_ignored() {
    assert!(links("[[   ]]").is_empty());
}

#[test]
fn stem_is_trimmed() {
    // Obsidian allows minor whitespace padding — trim it.
    let result = links("[[ my-note ]]");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].stem, "my-note");
}

// ── ranges ────────────────────────────────────────────────────────────────────

#[test]
fn link_ranges() {
    // "[[note]]" starting at col 0
    // outer: 0..8  → cols 0–8
    // inner: 2..6  → cols 2–6
    let result = links("[[note]]");
    assert_eq!(result[0].range, range((0, 0), (0, 8)));
    assert_eq!(result[0].inner_range, range((0, 2), (0, 6)));
}

#[test]
fn link_ranges_with_offset() {
    // "See [[note]] here"
    //      0123456789...
    // [[ at col 4, ]] ends at col 12
    let result = links("See [[note]] here");
    assert_eq!(result[0].range, range((0, 4), (0, 12)));
    assert_eq!(result[0].inner_range, range((0, 6), (0, 10)));
}

#[test]
fn link_range_on_second_line() {
    let result = links("first line\n[[note]]");
    assert_eq!(result[0].range, range((1, 0), (1, 8)));
    assert_eq!(result[0].inner_range, range((1, 2), (1, 6)));
}

// ── LineIndex ─────────────────────────────────────────────────────────────────

#[test]
fn line_index_single_line() {
    let idx = LineIndex::new("hello");
    assert_eq!(idx.position(0), pos(0, 0));
    assert_eq!(idx.position(4), pos(0, 4));
}

#[test]
fn line_index_positions() {
    // "ab\ncd\nef"
    //  01 2 34 5 67
    let idx = LineIndex::new("ab\ncd\nef");
    assert_eq!(idx.position(0), pos(0, 0)); // 'a'
    assert_eq!(idx.position(1), pos(0, 1)); // 'b'
    assert_eq!(idx.position(3), pos(1, 0)); // 'c'
    assert_eq!(idx.position(4), pos(1, 1)); // 'd'
    assert_eq!(idx.position(6), pos(2, 0)); // 'e'
    assert_eq!(idx.position(7), pos(2, 1)); // 'f'
}

#[test]
fn line_index_range() {
    let idx = LineIndex::new("ab\ncd");
    assert_eq!(idx.range(3..5), range((1, 0), (1, 2)));
}
