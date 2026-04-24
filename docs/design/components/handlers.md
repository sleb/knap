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
trigger character). Before building the list, the handler checks two trigger
conditions:

1. **Tag trigger** — cursor is inside the frontmatter on a `tags:` line or a
   `- ` list item following a bare `tags:` key → returns tag completions.
2. **Wiki-link trigger** — the text on the cursor's line immediately before the
   cursor ends with `[[` → returns note completions.

```rust
fn check_trigger(content: &str, pos: Position) -> bool {
    let line = content.lines().nth(pos.line as usize).unwrap_or("");
    let up_to_cursor = line.get(..pos.character as usize).unwrap_or(line);
    up_to_cursor.ends_with("[[")
}

fn check_tag_trigger(content: &str, pos: Position) -> bool { ... }
```

### Response

For the wiki-link trigger, one `CompletionItem` per note in the index. When the
note has a frontmatter `title`, the label is the title and `insert_text` /
`filter_text` are the stem; otherwise label, insert_text, and filter_text all
equal the stem.

```rust
pub fn handle_completion(params: CompletionParams, index: &NoteIndex) -> Vec<CompletionItem> {
    ...
    if check_tag_trigger(&note.content, pos) { return tag_completions(index); }
    if !check_trigger(&note.content, pos) { return vec![]; }

    index.all_notes().map(|n| {
        let title = n.frontmatter.as_ref().and_then(|fm| fm.title.as_deref()).map(str::to_owned);
        let label = title.clone().unwrap_or_else(|| n.stem.clone());
        CompletionItem {
            label,
            kind: Some(CompletionItemKind::FILE),
            filter_text: Some(n.stem.clone()),
            insert_text: Some(n.stem.clone()),
            detail: title.is_some().then(|| n.stem.clone()),
            ..Default::default()
        }
    }).collect()
}
```

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

## Diagnostics

Diagnostics are not a request handler — they are published proactively by the
Protocol Handler whenever the index changes. The Protocol Handler calls
`publish_diagnostics` with the set of affected paths returned by `IndexDelta`.

```rust
pub fn publish_diagnostics(paths: &HashSet<PathBuf>, index: &NoteIndex, sender: &Sender<Message>)
```

### compute_diagnostics()

```rust
pub fn compute_diagnostics(path: &Path, index: &NoteIndex) -> Vec<Diagnostic>
```

For each wiki-link in the note:

| Resolution                                | Diagnostic                                                           |
| ----------------------------------------- | -------------------------------------------------------------------- |
| `Broken`                                  | Warning: `Link target not found: '[[stem]]'`                         |
| `Ambiguous`                               | Warning: `'[[stem]]' matches multiple files: /a/stem.md, /b/stem.md` |
| `Found` + no anchor                       | No diagnostic                                                        |
| `Found` + anchor not matching any heading | Warning: `Heading not found: '#anchor' in '[[stem#anchor]]'`         |

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
