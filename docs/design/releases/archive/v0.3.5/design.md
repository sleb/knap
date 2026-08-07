# v0.3.5 Design — LSP Range Correctness for Multi-byte Characters

## Stories

| Story | Type | Description                                                                                     |
| ----- | ---- | ----------------------------------------------------------------------------------------------- |
| #4    | Bug  | Rename dialog does not appear for headings with multi-byte characters or trailing inline markup |

---

## Goal

Fix two bugs in `LineIndex` / `extract_body_elements` that together prevent the
rename dialog from appearing for headings like
`## v0.3 — Heading Navigation & Anchors _(released 2026-05-16)_`:

1. `text_range.end.character` is reported as 63 (a byte offset), which in UTF-16
   is position 61 (AT the closing `_`), then the byte-as-UTF-16 bug inflates that
   to 63 — one position past the newline. Editors that validate the range reject
   it and show no dialog.

2. Even after correcting for UTF-16, `text_range` still missed the closing `_`
   because `last_text_end` tracked the end of the last pulldown-cmark Text event.
   For headings ending with inline markup (`_..._`), the last Text event ends just
   before the closing `_`, so the `_` itself is not included in the range.

---

## Bug 1 — `LineIndex.position()` returns byte offsets, not UTF-16

### Root cause

```rust
pub fn position(&self, byte_offset: usize) -> Position {
    let line = self.line_starts.partition_point(|&s| s <= byte_offset) - 1;
    let character = byte_offset - self.line_starts[line]; // byte offset within line
    Position { line: line as u32, character: character as u32 }
}
```

LSP requires `character` to be a UTF-16 code unit offset. For pure ASCII content
the two are identical, so the bug was latent. The em dash (`—`, U+2014) is
3 UTF-8 bytes but 1 UTF-16 code unit; any byte position after it is inflated by
2 in the parser's output.

### Fix

Store the content string in `LineIndex` and compute the UTF-16 offset by summing
`char.len_utf16()` for each character from `line_start` to `byte_offset`:

```rust
let character = self.content[line_start..byte_offset]
    .chars()
    .map(|c| c.len_utf16() as u32)
    .sum();
```

---

## Bug 2 — `text_range` end misses trailing inline markup

### Root cause

`text_range` end was set from `last_text_end`, the `byte_range.end` of the last
pulldown-cmark `Text` event inside the heading. pulldown-cmark text events cover
only the rendered text content — they exclude surrounding markup characters like
the `_` in `_..._`. For `## ... _(released)_`, the last Text event covers
`(released)` and ends at the byte of the closing `_`, so the `_` is not in the
range.

### Fix

Compute `text_range.end` from the `End(Heading)` event's `byte_range.end` minus
one (to exclude the trailing `\n`), rather than from `last_text_end`:

```rust
let text_end = if byte_range.end > 0
    && content.as_bytes().get(byte_range.end - 1) == Some(&b'\n')
{
    byte_range.end - 1
} else {
    byte_range.end
};
```

This covers the full raw heading text — including any trailing markup characters —
up to but not including the newline. Headings at EOF (no trailing newline) are
handled by the `else` branch.

---

## Combined effect for `## v0.3 — Heading Navigation & Anchors _(released 2026-05-16)_`

After both fixes:

| Field                        | Before                                                 | After                                             |
| ---------------------------- | ------------------------------------------------------ | ------------------------------------------------- |
| `text_range.start.character` | 3 (byte = UTF-16, correct)                             | 3                                                 |
| `text_range.end.character`   | 63 (byte of closing `_`, wrong UTF-16)                 | 62 (UTF-16, exclusive, includes `_`)              |
| placeholder                  | `"v0.3 — ... _(released)_"` (correct after v0.3.4 fix) | same                                              |
| text at range (editor reads) | one past the newline (invalid)                         | `"v0.3 — ... _(released)_"` (matches placeholder) |

---

## Scope

This fix corrects ALL LSP positions for files with multi-byte characters (link
ranges, anchor ranges, frontmatter field ranges). Previously those were also
byte-based and therefore wrong for lines with non-ASCII characters, though the
impact was not visible in the tested workflows.

---

## Testing

| Test                                             | What it verifies                    |
| ------------------------------------------------ | ----------------------------------- |
| `line_index_utf16_multibyte` (parser)            | em dash byte offset → UTF-16 column |
| `heading_text_range_trailing_italic` (parser)    | closing `_` included in text_range  |
| `heading_text_range_with_em_dash_utf16` (parser) | UTF-16 positions after em dash      |
