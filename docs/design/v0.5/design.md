# v0.5 Design — Tags

Covers the stories in the v0.5 release:

| Story | Feature                                                  |
| ----- | -------------------------------------------------------- |
| US-14 | Frontmatter `tags:` completions from workspace tag index |
| US-15 | Find References on a tag value → all files using it      |
| US-13 | Go to Definition on a tag → all files using it           |

**Out of scope for v0.5:** tag validation or diagnostics (v0.8), a dedicated
tags panel, tag rename, nested tags (`parent/child`), tag-based filtering in
workspace symbols. The `tags` key is treated as a flat list of strings.

---

## Frontmatter tag syntax

Three tag forms are supported (the same subset Obsidian uses):

**Inline list:**

```yaml
---
tags: [foo, bar, baz]
---
```

**Block list:**

```yaml
---
tags:
  - foo
  - bar
---
```

**Single tag (bare scalar — no brackets, no dash):**

```yaml
---
tags: productivity
---
```

Tag names are trimmed of leading/trailing whitespace. Empty strings after
trimming are discarded. The inline form strips the surrounding `[` and `]` and
splits on `,`. The block form treats each `- value` line as one tag. The bare
scalar form treats the entire value (after `tags:`) as a single tag.

Block scalar values (`|`, `>`) on the `tags:` line are treated as absent (same
as for `title:`).

Tags are case-preserving in storage but **matched case-insensitively** across
the index and in all handler lookups.

---

## Data structures

### Tag (new)

```rust
pub struct Tag {
    pub name:  String,   // raw tag text, as written (e.g. "MyTag")
    pub range: LspRange, // the tag name's span in the full file (for cursor hit-testing)
}
```

### Frontmatter (extended)

```rust
pub struct Frontmatter {
    pub title: Option<String>,
    pub tags:  Vec<Tag>,  // new; empty when `tags:` key is absent or has no values
}
```

---

## Parser changes

### New function: `extract_tags`

Add:

```rust
fn extract_tags(content: &str, line_index: &LineIndex) -> Vec<Tag>
```

`content` is the full file content. `line_index` is the full-file `LineIndex`
already created by `parse()`. The function:

1. Returns `vec![]` if `content` does not start with `---\n` (no frontmatter).
2. Locates the frontmatter block (same two-branch check as
   `frontmatter_body_offset`: `\n---\n` or `\n---` at end-of-input). Returns
   `vec![]` on unclosed frontmatter.
3. Iterates over the lines of the frontmatter block, tracking their byte offsets
   within the full file.
4. Finds the first line matching `^tags:\s*(.*)`.
5. Parses the value according to which form is present:
   - **Inline list**: value starts with `[` → strip `[…]`, split on `,`, trim
     each element.
   - **Block scalar**: value starts with `|` or `>` → return `vec![]` (ignored).
   - **Bare scalar** (any other non-empty value) → single tag.
   - **Empty value**: switch to block-list mode, scan subsequent lines in the
     block for `^\s*-\s+(.+)`, collecting each match as a tag. Stop at the first
     line that is not a list item and not empty (i.e. another key).
6. For each extracted tag name, compute its byte range within the full file and
   call `line_index.range(start..end)` to get the `LspRange`. Strip leading/
   trailing whitespace from the tag name before computing the range so that
   `name` and `range` are consistent.
7. Discard tags whose trimmed name is empty.

### `parse()` call site

`parse()` already creates `line_index` before calling `extract_wiki_links` and
`extract_headings`. `extract_tags` slots in alongside those calls:

```rust
let tags       = extract_tags(content, &line_index);
let frontmatter = extract_frontmatter(content)
    .map(|fm| Frontmatter { title: fm.title, tags });
```

This keeps `extract_frontmatter`'s signature unchanged (no `LineIndex` required
for title-only extraction).

### `knap parse` CLI output

Extend the CLI output to print tags. When a note has tags, print each on its
own line after the title line, e.g.:

```
stem:  my-note
title: My Note
tags:  [foo, bar]
links: ...
```

---

## Index changes

### New field: `by_tag`

```rust
pub struct NoteIndex {
    // ...existing fields...
    by_tag: HashMap<String, Vec<PathBuf>>,  // lowercase tag → paths
}
```

Tag lookups are case-insensitive; `by_tag` stores lowercase keys. The original
casing is preserved in `Note.frontmatter.tags[*].name` and used for display
(e.g. completion labels).

### Maintaining `by_tag`

**`index(note)`**: after storing the note, for each `tag` in
`note.frontmatter.as_ref().map_or(&[], |fm| fm.tags.as_slice())`, insert
`note.path` into `by_tag.entry(tag.name.to_lowercase()).or_default()`.

**`remove_internal(path)`**: for each tag of the removed note, remove `path`
from the corresponding `by_tag` entry. Drop the entry when empty.

`by_tag` maintenance happens in `index()` and `remove_internal()` only — the
same two mutation points that maintain `by_stem` and `by_filename`.

### New query methods

```rust
/// Iterator over all distinct tag names (lowercase) in the index.
pub fn all_tags(&self) -> impl Iterator<Item = &str>

/// All notes that carry the given tag (case-insensitive match).
pub fn notes_by_tag(&self, tag: &str) -> Vec<&Note>
```

These are pure reads with no filesystem I/O.

---

## Completion changes (US-14)

### Trigger detection

Add a helper:

```rust
fn check_tag_trigger(content: &str, pos: Position) -> bool
```

Algorithm:

1. The file must begin with `---\n`. Find the closing `---` line. The cursor
   must be inside the frontmatter block (line > 0, line < closing-`---` line).
2. Get the text of `pos.line`. Let `line_trimmed = line.trim_start()`.
3. **Inline / bare-scalar trigger**: if `line_trimmed.starts_with("tags:")`,
   return `true`. (Any cursor position on the `tags:` line triggers tag
   completion, regardless of whether the user is before or after `[`.)
4. **Block-list trigger**: if the text up to the cursor on the current line
   matches `^\s*-\s*` (i.e. is a YAML list item), scan backwards from
   `pos.line - 1` through the frontmatter lines:
   - Skip empty lines.
   - If a line matches `^tags:\s*$` (key with no value, meaning a block list
     follows), return `true`.
   - If a line matches a different YAML key (`^[a-zA-Z].*:`), stop and return
     `false`.
   - If the scan reaches line 1 without finding `tags:`, return `false`.

In `handle_completion`, after the existing `check_trigger` (wiki-link) test,
add:

```rust
if check_tag_trigger(&note.content, pos) {
    return tag_completions(index);
}
```

Wiki-link and tag triggers are mutually exclusive in practice (`[[` never
appears in the frontmatter area).

### Tag completion items

```rust
fn tag_completions(index: &NoteIndex) -> Vec<CompletionItem> {
    index.all_tags().map(|tag| CompletionItem {
        label:       tag.to_string(),
        kind:        Some(CompletionItemKind::VALUE),
        insert_text: Some(tag.to_string()),
        ..Default::default()
    }).collect()
}
```

No `filter_text` override is needed — `label` and `insert_text` are the same
lowercase tag name, and editors fuzzy-match against `label` by default.

---

## Go to Definition for tags (US-13)

`handle_definition` currently checks for a wiki-link at the cursor. Add a
second check when no wiki-link is found:

```rust
// 2. Tag in frontmatter at cursor position.
if let Some(tag) = find_tag_at_position(note, pos) {
    return Some(definition_locations_for_tag(&tag.name, index));
}
```

The LSP `textDocument/definition` response type supports `GotoDefinitionResponse::Array(Vec<Location>)`. When the cursor is on a tag, return one `Location` per note that carries that tag:

```rust
fn definition_locations_for_tag(tag: &str, index: &NoteIndex) -> GotoDefinitionResponse {
    let locations = index.notes_by_tag(tag)
        .iter()
        .filter_map(|note| {
            // Point to the specific tag range within each note.
            let tag_range = note.frontmatter.as_ref()?
                .tags.iter()
                .find(|t| t.name.to_lowercase() == tag.to_lowercase())?
                .range;
            Some(Location { uri: path_to_uri(&note.path), range: tag_range })
        })
        .collect();
    GotoDefinitionResponse::Array(locations)
}
```

The existing `handle_definition` currently returns `Option<Location>` (a single
location). The return type must change to
`Option<GotoDefinitionResponse>` so it can return multiple locations for tags
while preserving single-location behaviour for wiki-links.

### `find_tag_at_position`

```rust
fn find_tag_at_position(note: &Note, pos: Position) -> Option<&Tag> {
    note.frontmatter.as_ref()?
        .tags.iter()
        .find(|t| contains(t.range, pos))
}
```

Uses the existing `contains` helper.

---

## Find References for tags (US-15)

`handle_references` currently checks for a wiki-link at the cursor. Add a
parallel path for tags:

```rust
// 2. Tag in frontmatter at cursor position.
if let Some(tag) = find_tag_at_position(note, pos) {
    return locations_for_tag(&tag.name, index);
}
```

```rust
fn locations_for_tag(tag: &str, index: &NoteIndex) -> Vec<Location> {
    index.notes_by_tag(tag)
        .iter()
        .filter_map(|note| {
            let tag_range = note.frontmatter.as_ref()?
                .tags.iter()
                .find(|t| t.name.to_lowercase() == tag.to_lowercase())?
                .range;
            Some(Location { uri: path_to_uri(&note.path), range: tag_range })
        })
        .collect()
}
```

Definition and References return the same set of locations for tags. (Tags have
no meaningful "definition site" — every usage is equivalent. Go to Definition is
an intuitive gesture for "show me all notes with this tag".)

---

## Capability advertisement

No new `ServerCapabilities` fields required. `textDocument/completion`,
`textDocument/definition`, and `textDocument/references` were already advertised
in v0.1. The `definition` response type change (single → array) is handled at
the serialisation layer; `GotoDefinitionResponse` already encodes both forms.

---

## Testing

### Unit tests

**Parser (`src/parser/tests.rs`):**

| Test                        | What it verifies                                               |
| --------------------------- | -------------------------------------------------------------- |
| `tags_inline_list`          | `tags: [foo, bar]` → `[Tag("foo", …), Tag("bar", …)]`          |
| `tags_block_list`           | `tags:\n  - foo\n  - bar` → two tags                           |
| `tags_bare_scalar`          | `tags: productivity` → single tag                              |
| `tags_absent`               | Frontmatter with no `tags:` key → `tags: vec![]`               |
| `tags_empty_value`          | `tags:` with no value and no list items → `tags: vec![]`       |
| `tags_block_scalar_ignored` | `tags: \|…` → `tags: vec![]`                                   |
| `tags_inline_range`         | `tags: [foo, bar]` → `foo` range is col 8–11, `bar` is correct |
| `tags_block_range`          | block list → each tag's range covers its name text             |
| `tags_no_frontmatter`       | Content without `---` block → `tags: vec![]`                   |
| `tags_trimmed`              | ` -` with trailing/leading spaces → trimmed name, no empty     |

**Index (`src/index/tests.rs`):**

| Test                            | What it verifies                                          |
| ------------------------------- | --------------------------------------------------------- |
| `index_by_tag_populated`        | Indexing a note with tags → `by_tag` has expected entries |
| `index_by_tag_removed`          | Removing note → `by_tag` entry cleared                    |
| `notes_by_tag_case_insensitive` | `notes_by_tag("FOO")` finds notes tagged `"foo"`          |
| `all_tags_distinct`             | Two notes with overlapping tags → each tag listed once    |

**Handlers (`src/handlers.rs` inline tests):**

| Test                                   | What it verifies                                                 |
| -------------------------------------- | ---------------------------------------------------------------- |
| `completion_tag_inline_trigger`        | Cursor on `tags: [` line → tag completions returned              |
| `completion_tag_block_trigger`         | Cursor on ` -` line after `tags:` block → tag completions        |
| `completion_tag_no_trigger_off_tags`   | Cursor on `title:` line → no tag completions                     |
| `completion_tag_no_trigger_body`       | Cursor below frontmatter → wiki-link trigger used, not tags      |
| `definition_tag_returns_all_locations` | Cursor on `foo` tag → Locations for all notes tagged `foo`       |
| `definition_tag_case_insensitive`      | `Foo` tag → matches notes with `foo`                             |
| `references_tag_returns_all_locations` | Same as definition test; verify both handlers return same result |
| `tag_at_position_miss`                 | Cursor on non-tag frontmatter text → no tag found                |

### Integration tests

**`tests/tags.rs` (new file):**

| Test                           | What it verifies                                                      |
| ------------------------------ | --------------------------------------------------------------------- |
| `tag_completion_round_trip`    | Index has notes with tags; completion at `tags: [` → tags in list     |
| `tag_definition_round_trip`    | Cursor on a tag → `definition` response lists all files with that tag |
| `tag_references_round_trip`    | Cursor on a tag → `references` response lists all files with that tag |
| `tag_definition_no_tag_at_pos` | Cursor on plain text → `definition` falls through to normal wiki-link |
