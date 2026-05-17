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
) -> Vec<CompletionItem>
```

### Anchor completion (`](path#`)

When `check_anchor_trigger` detects that the cursor is immediately after a `#`
inside a link destination with a non-empty path, the handler resolves the target
note and returns one item per heading. Each item has:

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

---

## Go to Definition (`textDocument/definition`)

```rust
pub fn handle_definition(
    params: GotoDefinitionParams,
    index: &NoteIndex,
) -> Option<GotoDefinitionResponse>
```

Finds the `MarkdownLink` at the cursor position. Resolves the target via
`index.resolve(source, &link.target)`. Returns `None` for broken links.

When the link has an anchor, navigates to the matching heading's `range` in the
target note. If the anchor doesn't match any heading, falls back to
`Range::default()` (top of file). When there is no anchor, always returns
`Range::default()`.

Response is always `GotoDefinitionResponse::Scalar(Location)`.

---

## Find References (`textDocument/references`)

```rust
pub fn handle_references(params: ReferenceParams, index: &NoteIndex) -> Vec<Location>
```

Priority:

1. **Markdown link at cursor** → resolves the target; returns all
   `LocatedLink`s from `index.links_to(target)`. Returns `vec![]` for broken
   links.
2. **No link at cursor** → returns all backlinks to the current document
   (`index.links_to(current_path)`). This is what the backlinks code lens
   triggers when clicked (v0.7+).

---

## Diagnostics

Diagnostics are not a request handler — they are published proactively by the
Protocol Handler whenever the index changes. The Protocol Handler calls
`publish_diagnostics` with the set of affected paths returned by `IndexDelta`.

```rust
pub fn publish_diagnostics(
    paths: &HashSet<PathBuf>,
    index: &NoteIndex,
    sender: &Sender<Message>,
)
```

### compute_diagnostics()

```rust
pub fn compute_diagnostics(path: &Path, index: &NoteIndex) -> Vec<Diagnostic>
```

Anchor-only links (`target = ""`) are skipped — they reference a heading in the
current file and are not validated in v0.1.

For each local Markdown link with a non-empty target:

| Resolution                                | Diagnostic range    | Message                                    |
| ----------------------------------------- | ------------------- | ------------------------------------------ |
| `Broken`                                  | `link.target_range` | `Link target not found: 'path/to/note.md'` |
| `Found` + anchor not matching any heading | `link.anchor_range` | `Heading not found: '#anchor'`             |
| `Found` + no anchor (or valid anchor)     | —                   | No diagnostic                              |

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

For the heading at the cursor:

1. **Heading text edit** — rewrites the heading text in place (preserving the
   `## ` prefix) to the new name.
2. **Incoming anchor edits** — for every note in the workspace, finds
   `[text](path#old-slug)` links whose slug matches the old heading (via
   `slug()`) and rewrites the anchor to the new slug.
3. **Self-link edits** — anchor-only links (`[text](#old-slug)`) within the
   same file are also rewritten.

URL targets are skipped. Returns `Some(WorkspaceEdit { changes: Some(map) })`.

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
