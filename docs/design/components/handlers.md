# Request Handlers & Diagnostics

Covers all LSP request handlers and the diagnostic publisher.

Each request handler receives the decoded params and a shared reference to the
`NoteIndex`. Handlers are pure functions — they do not mutate the index or send
messages directly. They return a value that the Protocol Handler serialises and
sends.

---

## Shared helpers

### find_md_link_at_position()

Used by Definition and References. Finds the Markdown link in a note whose
range contains a given cursor position.

```rust
fn find_md_link_at_position(note: &Note, pos: Position) -> Option<&MarkdownLink> {
    note.md_links.iter().find(|link| contains(link.range, pos))
}

fn contains(range: Range, pos: Position) -> bool {
    (pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character))
    && (pos.line < range.end.line
        || (pos.line == range.end.line && pos.character <= range.end.character))
}
```

---

## Completion (`textDocument/completion`)

### When it fires

The client sends a completion request when the user types `(`, `#`, or `/`
(all three are registered as trigger characters). Two distinct completion modes
are dispatched:

```rust
pub fn handle_completion(
    params: CompletionParams,
    index: &NoteIndex,
    config: &Config,
) -> Vec<CompletionItem>
```

### Anchor completion (`](path#` or `](#`)

When `check_anchor_trigger` detects that the cursor is immediately after a `#`
inside a link destination, the handler returns one item per heading. Two cases:

- **Same-file anchor** (`[text](#`) — `target_rel` is empty; items come from the
  headings of the current note itself.
- **Cross-file anchor** (`[text](file.md#`) — `target_rel` is non-empty; the
  handler resolves the target note via `index.resolve` and returns its headings.

Each item has:

- `label`: heading text as written (e.g. `"My Section"`)
- `insert_text`: GFM slug (e.g. `"my-section"`)
- `filter_text`: heading text (for editor-side fuzzy matching)
- `kind`: `REFERENCE`

### Directory completion (`](` or `](partial/`)

When `check_dir_trigger` detects that the cursor is inside a link destination
with no `#`, the handler returns items in three sorted tiers. `sort_text` uses
a string prefix so editors that respect the field keep the tiers ordered even
when their fuzzy scorer would otherwise rerank items:

| Tier | `sort_text` prefix | Contents                                                                                                                                                                                                                                                                                                                                                          |
| ---- | ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0    | `"0_"`             | **FOLDER** items — immediate subdirectories of `base_dir`. Label is `subdir/`; selecting one re-triggers completion (via the registered `/` trigger character) to show its contents.                                                                                                                                                                              |
| 1    | `"1_"`             | **FILE** items — notes and attachments directly inside `base_dir`. For notes with a frontmatter `title`, the label is the title and `detail` is the filename.                                                                                                                                                                                                     |
| 2    | `"2_"`             | **FILE** items — every other workspace file not already shown as a tier-1 item and not the current file. Label is the frontmatter `title` if present, otherwise the bare filename. `filter_text` is the full relative path so editors surface the item when the user types any path segment (e.g. `sub` surfaces `sub/b.md`). `detail` is the full relative path. |

Files already shown in tier 1 are tracked in a `HashSet` and excluded from
tier 2 to avoid duplicates.

Every item uses `text_edit: CompletionTextEdit::Edit(TextEdit { range, new_text
})` where `range` replaces everything from right after `](` to the cursor, and
`new_text` is the full relative path from the current note's directory (e.g.
`sub/` for a folder item, `sub/b.md` for a file). This ensures that
re-triggering after selecting a folder item, or selecting a global item while a
partial prefix is typed, replaces the prefix cleanly.

### Frontmatter value completion

When `check_frontmatter_value_trigger` detects the cursor is after the `:` on a
frontmatter key line, the handler looks up the key (case-insensitive) in
`config.frontmatter_schema.fields`. If the matching `SchemaField` has a `values`
list, it returns one `VALUE` item per allowed value whose string starts with the
typed partial (exact-case prefix match). Returns `vec![]` when the key is absent
from the schema or has no `values` list.

### Frontmatter key completion

When `check_frontmatter_key_trigger` detects the cursor is in key position inside
the frontmatter block, the handler returns one `FIELD` item per schema key that:

- is not already present in the note's frontmatter (case-insensitive), and
- starts with the typed partial (case-insensitive prefix match).

Each item's `new_text` is `"key: "` (key name followed by colon-space). Returns
`vec![]` when `config.frontmatter_schema.fields` is empty.

**Priority order** within `handle_completion`: tag trigger → frontmatter value trigger → anchor trigger → directory trigger → frontmatter key trigger.

---

## Go to Definition (`textDocument/definition`)

```rust
pub fn handle_definition(
    params: GotoDefinitionParams,
    index: &NoteIndex,
) -> Option<GotoDefinitionResponse>
```

Finds the `MarkdownLink` at the cursor position and returns a `Location`.

**Same-file anchor** (`link.target.is_empty()`): resolves the anchor against
`note.headings` directly. Returns a `Location` in the current file at the
matching heading's `range`, or `Range::default()` (top of file) if no heading
matches.

**Cross-file link** (`link.target` is non-empty): resolves via
`index.resolve(source, &link.target)`. Returns `None` for broken links. When
the link has an anchor, navigates to the matching heading's `range` in the
target note (falling back to `Range::default()` if the anchor doesn't match).
When there is no anchor, returns `Range::default()`.

Response is always `GotoDefinitionResponse::Scalar(Location)`.

---

## Find References (`textDocument/references`)

```rust
pub fn handle_references(params: ReferenceParams, index: &NoteIndex) -> Vec<Location>
```

Priority:

1. **Tag at cursor** → returns all notes carrying that tag.
2. **Markdown link at cursor** → resolves the target; returns all
   `LocatedLink`s from `index.links_to(target)`. Returns `vec![]` for broken
   links.
3. **Heading at cursor** (no link at cursor) → collects all anchor references
   to that heading: same-file bare anchors (`[text](#slug)` in the current note
   whose anchor slug matches) plus cross-file anchors (`[text](this.md#slug)`
   from `index.links_to(current_path)` filtered by anchor slug).
4. **No link or heading at cursor** → returns all backlinks to the current
   document (`index.links_to(current_path)`).

---

## Diagnostics

Diagnostics are not a request handler — they are published proactively by the
Protocol Handler whenever the index changes. The Protocol Handler calls
`publish_diagnostics` with the set of affected paths returned by `IndexDelta`.

```rust
pub fn publish_diagnostics(
    paths: &HashSet<PathBuf>,
    index: &NoteIndex,
    config: &Config,
    sender: &Sender<Message>,
)
```

### compute_diagnostics()

```rust
pub fn compute_diagnostics(path: &Path, index: &NoteIndex, config: &Config) -> Vec<Diagnostic>
```

For each Markdown link in the note:

| Link type                                              | Diagnostic range               | Message                                    |
| ------------------------------------------------------ | ------------------------------ | ------------------------------------------ |
| Bare anchor `[text](#slug)` — slug not in this file    | `link.anchor_range` (or range) | `Heading not found: '#slug'`               |
| Bare anchor `[text](#)` — empty slug (`anchor = None`) | —                              | No diagnostic                              |
| Cross-file — `Broken` target                           | `link.target_range`            | `Link target not found: 'path/to/note.md'` |
| Cross-file — `Found` + anchor not matching any heading | `link.anchor_range`            | `Heading not found: '#anchor'`             |
| Cross-file — `Found` + no anchor (or valid anchor)     | —                              | No diagnostic                              |

Bare anchor-only links (`target = ""`) are validated against the current note's
headings (via GFM slug comparison). A link `[text](#)` with an empty anchor
(`link.anchor = None`) produces no diagnostic.

### Schema diagnostics

When `config.frontmatter_schema` is non-empty (has fields, `require_frontmatter`,
or `warn_unknown_keys` set), an additional validation pass runs after the
link-diagnostics loop:

| Condition                                                                   | Diagnostic range    | Message                                          |
| --------------------------------------------------------------------------- | ------------------- | ------------------------------------------------ |
| Note has no frontmatter + `require_frontmatter: true` + field is `required` | `(0,0)`             | `Required frontmatter key missing: 'key'`        |
| Note has frontmatter + required schema key absent (case-insensitive match)  | `(0,0)`             | `Required frontmatter key missing: 'key'`        |
| Field value not in schema `values` list (exact-case equality)               | `field.value_range` | `Value 'X' is not in the allowed list for 'key'` |
| Key not in schema + `warn_unknown_keys: true`                               | `field.key_range`   | `Unknown frontmatter key: 'key'`                 |
| Field has no scalar value (`value: None`) or schema has no `values` list    | —                   | No diagnostic                                    |

Key matching is case-insensitive (`eq_ignore_ascii_case`). Value matching is
exact-case.

---

## Rename (`workspace/willRenameFiles`)

```rust
#[allow(clippy::mutable_key_type)]
pub fn handle_will_rename_files(params: RenameFilesParams, index: &NoteIndex) -> WorkspaceEdit
```

Called by the editor before applying a rename. Returns a `WorkspaceEdit` that
rewrites all affected links atomically — editors apply the edit and the rename
together so no link is left broken.

For each `FileRename { old_uri, new_uri }` in `params.files`:

1. **Incoming links** — iterates `index.links_to(&old_path)`. For each
   `LocatedLink`, computes `new_target = relative_path(source_dir, new_path)`
   and pushes a `TextEdit` on `located.md_link.target_range` into the source
   file's entry in `changes`.

2. **Outgoing links** — fetches `index.get_note(&old_path).md_links`. For each
   link, skips empty targets and URLs; computes
   `abs_target = normalize_path(old_dir.join(&link.target))`, then
   `new_target = relative_path(new_dir, &abs_target)`; pushes a `TextEdit` on
   `link.target_range` into `old_path`'s entry only when
   `new_target != link.target` (i.e. the rename changes the relative path).

Returns `WorkspaceEdit { changes: Some(changes), ..Default::default() }`. The
`changes` map is keyed by `lsp_types::Uri`; an empty map is returned for files
with no affected links.

---

## Document Symbols (`textDocument/documentSymbol`)

```rust
pub fn handle_document_symbols(
    params: DocumentSymbolParams,
    index: &NoteIndex,
) -> Option<DocumentSymbolResponse>
```

Returns a flat list of `DocumentSymbol` entries, one per heading in document
order. Each symbol carries the heading text as its `name`, `SymbolKind::STRING`,
and a `range` / `selection_range` covering the full heading line. Returns `None`
when the file is not indexed; returns an empty list for a file with no headings.

---

## Workspace Symbols (`workspace/symbol`)

```rust
pub fn handle_workspace_symbols(
    params: WorkspaceSymbolParams,
    index: &NoteIndex,
) -> Vec<SymbolInformation>
```

Returns headings from all indexed notes whose text contains the query string
(case-insensitive). An empty query returns every heading in the workspace.
Each result carries the heading text as `name`, the containing filename (without
directory) as `container_name`, and `SymbolKind::STRING`.

---

## Prepare Rename (`textDocument/prepareRename`)

```rust
pub fn handle_prepare_rename(
    params: TextDocumentPositionParams,
    index: &NoteIndex,
) -> Option<PrepareRenameResponse>
```

Returns `Some(PrepareRenameResponse::RangeWithPlaceholder { range, placeholder
})` when the cursor is on a heading line, where `range` is the heading
text range (excluding the `## ` prefix) and `placeholder` is the heading text.
Returns `None` when the cursor is not on a heading — the editor shows no rename
UI in that case.

The handler uses the indexed note when available. If the file is absent from the
index (e.g. the server started without workspace folders configured and no
`didOpen` has been received yet), it falls back to reading the file from disk
and parsing it on the fly. Returns `None` if the file cannot be read.

---

## Rename (`textDocument/rename`)

```rust
pub fn handle_rename(
    params: RenameParams,
    index: &NoteIndex,
) -> Option<WorkspaceEdit>
```

Renames a heading and all anchor links that point to it. The cursor must be on
a heading line (same check as `prepareRename`); returns `None` otherwise.
Applies the same indexed-note / disk-parse fallback as `handle_prepare_rename`.

For the heading at the cursor:

1. **Heading text edit** — rewrites the heading text in place (preserving the
   `## ` prefix) to the new name.
2. **Self-link edits** — anchor-only links (`[text](#old-slug)`) within the
   same file are rewritten to the new slug.
3. **Incoming anchor edits** — for every note in the workspace that links to
   this file via `index.links_to`, finds `[text](path#old-slug)` links whose
   slug matches the old heading (via `slug()`) and rewrites the anchor to the
   new slug. When the file was not in the index (disk-parse fallback), `links_to`
   returns an empty slice and no incoming-link edits are produced.

URL targets are skipped. Returns `Some(WorkspaceEdit { changes: Some(map) })`.

---

## Code Actions (`textDocument/codeAction`)

```rust
pub(crate) fn handle_code_actions(
    params: CodeActionParams,
    index: &NoteIndex,
    config: &Config,
) -> Vec<CodeActionOrCommand>
```

Re-derives link context from the index by iterating `note.md_links` and
checking `contains(link.range, cursor)` where `cursor = params.range.start`.
Anchor-only links (`link.target.is_empty()`) are always skipped.

For each link under the cursor:

| Condition                                       | Action offered                                                                                    |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `index.resolve(…) == Broken`                    | **Create note** — a `CreateFile` workspace edit (`ignore_if_exists: true`)                        |
| `Found(target)` + broken anchor (slug mismatch) | One **Change anchor to "…"** per heading in the target note — a `TextEdit` on `link.anchor_range` |
| `Found(target)` + valid anchor (or no anchor)   | No action                                                                                         |

New-file path logic for **Create note** (`new_note_path`):

```rust
fn new_note_path(link_target: &str, source: &Path, config: &Config) -> PathBuf {
    match config.new_note_dir.as_deref().zip(config.index_roots.first()) {
        Some((dir, root)) => root.join(dir).join(Path::new(link_target).file_name()),
        None => normalize_path(&source.parent().join(link_target)),
    }
}
```

---

## Code Lens (`textDocument/codeLens`)

```rust
pub(crate) fn handle_code_lens(params: CodeLensParams, index: &NoteIndex) -> Vec<CodeLens>
```

Returns two classes of lenses:

1. **Backlinks lens** — a single `↑ N backlink(s)` lens anchored at line 0,
   character 0. Omitted when the file has no incoming links. Uses
   `editor.action.showReferences` with the pre-computed `Location` list so VS
   Code opens the references panel on click without a second request.

2. **Heading anchor-link lenses** — one `↑ N anchor link(s)` lens per heading
   that is the target of one or more `#slug` anchor links. Includes same-file
   bare anchors (`[text](#slug)` in the current file) and cross-file anchors
   (`[text](this.md#slug)` from any note in the workspace). Headings with no
   incoming anchor links produce no lens. The lens `range.start` equals the
   heading's `range.start`.

---

## Folding Ranges (`textDocument/foldingRange`)

```rust
pub(crate) fn handle_folding_ranges(params: FoldingRangeParams, index: &NoteIndex) -> Vec<FoldingRange>
```

Returns fold regions for heading sections and fenced code blocks.

- **Heading sections** — one region per heading, from the heading's line to the
  line before the next peer-or-higher-level heading (or the last content line of
  the document). Single-line sections (end equals start) are omitted.
- **Code fences** — one `FoldingRangeKind::Region` per `CodeFence` in
  `note.code_fences`.

Private helper: `fn last_content_line(content: &str) -> u32` — returns the
zero-based line number of the last non-empty line in the document.

---

## Selection Range (`textDocument/selectionRange`)

```rust
pub(crate) fn handle_selection_range(params: SelectionRangeParams, index: &NoteIndex) -> Vec<SelectionRange>
```

Returns one `SelectionRange` per position in `params.positions`, each
describing a chain of nested ranges for smart expand/contract:

**word → link → paragraph → heading section → document**

Levels are deduplicated — if two consecutive levels would have the same range,
the inner one is omitted. The outermost range always covers the full document.

Private helpers:

- `fn word_range_at(line: &str, cursor_char: u32, line_num: u32) -> Option<Range>` —
  returns the UTF-16 range of the word under the cursor; `None` on whitespace.
- `fn paragraph_range(content: &str, cursor_line: u32) -> Range` — scans
  backward and forward from `cursor_line` to the nearest blank lines.
- `fn heading_section_range(content: &str, headings: &[Heading], cursor_line: u32) -> Option<Range>` —
  the section from the enclosing heading to just before the next peer-level
  heading.
- `fn build_selection_chain(pos: Position, note: &Note) -> SelectionRange` —
  assembles the full chain for one position.

---

## Inlay Hints (`textDocument/inlayHint`)

```rust
pub(crate) fn handle_inlay_hints(params: InlayHintParams, index: &NoteIndex) -> Vec<InlayHint>
```

For each Markdown link in the visible range (`params.range`), if the link
resolves to an indexed note with a `title:` frontmatter field, emits one inlay
hint positioned at the end of the link's `target_range`:

- `label`: `InlayHintLabel::String(format!("-> {title}"))`
- `kind`: `None` (neither TYPE nor PARAMETER fits a linked-note title)

External URL targets and broken links produce no hint. Links outside
`params.range` are excluded via `range_contains_position`.

Private helper: `fn range_contains_position(range: &Range, pos: Position) -> bool`.

---

## Utilities

```rust
/// Convert an LSP URI to an absolute filesystem path.
/// Returns `None` for non-`file://` URIs (e.g. `untitled:`, `vscode-notebook-cell:`).
pub fn uri_to_path(uri: &lsp_types::Uri) -> Option<PathBuf>

/// Convert an absolute filesystem path to an LSP URI.
/// Panics if `path` is not absolute.
pub fn path_to_uri(path: &Path) -> lsp_types::Uri
```
