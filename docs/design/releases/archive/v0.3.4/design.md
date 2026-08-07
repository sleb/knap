# v0.3.4 Design — Rename Placeholder Mismatch

## Stories

| Story | Type | Description                                                       |
| ----- | ---- | ----------------------------------------------------------------- |
| #3    | Bug  | Rename dialog does not appear for headings with inline formatting |

---

## Goal

When a heading contains inline Markdown formatting (e.g. `## v0.3 _(released 2026-05-16)_`),
the rename dialog silently never appears in the editor, even after the v0.3.3 disk-fallback
fix. The fix makes `prepareRename` return a placeholder that equals the raw source text at
`text_range`, satisfying the LSP spec requirement that editors SHOULD validate
`placeholder == text-at-range`.

---

## Bug

### Symptom

`handle_prepare_rename` returns `PrepareRenameResponse::RangeWithPlaceholder` with:

- `range`: the `text_range` from the parser — covers raw source (e.g. `"v0.3 — Heading Navigation & Anchors _(released 2026-05-16)_"`)
- `placeholder`: `heading.text.clone()` — pulldown-cmark rendered text, inline formatting stripped (e.g. `"v0.3 — Heading Navigation & Anchors (released 2026-05-16)"`)

These two values differ when the heading contains inline markup (`_..._`, `**...**`, `` `...` ``, etc.).
Per the LSP spec, editors SHOULD validate that `placeholder == text-at-range`. When they do,
they refuse to show the rename dialog.

Headings without inline formatting (like `# My Heading`) are unaffected because rendered
text and raw text are identical.

### Root cause

`parser::parse` populates `Heading.text` from pulldown-cmark events, which deliver rendered
text with all inline markup stripped. The `text_range` correctly spans the raw source.
`heading.text` and the raw text at `heading.text_range` are only identical for plain-text headings.

---

## Fix

### Strategy: extract raw text from `note.content`

`Note.content` holds the raw source string. Given `heading.text_range` (in LSP coordinates),
extract the raw substring:

```rust
let placeholder = {
    let line_text = note.content.lines()
        .nth(heading.text_range.start.line as usize)
        .unwrap_or("");
    let start = utf16_to_byte_offset(line_text, heading.text_range.start.character);
    let end   = utf16_to_byte_offset(line_text, heading.text_range.end.character);
    line_text[start..end].to_string()
};
```

`utf16_to_byte_offset` already exists in `src/handlers.rs` (line 103).

This extraction is safe because:

- `heading.text_range` is always on a single line (a Markdown heading cannot span lines).
- `note.content` is the raw source from which the heading was parsed; its line/char offsets
  are directly comparable to `text_range`.
- `utf16_to_byte_offset` handles multi-byte characters and multi-codepoint characters
  correctly by counting UTF-16 code units.

No changes to the parser, NoteIndex, or any other module.

---

## Testing

| Test                                     | What it verifies                                                                                           |
| ---------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `prepare_rename_placeholder_is_raw_text` | Heading with `_..._` italic → placeholder is raw `"My _Fancy_ Heading"`, not rendered `"My Fancy Heading"` |

The existing `prepare_rename_on_heading_returns_text_range_and_placeholder` test covers
plain-text headings (rendered == raw) and continues to pass unchanged.
