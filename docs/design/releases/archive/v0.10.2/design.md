# v0.10.2 Design — Escaped Link Targets for Paths with Spaces

Covers the bug fixed in v0.10.2:

| Story | Type | Description                                                                     |
| ----- | ---- | ------------------------------------------------------------------------------- |
| #57   | Bug  | Links to files with spaces in the name are broken (autocomplete + quick action) |

---

## Goal

A writer whose vault contains files with spaces in their names (`My File.md`,
`Meeting Notes/2026 Plan.md`) can still link to them through completion or the
"Create note" quick action and get a link that actually resolves. Today knap
inserts the raw path into `](...)`, and a bare Markdown link destination
containing an unencoded space is invalid CommonMark. This mirrors the problem
`slug()` already solves for heading anchors, but file paths need a
non-destructive fix: wrapping the destination in angle brackets (`<...>`)
rather than stripping or percent-encoding it.

**Correction from the original draft of this design:** a bare destination
containing a space isn't merely truncated at the space — pulldown-cmark
doesn't recognize `[text](My File)` as a link event at all; the whole thing
is emitted as plain `Text`. That has two consequences beyond the two write
sites the issue names:

- The "Create note" quick action operates on `note.md_links`, which is
  populated purely from pulldown-cmark's event stream. Before this release, a
  hand-typed `[text](My File)` was invisible to it — no broken-link
  diagnostic, no quick action, nothing — even though the issue's own repro
  describes exactly that quick action firing. A **parser-side fallback scan**
  (`find_fallback_links()` in `src/parser/mod.rs`) was added to close this
  gap; see below.
- A link that _is_ syntactically valid because the user (or a previous
  version of knap) already wrapped it — `[text](<My File>)` — parses fine,
  but knap's own destination extraction re-slices the raw source between `(`
  and `)` rather than using pulldown-cmark's already-unwrapped `dest_url`, so
  `link.target` comes back as the literal `"<My File>"`, brackets included.
  Anything that reuses `link.target` (new file naming, re-escaping) has to
  unwrap it first or it double-wraps. See `index::unescape_link_target()`
  usage below.

Angle brackets were chosen over percent-encoding because pulldown-cmark parses
`<My File.md>` back into the raw destination string `"My File.md"` with no
decoding step required downstream. Percent-encoding (`My%20File.md`) is passed
through unchanged by pulldown-cmark's event parser — `index.resolve`, Go to
Definition, diagnostics, and rename all key off `link.target` verbatim, and
none of them decode `%XX` sequences. Adopting percent-encoding would require
teaching every one of those call sites to decode first, which is out of scope
for this fix. Angle brackets need no changes anywhere except the two write
sites named in the issue.

---

## Handler Changes

### Shared helper: `escape_link_target()`

New helper in `src/handlers.rs`, next to `slug()`:

```rust
/// Escape a path so it's safe to use as a Markdown link destination.
/// Wraps in angle brackets when the path contains characters that would
/// otherwise break or truncate a bare `](...)` destination: whitespace,
/// ASCII control characters, or parentheses. Any `<`, `>`, or `\` already
/// present are backslash-escaped so the bracketed form stays valid.
fn escape_link_target(target: &str) -> String {
    if !crate::parser::link_destination_needs_wrapping(target) {
        return target.to_string();
    }
    let mut escaped = String::with_capacity(target.len() + 2);
    escaped.push('<');
    for c in target.chars() {
        if c == '<' || c == '>' || c == '\\' {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped.push('>');
    escaped
}
```

Paths with no special characters are returned unchanged — this keeps every
existing completion and quick-action insertion byte-for-byte identical to
today's output, which is why none of the current tests need to change.

The wrapping predicate itself (`link_destination_needs_wrapping()`) lives in
`src/parser/mod.rs`, not `src/handlers.rs`, because the parser-side fallback
scan (below) needs the exact same condition to decide which unparsed
`[text](...)` spans to recover as links. `escape_link_target()` calls into it
rather than duplicating the character class.

### Parser fallback: recovering links pulldown-cmark won't parse

`src/parser/mod.rs` gains `find_fallback_links()`, run once after the normal
pulldown-cmark pass over `extract_body_elements()`'s event stream. It scans
the raw body text for `[text](target)` / `![alt](target)` spans that:

- weren't already captured by the pulldown-cmark pass (tracked via the byte
  ranges of links found in the main loop),
- aren't inside a fenced code block (checked against the same `code_fences`
  the main loop already collects), and
- have a destination that `link_destination_needs_wrapping()` flags — i.e.
  exactly the destinations pulldown-cmark refuses to parse as a bare
  (unwrapped) link.

It balances parentheses by hand (honoring `\`-escapes) to find the closing
`)`, since it can't rely on pulldown-cmark's own bracket matching for spans
pulldown-cmark didn't parse. The destination-splitting logic (target vs.
`#anchor`, and their ranges) was factored out of the main pulldown-cmark
handler into a shared `split_link_destination()` helper so both paths produce
identical `MarkdownLink` shapes. Recovered links are merged into `md_links`
and the list is sorted by position for deterministic ordering.

Critically, `target` for a fallback-recovered link is the **raw, unwrapped**
text (e.g. `"My File"`), matching what a normal parse would have produced had
the destination been valid. This is what makes `[text](My File)` — the
literal repro in issue #57 — visible to `handle_code_actions` as a broken
link at all, and what makes `code_actions_create_note_target_with_space_adds_text_edit`
(below) pass end-to-end instead of only exercising the write side.

### `handle_completion` (`textDocument/completion`)

Only the **terminal** insertions — ones that complete a full link
destination — are escaped. The directory-drill-down items are not, because
their `new_text` is an intentionally incomplete destination (e.g. `"sub/"`)
that the user keeps typing after, and re-triggering completion depends on
`check_dir_trigger` re-reading the raw, unwrapped text between `](` and the
cursor. Wrapping a partial destination in `<` would break that re-read on the
very next keystroke.

| Item kind (src/handlers.rs)                        | `new_text` today           | `new_text` after fix            |
| -------------------------------------------------- | -------------------------- | ------------------------------- |
| Folder item (tier 0, ~line 614–625)                | `full_rel` (raw, `"sub/"`) | unchanged — **not** escaped     |
| File item, immediate child (tier 1, ~line 631–656) | `full_rel` (raw)           | `escape_link_target(&full_rel)` |
| File item, global (tier 2, ~line 660–692)          | `full_rel` (raw)           | `escape_link_target(&full_rel)` |

`filter_text`, `label`, and `detail` are unaffected — they're display/matching
fields, never inserted into the document, so they stay in raw, human-readable
form.

### `handle_code_actions` (`textDocument/codeAction`) — "Create note"

Today the **Create note** quick action only emits a `CreateFile` resource
operation; the broken link text in the document is left exactly as typed,
still invalid. The fix adds a second operation to the same `WorkspaceEdit`
that rewrites the link's target text to the escaped form, when escaping
changes anything:

```rust
ResolvedLink::Broken => {
    let clean_target = index::unescape_link_target(&link.target);
    let new_path = new_note_path(&clean_target, &path, config);
    let mut ops = vec![DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
        uri: path_to_uri(&new_path),
        options: Some(CreateFileOptions { ignore_if_exists: Some(true), overwrite: None }),
        annotation_id: None,
    }))];

    let escaped_target = escape_link_target(&clean_target);
    if escaped_target != link.target {
        ops.push(DocumentChangeOperation::Edit(TextDocumentEdit {
            text_document: OptionalVersionedTextDocumentIdentifier {
                uri: path_to_uri(&path),
                version: None,
            },
            edits: vec![OneOf::Left(TextEdit {
                range: link.target_range,
                new_text: escaped_target,
            })],
        }));
    }

    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
        title: "Create note".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        edit: Some(WorkspaceEdit {
            document_changes: Some(DocumentChanges::Operations(ops)),
            ..Default::default()
        }),
        ..Default::default()
    }));
}
```

Two adjustments from the original draft, both needed because `link.target`
isn't always the clean path it looks like:

- **Unescape before reuse.** When the existing broken link is already
  wrapped (`[text](<My File>)`, valid CommonMark, but `My File.md` doesn't
  exist), `link.target` is the literal string `"<My File>"` — see the
  "Correction" note above. Feeding that straight into `new_note_path` or
  `escape_link_target` would create a file named `<My File>.md` and
  double-wrap the re-escaped text (`<\<My File\>>`). `clean_target` calls
  `index::unescape_link_target()` (promoted from a private helper in
  `src/index/mod.rs` to `pub(crate)`, since it's now shared by `index::resolve()`
  and this call site) to strip any existing `<...>` wrapping first. For a
  target that was never wrapped (the `find_fallback_links()` case), this is a
  no-op.
- **`new_note_path` now appends `.md` when the target has no extension.**
  `[text](My File)` has no extension at all — the old behavior would have
  created a file literally named `My File` (no `.md`). `new_note_path` now
  calls `.with_extension("md")` when `Path::extension()` returns `None`.
  This is a pre-existing gap unrelated to escaping, surfaced only because
  fixing the parser gap above made extension-less broken links reachable for
  the first time.

`link.target_range` (from `src/parser/mod.rs`) already covers just the path
portion of the destination, excluding any `#anchor`, so anchored broken links
(`[text](missing file#x)`) get only their path segment rewritten.

`OneOf` and `TextDocumentEdit`/`OptionalVersionedTextDocumentIdentifier` are
newly imported from `lsp_types` in `src/handlers.rs`.

---

## Testing

### Unit tests (`src/handlers.rs`)

| Test                                                                 | What it verifies                                                                                                                                                                                                                             |
| -------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `escape_link_target_no_special_chars_unchanged`                      | A plain relative path is returned unchanged                                                                                                                                                                                                  |
| `escape_link_target_space_wraps_in_angle_brackets`                   | `"My File.md"` → `"<My File.md>"`                                                                                                                                                                                                            |
| `escape_link_target_parens_wrap_in_angle_brackets`                   | `"file (1).md"` → `"<file (1).md>"`                                                                                                                                                                                                          |
| `escape_link_target_escapes_inner_angle_brackets_when_wrapping`      | `"My <File>.md"` (space triggers wrapping) → inner `<`/`>` are backslash-escaped                                                                                                                                                             |
| `completion_file_item_with_space_wraps_target`                       | Tier-1 file completion for a filename containing a space produces `new_text` wrapped in `<...>`                                                                                                                                              |
| `completion_global_item_with_space_wraps_target`                     | Tier-2 global completion for a filename containing a space produces wrapped `new_text`                                                                                                                                                       |
| `completion_folder_item_with_space_not_wrapped`                      | Tier-0 folder completion for a directory containing a space is **not** wrapped                                                                                                                                                               |
| `completion_file_item_without_space_unchanged`                       | Regression: a filename with no special characters still inserts the raw path                                                                                                                                                                 |
| `code_actions_create_note_target_with_space_adds_text_edit`          | End-to-end repro of issue #57: raw `[link](My File)` (only visible at all thanks to the parser fallback) produces a `CreateFile` op targeting the URL-encoded `My%20File.md` and a `TextDocumentEdit` rewriting the link text to `<My File>` |
| `code_actions_create_note_already_wrapped_target_not_double_escaped` | An already-valid, already-wrapped broken link (`[link](<My File>)`) creates the right file and does **not** re-wrap the link text                                                                                                            |
| `code_actions_create_note_target_without_space_no_text_edit`         | Regression: a broken link with no special characters produces only the `CreateFile` op, no extra edit                                                                                                                                        |

### Unit tests (`src/parser/tests.rs`)

| Test                                              | What it verifies                                                                                             |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `md_link_fallback_bare_space_target`              | `[link](My File)` is recovered as a link with `target: "My File"`, matching what a valid parse would produce |
| `md_link_fallback_bare_paren_target`              | `[link](file (1).md)` — balanced-but-unescaped parens are also recovered                                     |
| `md_link_fallback_image_with_space`               | `![alt](My Image.png)` — the fallback also covers images, matching `is_image` handling elsewhere             |
| `md_link_fallback_with_anchor`                    | `[link](My File#section)` splits into `target: "My File"` / `anchor: "section"`                              |
| `md_link_fallback_does_not_duplicate_valid_links` | A normal valid link is not double-counted between the pulldown-cmark pass and the fallback scan              |
| `md_link_fallback_skips_fenced_code`              | A space-containing destination inside a fenced code block is not recovered as a link                         |
| `md_link_fallback_no_bracket_no_link`             | Plain prose containing stray `[...]`/`(...)` with no `](` boundary is not matched                            |
| `md_link_fallback_multiple_on_one_line`           | Two independent fallback links on the same line are both recovered with correct ranges                       |

No integration tests are needed — the fix is confined to a handful of pure
functions (`handle_completion`, `handle_code_actions`, `find_fallback_links`)
already covered end-to-end by existing integration coverage of completion,
code actions, and parsing; the unit tests above are sufficient to pin the new
behavior.
