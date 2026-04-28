# Request Handlers & Diagnostics

Covers all LSP request handlers and the diagnostic publisher.

Each request handler receives the decoded params and a shared reference to the
`NoteIndex`. Handlers are pure functions — they do not mutate the index or send
messages directly. They return a value that the Protocol Handler serialises and
sends.

---

## Shared helpers

### find_link_at_position()

Used by Definition, References, and Hover. Finds the wiki-link in a note whose
range contains a given cursor position.

```rust
fn find_link_at_position(note: &Note, pos: Position) -> Option<&WikiLink> {
    note.wiki_links.iter().find(|link| contains(link.range, pos))
}

fn contains(range: Range, pos: Position) -> bool {
    (pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character))
    && (pos.line < range.end.line
        || (pos.line == range.end.line && pos.character <= range.end.character))
}
```

### find_tag_at_position()

Used by Definition and References. Finds the tag in a note's frontmatter whose
range contains the cursor.

```rust
fn find_tag_at_position(note: &Note, pos: Position) -> Option<&Tag> {
    note.frontmatter.as_ref()?.tags.iter().find(|t| contains(t.range, pos))
}
```

---

## Completion (`textDocument/completion`)

### When it fires

The client sends a completion request when the user types `[` (registered as a
trigger character). Before building the list, the handler checks trigger
conditions in priority order:

1. **Tag trigger** — cursor is inside the frontmatter on a `tags:` line or a
   `- ` list item following a bare `tags:` key → returns tag completions.
2. **Schema value trigger** — cursor is inside the frontmatter after `key: ` and
   the schema has `enum` values for that key → returns enum completions
   (`CompletionItemKind::VALUE`).
3. **Schema key trigger** — cursor is inside the frontmatter on a blank line
   (no `:`) and a schema is present → returns schema keys not yet present in
   the note's frontmatter (`CompletionItemKind::PROPERTY`, `insert_text` is
   `"key: "`).
4. **Wiki-link trigger** — the text on the cursor's line immediately before the
   cursor ends with `[[` → returns note completions.

```rust
pub fn handle_completion(
    params: CompletionParams,
    index: &NoteIndex,
    schema: Option<&FrontmatterSchema>,
) -> Vec<CompletionItem>
```

### Response

For the wiki-link trigger, one `CompletionItem` per note in the index. When the
note has a frontmatter `title`, the label is the title and `insert_text` /
`filter_text` are the stem; otherwise label, insert_text, and filter_text all
equal the stem.

For the tag trigger, one `CompletionItem` per distinct tag in the index
(`CompletionItemKind::VALUE`).

---

## Hover (`textDocument/hover`)

```rust
pub fn handle_hover(params: HoverParams, index: &NoteIndex) -> Option<Hover>
```

Priority:

1. **Wiki-link at cursor** → resolved note preview (bold title + first
   `PREVIEW_LINES` body lines, frontmatter stripped, truncated with `…`).
   Returns `None` for broken/ambiguous links.
2. **Standard Markdown link at cursor** → varies by target type:
   - External URL → formatted `[text](url)` string.
   - Local path resolving to an indexed note → note preview.
   - Image → `**Image**\n\n\`path\`` string.
   - Unresolved local path → `` `path` `` string.

`render_preview(note)` builds the hover markdown. It strips frontmatter via
`frontmatter_body_offset` and takes the first `PREVIEW_LINES = 10` body lines.

---

## Document Symbols (`textDocument/documentSymbol`)

```rust
pub fn handle_document_symbols(
    params: DocumentSymbolParams,
    index: &NoteIndex,
) -> DocumentSymbolResponse
```

Returns `DocumentSymbolResponse::Nested` with one `DocumentSymbol` per heading
in the note. `range` covers the full heading line; `selection_range` covers the
heading text only (mirrors `Heading::text_range`).

---

## Workspace Symbols (`workspace/symbol`)

```rust
pub fn handle_workspace_symbols(
    params: WorkspaceSymbolParams,
    index: &NoteIndex,
) -> Vec<SymbolInformation>
```

Returns all headings across all indexed notes that contain the query string
(case-insensitive). An empty query returns every heading.

---

## Go to Definition (`textDocument/definition`)

```rust
pub fn handle_definition(
    params: GotoDefinitionParams,
    index: &NoteIndex,
) -> Option<GotoDefinitionResponse>
```

Priority:

1. **Wiki-link at cursor** → `GotoDefinitionResponse::Scalar(Location)`. The
   location is the heading range if the link has a matching anchor, otherwise
   `Range::default()` (top of file). Returns `None` for broken/ambiguous links.
2. **Tag in frontmatter at cursor** → `GotoDefinitionResponse::Array(locations)`
   with one `Location` per note that carries that tag (pointing at the tag's
   range in each note). Case-insensitive match.

---

## Find References (`textDocument/references`)

```rust
pub fn handle_references(params: ReferenceParams, index: &NoteIndex) -> Vec<Location>
```

Priority:

1. **Wiki-link at cursor** → all `LocatedLink`s from `index.links_to(target)`.
2. **Tag in frontmatter at cursor** → same locations as `handle_definition` for
   that tag.
3. **No symbol at cursor** → all backlinks to the current document
   (`index.links_to(current_path)`). This is what the backlinks code lens
   triggers when clicked.

---

## Prepare Rename (`textDocument/prepareRename`)

```rust
pub fn handle_prepare_rename(
    params: TextDocumentPositionParams,
    index: &NoteIndex,
) -> Option<PrepareRenameResponse>
```

Returns `PrepareRenameResponse::RangeWithPlaceholder` when the cursor is on a
heading line, with `range = heading.text_range` and `placeholder = heading.text`.
Returns `None` otherwise (editor shows "nothing to rename").

---

## Rename (`textDocument/rename`)

```rust
pub fn handle_rename(params: RenameParams, index: &NoteIndex) -> Option<WorkspaceEdit>
```

Builds a `WorkspaceEdit` that:

1. Rewrites the heading text itself (at `text_range`).
2. Rewrites every `[[note#OldText]]` anchor link whose stem resolves to the
   heading's file and whose anchor matches the old text (case-insensitive),
   using the stored `anchor_range`.

Returns `None` when the cursor is not on any heading.

---

## Code Actions (`textDocument/codeAction`)

```rust
pub fn handle_code_action(
    params: CodeActionParams,
    index: &NoteIndex,
    new_note_dir: Option<&Path>,
) -> Vec<CodeAction>
```

Returns zero or more `CodeAction` values for the wiki-link at `params.range.start`.
Returns `vec![]` when there is no wiki-link at the cursor, the note is not indexed,
or the link resolves in a way that offers no action.

`new_note_dir` is the resolved absolute path for new notes (from `Config::new_note_dir`,
already joined to the workspace root by `dispatch_request`). When `None`, new files
are created in the same directory as the current note.

| Condition                                           | Actions returned                                                                                                                                                                                                                                                                                                                                                                                |
| --------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Link resolves to `Broken`                           | One `QUICKFIX` action: `"Create note 'stem.ext'"` (or `"Create note 'dir/stem.ext'"` when `new_note_dir` is set) — a `WorkspaceEdit` with a `CreateFile` operation. New file lands in `new_note_dir` if configured, otherwise the same directory as the current note. Extension inferred from the current note's own extension; defaults to `md`. `ignore_if_exists: true` makes it idempotent. |
| Link resolves to `Found` with a non-matching anchor | One `QUICKFIX` action per heading in the target note: `"Change anchor to '#HeadingText'"` — a `WorkspaceEdit` with a `TextEdit` replacing `link.anchor_range` with the heading text. Returns `vec![]` if the target has no headings or the anchor already matches.                                                                                                                              |
| Link resolves to `Ambiguous`                        | `vec![]` — not actionable                                                                                                                                                                                                                                                                                                                                                                       |
| Link resolves to `Found` with no anchor             | `vec![]`                                                                                                                                                                                                                                                                                                                                                                                        |

---

## File Rename (`workspace/willRenameFiles`)

```rust
pub fn handle_will_rename_files(
    params: RenameFilesParams,
    index: &NoteIndex,
) -> WorkspaceEdit
```

Returns a `WorkspaceEdit` that rewrites every `[[old-stem]]` backlink (via
`index.links_to(old_path)`) to use the new stem. The edit targets `inner_range`
so that aliases (`[[old|display]]`) are rewritten to `[[new|display]]` — the
alias is preserved.

---

## Code Lens (`textDocument/codeLens`)

```rust
pub fn handle_code_lens(params: CodeLensParams, index: &NoteIndex) -> Vec<CodeLens>
```

Returns `vec![]` for URIs not in the index. For indexed notes, always returns
exactly one `CodeLens` at `(0,0)–(0,0)`:

- Title: `"↑ N backlink"` (singular) or `"↑ N backlinks"` (plural/zero)
- Command: `"knap.findBacklinks"` with `arguments: None`

Clicking the lens fires `knap.findBacklinks`, registered by editor-specific
extensions. The VS Code extension calls `references-view.findReferences` with
the document URI and `new vscode.Position(0, 0)`, which triggers
`textDocument/references` at position `(0,0)`. Because no link or tag sits at
`(0,0)`, `handle_references` falls through to case 3 (all backlinks to the
document).

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
    schema: Option<&FrontmatterSchema>,
)
```

### compute_diagnostics()

```rust
pub fn compute_diagnostics(
    path: &Path,
    index: &NoteIndex,
    schema: Option<&FrontmatterSchema>,
) -> Vec<Diagnostic>
```

For each wiki-link in the note:

| Resolution                                | Diagnostic                                                           |
| ----------------------------------------- | -------------------------------------------------------------------- |
| `Broken`                                  | Warning: `Link target not found: '[[stem]]'`                         |
| `Ambiguous`                               | Warning: `'[[stem]]' matches multiple files: /a/stem.md, /b/stem.md` |
| `Found` + no anchor                       | No diagnostic                                                        |
| `Found` + anchor not matching any heading | Warning: `Heading not found: '#anchor' in '[[stem#anchor]]'`         |

When `schema` is `Some`, three additional classes are emitted:

| Condition                         | Range         | Diagnostic                                                        |
| --------------------------------- | ------------- | ----------------------------------------------------------------- |
| Key not in `properties`           | `key_range`   | Warning: `Unknown frontmatter key: 'key'`                         |
| Key has `enum`, value not in enum | `value_range` | Warning: `Invalid value 'v' for 'key': expected one of [a, b, c]` |
| Key in `required` but not present | `(0,0)–(0,3)` | Warning: `Missing required frontmatter key: 'key'`                |

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
