use std::path::Path;

use lsp_types::{Position, Range};
use super::{
    extract_frontmatter, Frontmatter, FrontmatterField, Heading, LineIndex, MarkdownLink, parse,
    Tag,
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn pos(line: u32, character: u32) -> Position {
    Position { line, character }
}

fn range(start: (u32, u32), end: (u32, u32)) -> Range {
    Range { start: pos(start.0, start.1), end: pos(end.0, end.1) }
}

fn md_links(content: &str) -> Vec<MarkdownLink> {
    parse(Path::new("note.md"), content).md_links
}

fn headings(content: &str) -> Vec<Heading> {
    parse(Path::new("note.md"), content).headings
}

// ── parse() ──────────────────────────────────────────────────────────────────

#[test]
fn content_stored_verbatim() {
    let note = parse(Path::new("note.md"), "hello world");
    assert_eq!(note.content, "hello world");
}

// ── Markdown links — basic extraction ────────────────────────────────────────

#[test]
fn md_link_basic() {
    let result = md_links("[text](path.md)");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].text, "text");
    assert_eq!(result[0].target, "path.md");
    assert_eq!(result[0].anchor, None);
    assert!(!result[0].is_image);
    // full range: [text](path.md) = 15 chars, cols 0–15
    assert_eq!(result[0].range, range((0, 0), (0, 15)));
}

#[test]
fn md_link_with_anchor() {
    // "[text](note.md#section)"
    //  0123456789012345678901234
    //  target: "note.md" = cols 7–14
    //  anchor: "section" = cols 15–22
    let result = md_links("[text](note.md#section)");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].target, "note.md");
    assert_eq!(result[0].anchor, Some("section".to_string()));
    assert_eq!(result[0].target_range, range((0, 7), (0, 14)));
    assert_eq!(result[0].anchor_range, Some(range((0, 15), (0, 22))));
}

#[test]
fn md_link_anchor_only() {
    // "[text](#heading)" — empty target, anchor "heading"
    //  0123456789012345
    //  url starts at 7, '#' at 7, anchor starts at 8, ends at 15
    let result = md_links("[text](#heading)");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].target, "");
    assert_eq!(result[0].anchor, Some("heading".to_string()));
    // target_range: zero-width at col 7
    assert_eq!(result[0].target_range, range((0, 7), (0, 7)));
    // anchor_range: cols 8–15
    assert_eq!(result[0].anchor_range, Some(range((0, 8), (0, 15))));
}

#[test]
fn md_link_image() {
    let result = md_links("![alt text](img.png)");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].text, "alt text");
    assert_eq!(result[0].target, "img.png");
    assert_eq!(result[0].anchor, None);
    assert!(result[0].is_image);
    // full range: "![alt text](img.png)" = 20 chars
    assert_eq!(result[0].range, range((0, 0), (0, 20)));
}

#[test]
fn md_link_external_url() {
    let result = md_links("[text](https://example.com)");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].target, "https://example.com");
    assert_eq!(result[0].anchor, None);
}

#[test]
fn md_link_in_fenced_code_ignored() {
    let content = "```\n[hidden](url)\n```\n";
    assert!(md_links(content).is_empty());
}

// ── Markdown links — range assertions ────────────────────────────────────────

#[test]
fn md_link_range() {
    // "[text](url)" — 11 bytes at column 0
    let result = md_links("[text](url)");
    assert_eq!(result[0].range, range((0, 0), (0, 11)));
}

#[test]
fn md_link_target_range_no_anchor() {
    // "[text](path.md)" — target "path.md" occupies cols 7–14
    let result = md_links("[text](path.md)");
    assert_eq!(result[0].target_range, range((0, 7), (0, 14)));
    assert_eq!(result[0].anchor_range, None);
}

#[test]
fn md_link_target_range_with_anchor() {
    // "[text](path.md#section)" — target "path.md" occupies cols 7–14
    let result = md_links("[text](path.md#section)");
    assert_eq!(result[0].target_range, range((0, 7), (0, 14)));
}

#[test]
fn md_link_anchor_range() {
    // "[text](note.md#section)" — anchor "section" occupies cols 15–22
    let result = md_links("[text](note.md#section)");
    let ar = result[0].anchor_range.expect("expected anchor_range");
    assert_eq!(ar, range((0, 15), (0, 22)));
}

#[test]
fn md_link_after_offset() {
    // Link on second line: "\n[text](url)"
    // "[" is at body byte 1 which is line 1 col 0
    let result = md_links("\n[text](url)");
    assert_eq!(result[0].range.start.line, 1);
    assert_eq!(result[0].range.start.character, 0);
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
    assert_eq!(fm, Frontmatter { title: None, tags: vec![], fields: vec![] });
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

#[test]
fn frontmatter_headings_not_scanned() {
    // Without the body-offset fix, pulldown-cmark treats the closing `---` of
    // the frontmatter as a setext-H2 underline and emits a spurious heading.
    // With the fix, only the ATX heading in the body is collected.
    let content = "---\ntitle: My Title\ntags: [foo]\n---\n\n## Real Heading\n";
    let note = parse(Path::new("note.md"), content);
    assert_eq!(note.headings.len(), 1);
    assert_eq!(note.headings[0].text, "Real Heading");
}

// ── tags ──────────────────────────────────────────────────────────────────────

fn tags(content: &str) -> Vec<Tag> {
    parse(Path::new("note.md"), content)
        .frontmatter
        .map(|fm| fm.tags)
        .unwrap_or_default()
}

#[test]
fn tags_inline_list() {
    let result = tags("---\ntags: [foo, bar]\n---\n");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].name, "foo");
    assert_eq!(result[1].name, "bar");
}

#[test]
fn tags_block_list() {
    let result = tags("---\ntags:\n  - foo\n  - bar\n---\n");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].name, "foo");
    assert_eq!(result[1].name, "bar");
}

#[test]
fn tags_bare_scalar() {
    let result = tags("---\ntags: productivity\n---\n");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "productivity");
}

#[test]
fn tags_absent() {
    let result = tags("---\ntitle: Note\n---\n");
    assert!(result.is_empty());
}

#[test]
fn tags_empty_value_no_list_items() {
    // `tags:` with nothing after and no list items below → empty
    let result = tags("---\ntags:\ntitle: foo\n---\n");
    assert!(result.is_empty());
}

#[test]
fn tags_block_scalar_ignored() {
    let result = tags("---\ntags: |\n  block scalar\n---\n");
    assert!(result.is_empty());
}

#[test]
fn tags_no_frontmatter() {
    let result = tags("No frontmatter here.\n");
    assert!(result.is_empty());
}

#[test]
fn tags_trimmed() {
    // Leading/trailing whitespace inside brackets or on list items is stripped.
    let result = tags("---\ntags: [  foo  ,  bar  ]\n---\n");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].name, "foo");
    assert_eq!(result[1].name, "bar");
}

#[test]
fn tags_inline_range() {
    // "---\ntags: [foo, bar]\n---\n"
    //  line 0: ---       (0..4)
    //  line 1: tags: [foo, bar]  (4..21)
    //          t=4 a=5 g=6 s=7 :=8 ' '=9 [=10 f=11 o=12 o=13 ,=14 ' '=15 b=16 a=17 r=18 ]=19 \n=20
    //  foo: line 1, chars 7–10
    //  bar: line 1, chars 12–15
    let result = tags("---\ntags: [foo, bar]\n---\n");
    assert_eq!(result[0].range, range((1, 7), (1, 10)));
    assert_eq!(result[1].range, range((1, 12), (1, 15)));
}

#[test]
fn tags_block_range() {
    // "---\ntags:\n  - foo\n  - bar\n---\n"
    //  line 0: ---         (0..4)
    //  line 1: tags:       (4..10)
    //  line 2:   - foo     (10..18)  foo at byte 14, col 4
    //  line 3:   - bar     (18..26)  bar at byte 22, col 4
    let result = tags("---\ntags:\n  - foo\n  - bar\n---\n");
    assert_eq!(result[0].range, range((2, 4), (2, 7)));
    assert_eq!(result[1].range, range((3, 4), (3, 7)));
}

// ── FrontmatterField extraction ───────────────────────────────────────────────

fn fields(content: &str) -> Vec<FrontmatterField> {
    parse(Path::new("note.md"), content)
        .frontmatter
        .map(|fm| fm.fields)
        .unwrap_or_default()
}

#[test]
fn fields_no_frontmatter() {
    assert!(fields("Just prose.\n").is_empty());
}

#[test]
fn fields_scalar_values() {
    // "---\nstatus: draft\nauthor: alice\n---\n"
    //  line 0: ---           (0..4)
    //  line 1: status: draft (4..18)
    //  line 2: author: alice (18..31)
    let result = fields("---\nstatus: draft\nauthor: alice\n---\n");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].key, "status");
    assert_eq!(result[0].value.as_deref(), Some("draft"));
    assert_eq!(result[1].key, "author");
    assert_eq!(result[1].value.as_deref(), Some("alice"));
}

#[test]
fn fields_empty_value() {
    // `key:` with nothing after colon → value: None
    let result = fields("---\nstatus:\n---\n");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].key, "status");
    assert!(result[0].value.is_none());
}

#[test]
fn fields_block_scalar_skipped() {
    let result = fields("---\nbody: |\n  multi\n---\n");
    assert_eq!(result.len(), 1);
    assert!(result[0].value.is_none());
}

#[test]
fn fields_inline_list_skipped() {
    let result = fields("---\ntags: [a, b]\n---\n");
    assert_eq!(result.len(), 1);
    assert!(result[0].value.is_none());
}

#[test]
fn fields_double_quoted_value() {
    // Quotes are stripped; value_range covers text inside quotes.
    let result = fields("---\ntitle: \"My Note\"\n---\n");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].value.as_deref(), Some("My Note"));
    // "---\ntitle: \"My Note\"\n"
    //  line 0: ---                   bytes 0..4
    //  line 1: title: "My Note"      bytes 4..21
    //  'M' is at byte 4 + len("title: \"") = 4+8 = 12
    let vr = result[0].value_range.expect("expected value_range");
    assert_eq!(vr.start, pos(1, 8));  // inside opening quote
    assert_eq!(vr.end,   pos(1, 15)); // before closing quote
}

#[test]
fn fields_single_quoted_value() {
    let result = fields("---\ntitle: 'My Note'\n---\n");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].value.as_deref(), Some("My Note"));
}

#[test]
fn fields_key_range_correct() {
    // "---\nstatus: draft\n---\n"
    //  line 1 starts at byte 4; "status" is chars 0–5 on line 1
    let result = fields("---\nstatus: draft\n---\n");
    let kr = result[0].key_range;
    assert_eq!(kr.start, pos(1, 0));
    assert_eq!(kr.end,   pos(1, 6)); // "status" is 6 chars
}

#[test]
fn fields_value_range_correct() {
    // "---\nstatus: draft\n---\n"
    //  line 1: "status: draft"  — "draft" starts at col 8
    let result = fields("---\nstatus: draft\n---\n");
    let vr = result[0].value_range.expect("expected value_range");
    assert_eq!(vr.start, pos(1, 8));
    assert_eq!(vr.end,   pos(1, 13)); // "draft" is 5 chars
}

#[test]
fn fields_title_and_status_both_extracted() {
    // title and tags go through the existing extractors, but fields also
    // captures them so schema validation works uniformly.
    let result = fields("---\ntitle: My Note\nstatus: draft\n---\n");
    let keys: Vec<&str> = result.iter().map(|f| f.key.as_str()).collect();
    assert!(keys.contains(&"title"), "expected title in fields");
    assert!(keys.contains(&"status"), "expected status in fields");
}
