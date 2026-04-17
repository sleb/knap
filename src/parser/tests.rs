use std::path::Path;

use lsp_types::{Position, Range};
use super::{extract_frontmatter, Frontmatter, Heading, LineIndex, parse, WikiLink};

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

fn headings(content: &str) -> Vec<Heading> {
    parse(Path::new("note.md"), content).headings
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
fn aliased_link_extracts_stem() {
    let result = links("[[note|display text]]");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].stem, "note");
}

#[test]
fn anchor_link_extracts_stem() {
    let result = links("[[note#section]]");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].stem, "note");
}

#[test]
fn anchor_only_ignored() {
    // [[#section]] has no note name — skip it.
    assert!(links("[[#section]]").is_empty());
}

#[test]
fn alias_only_ignored() {
    // [[|alias]] has no note name — skip it.
    assert!(links("[[|alias]]").is_empty());
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

#[test]
fn aliased_link_ranges() {
    // "[[note|display text]]"
    //  0123456789...
    // outer: 0..21, inner (stem "note"): 2..6
    let result = links("[[note|display text]]");
    assert_eq!(result[0].range, range((0, 0), (0, 21)));
    assert_eq!(result[0].inner_range, range((0, 2), (0, 6)));
}

#[test]
fn anchor_link_ranges() {
    // "[[note#section]]"
    //  0123456789...
    // outer: 0..16, inner (stem "note"): 2..6
    let result = links("[[note#section]]");
    assert_eq!(result[0].range, range((0, 0), (0, 16)));
    assert_eq!(result[0].inner_range, range((0, 2), (0, 6)));
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

// ── headings ──────────────────────────────────────────────────────────────────

#[test]
fn heading_single() {
    let result = headings("## My Heading\n");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].text, "My Heading");
    assert_eq!(result[0].level, 2);
}

#[test]
fn heading_multiple_levels() {
    let content = "# Title\n\n## Section\n\n### Subsection\n";
    let result = headings(content);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].text, "Title");
    assert_eq!(result[0].level, 1);
    assert_eq!(result[1].text, "Section");
    assert_eq!(result[1].level, 2);
    assert_eq!(result[2].text, "Subsection");
    assert_eq!(result[2].level, 3);
}

#[test]
fn heading_in_code_block_ignored() {
    let content = "```\n## Not a heading\n```\n";
    assert!(headings(content).is_empty());
}

#[test]
fn heading_text_range() {
    // "## My Heading\n"
    //  0123456789012345
    // "## " is bytes 0–2, "My Heading" is bytes 3–13 (chars 3..13 on line 0)
    let result = headings("## My Heading\n");
    assert_eq!(result[0].text_range, range((0, 3), (0, 13)));
}

// ── wiki-link anchor capture ──────────────────────────────────────────────────

#[test]
fn wiki_link_anchor_captured() {
    let result = links("[[note#Section]]");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].stem, "note");
    assert_eq!(result[0].anchor, Some("Section".to_string()));
}

#[test]
fn wiki_link_anchor_range() {
    // "[[note#Section]]"
    //  0123456789012345
    // "Section" occupies bytes 7–14 → chars (0,7)–(0,14)
    let result = links("[[note#Section]]");
    assert_eq!(result[0].anchor_range, Some(range((0, 7), (0, 14))));
}

#[test]
fn wiki_link_no_anchor() {
    let result = links("[[note]]");
    assert_eq!(result[0].anchor, None);
    assert_eq!(result[0].anchor_range, None);
}

#[test]
fn wiki_link_alias_and_anchor() {
    // Anchor comes before alias in the syntax: [[note#Section|alias]]
    let result = links("[[note#Section|alias]]");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].stem, "note");
    assert_eq!(result[0].anchor, Some("Section".to_string()));
}

#[test]
fn wiki_link_empty_anchor_treated_as_none() {
    // [[note#]] — hash present but no text after it → anchor: None
    let result = links("[[note#]]");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].stem, "note");
    assert_eq!(result[0].anchor, None);
    assert_eq!(result[0].anchor_range, None);
}

// ── frontmatter ───────────────────────────────────────────────────────────────

#[test]
fn frontmatter_title_plain() {
    let content = "---\ntitle: My Title\n---\nBody.\n";
    let fm = extract_frontmatter(content).expect("should have frontmatter");
    assert_eq!(fm.title, Some("My Title".to_string()));
}

#[test]
fn frontmatter_title_double_quoted() {
    let content = "---\ntitle: \"Quoted\"\n---\nBody.\n";
    let fm = extract_frontmatter(content).expect("should have frontmatter");
    assert_eq!(fm.title, Some("Quoted".to_string()));
}

#[test]
fn frontmatter_title_single_quoted() {
    let content = "---\ntitle: 'Quoted'\n---\nBody.\n";
    let fm = extract_frontmatter(content).expect("should have frontmatter");
    assert_eq!(fm.title, Some("Quoted".to_string()));
}

#[test]
fn frontmatter_title_absent() {
    // Block exists but has no title key.
    let content = "---\ntags: [foo, bar]\n---\nBody.\n";
    let fm = extract_frontmatter(content).expect("should have frontmatter");
    assert_eq!(fm, Frontmatter { title: None });
}

#[test]
fn frontmatter_no_block() {
    // No leading --- → note.frontmatter is None.
    let note = parse(Path::new("note.md"), "No frontmatter here.\n");
    assert_eq!(note.frontmatter, None);
}

#[test]
fn frontmatter_unclosed() {
    // Opening --- with no closing --- → treat as absent.
    let content = "---\ntitle: My Title\nBody without closing.\n";
    assert_eq!(extract_frontmatter(content), None);
}

#[test]
fn frontmatter_block_scalar_ignored() {
    // title: | → block scalar, treated as None.
    let content = "---\ntitle: |\n  multi-line\n---\nBody.\n";
    let fm = extract_frontmatter(content).expect("should have frontmatter");
    assert_eq!(fm.title, None);
}
