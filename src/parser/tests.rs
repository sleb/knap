use std::path::Path;

use lsp_types::{Position, Range};
use super::{extract_frontmatter, FrontmatterField, Frontmatter, Heading, LineIndex, MarkdownLink, parse, Tag, WikiLink};

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
fn frontmatter_wiki_links_not_scanned() {
    // [[hidden]] lives inside the frontmatter block and must not be collected.
    // [[real]] lives in the body and must be collected.
    let content = "---\ntitle: foo\n[[hidden]]\n---\nBody [[real]].\n";
    let note = parse(Path::new("note.md"), content);
    assert_eq!(note.wiki_links.len(), 1);
    assert_eq!(note.wiki_links[0].stem, "real");
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

// ── Markdown links ────────────────────────────────────────────────────────────

fn md_links(content: &str) -> Vec<MarkdownLink> {
    parse(Path::new("note.md"), content).md_links
}

#[test]
fn md_link_basic() {
    let result = md_links("[text](url)");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].text, "text");
    assert_eq!(result[0].target, "url");
    assert_eq!(result[0].is_image, false);
}

#[test]
fn md_link_image() {
    let result = md_links("![alt text](img.png)");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].text, "alt text");
    assert_eq!(result[0].target, "img.png");
    assert_eq!(result[0].is_image, true);
}

#[test]
fn md_link_range() {
    // "[text](url)" — 11 bytes at column 0; range should span the full construct.
    let result = md_links("[text](url)");
    assert_eq!(result[0].range, range((0, 0), (0, 11)));
}

#[test]
fn md_link_in_fenced_code_ignored() {
    let content = "```\n[hidden](url)\n```\n";
    assert!(md_links(content).is_empty());
}

#[test]
fn wiki_link_range_after_frontmatter() {
    // "---\ntitle: foo\n---\n[[note]]\n"
    // frontmatter_body_offset = 4 ("---\n") + 10 ("title: foo") + 5 ("\n---\n") = 19
    // "[[note]]" is at body byte 0..8, which maps to full-file line 3, cols 0–8.
    let content = "---\ntitle: foo\n---\n[[note]]\n";
    let note = parse(Path::new("note.md"), content);
    assert_eq!(note.wiki_links.len(), 1);
    assert_eq!(note.wiki_links[0].range, range((3, 0), (3, 8)));
    assert_eq!(note.wiki_links[0].inner_range, range((3, 2), (3, 6)));
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
