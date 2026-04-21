# v0.5 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                                | Status | Notes |
| --------------------------------------------------- | ------ | ----- |
| 1 — Parser: extract tags with ranges                | Done   |       |
| 2 — Index: tag index + query methods                | Done   |       |
| 3 — Completion: tag trigger (US-14)                 | Done   |       |
| 4 — Definition + References for tags (US-13, US-15) | Done   |       |
| 5 — Integration tests + CLI output                  | Done   |       |

---

## Step 1 — Parser: extract tags with ranges

Extend `Frontmatter` to carry tags. Add `Tag` and `extract_tags`. This is the
foundation for every subsequent step.

**Deliverables:**

- `Tag { pub name: String, pub range: LspRange }` added to `src/parser/mod.rs`
- `Frontmatter` gains `pub tags: Vec<Tag>` (default `vec![]`)
- `fn extract_tags(content: &str, line_index: &LineIndex) -> Vec<Tag>` (private)
  — parses frontmatter for the `tags:` key and returns one `Tag` per value,
  with ranges computed via `line_index.range()`; supports inline list
  `[a, b]`, block list (`- a` / `- b`), and bare scalar forms; block scalars
  (`|`, `>`) and missing keys return `vec![]`
- `parse()` calls `extract_tags(content, &line_index)` and folds the result
  into the `Frontmatter` returned by `extract_frontmatter`, producing a single
  `Option<Frontmatter>` on `Note`

**Unit tests** (`src/parser/tests.rs`):

| Test                        | What it verifies                                                  |
| --------------------------- | ----------------------------------------------------------------- |
| `tags_inline_list`          | `tags: [foo, bar]` → two tags with correct names                  |
| `tags_block_list`           | `tags:\n  - foo\n  - bar` → two tags                              |
| `tags_bare_scalar`          | `tags: productivity` → single tag                                 |
| `tags_absent`               | Frontmatter with no `tags:` key → `tags: vec![]`                  |
| `tags_empty_value`          | `tags:` with no list items → `tags: vec![]`                       |
| `tags_block_scalar_ignored` | `tags: \|…` → `tags: vec![]`                                      |
| `tags_inline_range`         | `tags: [foo, bar]` → `foo` range and `bar` range are correct      |
| `tags_block_range`          | block list → each tag range covers only the name text             |
| `tags_no_frontmatter`       | No `---` block → `note.frontmatter == None` (unchanged behaviour) |
| `tags_trimmed`              | Tags with surrounding whitespace → trimmed name, none discarded   |

> **Manual checkpoint:** `cargo run -- parse <file>` on a file with
> `tags: [foo, bar]` in its frontmatter should print all tags. A file with no
> frontmatter or no `tags:` key should show no tags. Existing title, wiki-link,
> and heading output must be unaffected.

---

## Step 2 — Index: tag index and query methods

Add `by_tag` to `NoteIndex` and expose the two query methods needed by handlers.

**Deliverables:**

- `by_tag: HashMap<String, Vec<PathBuf>>` field on `NoteIndex` (lowercase keys)
- `index(note)` populates `by_tag` for each tag on the note; existing notes are
  cleaned up via `remove_internal` before re-indexing (already the case)
- `remove_internal(path)` removes the path from each `by_tag` entry for the
  removed note's tags; drops empty entries
- `pub fn all_tags(&self) -> impl Iterator<Item = &str>` — distinct lowercase
  tag names across all indexed notes
- `pub fn notes_by_tag(&self, tag: &str) -> Vec<&Note>` — all notes carrying
  the given tag (case-insensitive lookup)

**Unit tests** (`src/index/tests.rs`):

| Test                            | What it verifies                                             |
| ------------------------------- | ------------------------------------------------------------ |
| `index_by_tag_populated`        | Indexing a note with tags → `all_tags()` returns those tags  |
| `index_by_tag_removed`          | Removing a note → its tags disappear from `all_tags()`       |
| `notes_by_tag_case_insensitive` | `notes_by_tag("FOO")` finds notes tagged `"foo"`             |
| `all_tags_distinct`             | Two notes sharing a tag → tag appears once in `all_tags()`   |
| `index_replace_updates_tags`    | Re-indexing a note with changed tags → old tags gone, new in |

> **Manual checkpoint:** none needed here — tag data is internal and not yet
> surfaced via any LSP capability. The unit tests are the full checkpoint for
> this step.

---

## Step 3 — Completion: tag trigger (US-14)

Add tag completion items when the cursor is in a `tags:` context within the
frontmatter.

**Deliverables:**

- `fn check_tag_trigger(content: &str, pos: Position) -> bool` (private in
  `src/handlers.rs`):
  - Returns `false` if the file has no frontmatter or the cursor is outside it
  - Returns `true` when the cursor is on the `tags:` line (any position)
  - Returns `true` when the cursor is on a `- ` list item that follows a bare
    `tags:` key (no value on same line) within the frontmatter block
- `fn tag_completions(index: &NoteIndex) -> Vec<CompletionItem>` — one
  `CompletionItem` per `index.all_tags()`, kind `VALUE`, label and insert_text
  are the lowercase tag name
- `handle_completion` checks `check_tag_trigger` after the existing
  `check_trigger` (wiki-link) check; returns tag completions when triggered

**Unit tests** (`src/handlers.rs` inline):

| Test                              | What it verifies                                              |
| --------------------------------- | ------------------------------------------------------------- |
| `completion_tag_inline_trigger`   | Cursor on `tags: [foo, ` line → tag completions returned      |
| `completion_tag_block_trigger`    | Cursor on ` -` line after `tags:` block key → tag completions |
| `completion_tag_no_trigger_title` | Cursor on `title:` frontmatter line → no tag completions      |
| `completion_tag_no_trigger_body`  | Cursor in file body (below frontmatter) → wiki-link path      |
| `completion_tag_items_from_index` | Index with notes tagged `foo`, `bar` → both in completions    |

**Integration test** (`tests/tags.rs` — new file):

| Test                        | What it verifies                                               |
| --------------------------- | -------------------------------------------------------------- |
| `tag_completion_round_trip` | Completion at `tags: [` position → response includes tag names |

> **Manual checkpoint:** in a workspace with notes carrying various `tags:`
> values, open a note, position the cursor inside `tags: [` or on a `- ` line
> after `tags:`, and invoke completion (Ctrl+Space). The tag picker should show
> all known tags from the workspace.

---

## Step 4 — Definition and References for tags (US-13, US-15)

Extend `handle_definition` and `handle_references` to recognise a tag at the
cursor position and return all notes carrying that tag.

**Deliverables:**

- `fn find_tag_at_position(note: &Note, pos: Position) -> Option<&Tag>` —
  searches `note.frontmatter.as_ref()?.tags` for a `Tag` whose `range`
  contains `pos`; uses the existing `contains` helper
- `fn locations_for_tag(tag: &str, index: &NoteIndex) -> Vec<Location>` —
  iterates `index.notes_by_tag(tag)`, finds the matching `Tag.range` within
  each note's frontmatter, returns `Location { uri, range: tag_range }`
- `handle_definition` return type changed from `Option<Location>` to
  `Option<GotoDefinitionResponse>` — wiki-link path returns
  `GotoDefinitionResponse::Scalar(location)` (same behaviour as before),
  tag path returns `GotoDefinitionResponse::Array(locations)`; the server
  dispatch site updated accordingly
- `handle_references` gains a second branch: if no wiki-link is at the cursor,
  check `find_tag_at_position` and return `locations_for_tag`

**Unit tests** (`src/handlers.rs` inline):

| Test                                   | What it verifies                                                   |
| -------------------------------------- | ------------------------------------------------------------------ |
| `definition_tag_returns_all_locations` | Cursor on tag → response lists every note carrying that tag        |
| `definition_tag_case_insensitive`      | Note tagged `Foo`, cursor on `foo` → still matched                 |
| `definition_wiki_link_unchanged`       | Cursor on wiki-link (not tag) → single-location Scalar unchanged   |
| `references_tag_returns_all_locations` | Cursor on tag → `references` returns same set as `definition`      |
| `tag_at_position_miss`                 | Cursor on `title:` line or body text → `find_tag_at_position` None |

**Integration tests** (extend `tests/tags.rs`):

| Test                           | What it verifies                                                 |
| ------------------------------ | ---------------------------------------------------------------- |
| `tag_definition_round_trip`    | `textDocument/definition` on a tag → array of matching note URIs |
| `tag_references_round_trip`    | `textDocument/references` on a tag → same set of locations       |
| `tag_definition_no_tag_at_pos` | Definition on body text → falls through to wiki-link or null     |

> **Manual checkpoint:** in a workspace with multiple notes sharing a tag (e.g.
> `tags: [rust]`), position the cursor on `rust` in one note's frontmatter and
> invoke Go to Definition. The editor should open a multi-location picker listing
> all files with that tag. Find References should show the same list. Verify that
> Go to Definition on a `[[wiki-link]]` still navigates to the single target file.

---

## Step 5 — Integration tests and CLI output

Polish: ensure the full test suite is green, the CLI prints tags, and every
story is verified end-to-end.

**Deliverables:**

- `knap parse <file>` prints tags in the output (format: `tags:  [foo, bar]`;
  prints nothing when the note has no tags)
- All unit and integration tests pass: `cargo test`
- Clippy clean: `cargo clippy -- -D warnings`

> **Manual checkpoint (full session):** run through all v0.5 features in a real
> editor session:
>
> 1. Tag completion inside `tags: [` and inside a block list.
> 2. Go to Definition on a tag value → multi-file picker.
> 3. Find References on a tag → same list.
> 4. Confirm v0.4 hover and v0.3 heading navigation are unaffected.
> 5. Rename a note — tag-bearing notes are unaffected (tags are not link targets).

---

## Done — v0.5 complete

At this point all three v0.5 user stories are implemented and tested:

| Story | Feature                                  | Delivered in step |
| ----- | ---------------------------------------- | ----------------- |
| US-14 | Tag completions from workspace tag index | Step 3            |
| US-13 | Go to Definition on a tag → all usages   | Step 4            |
| US-15 | Find References on a tag → all usages    | Step 4            |

Final check before tagging: run `cargo test`, run
`cargo clippy -- -D warnings`, then do a full manual end-to-end session covering
all three stories. Confirm all v0.4 and earlier features remain unaffected.
