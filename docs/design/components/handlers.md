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

The client sends a completion request when the user types `(` (registered as a
trigger character). Before building the list, the handler checks that the text
on the cursor's line immediately before the cursor ends with `](` — confirming
the user is inside the URL portion of a standard Markdown link.

```rust
pub fn handle_completion(
    params: CompletionParams,
    index: &NoteIndex,
) -> Vec<CompletionItem>
```

### Response

One `CompletionItem` per note in the index (excluding the current file), plus
one item per non-note file (attachment) in the index. For notes, `insert_text`
and `filter_text` are the path relative to the current file's directory; when
the note has a frontmatter `title`, the label is the title and `detail` is the
relative path; otherwise label equals the relative path. For attachments, the
label is the bare filename and `insert_text` is the relative path from the
current file's directory; `kind` is `FILE`.

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

## Utilities

```rust
/// Convert an LSP URI to an absolute filesystem path.
/// Returns `None` for non-`file://` URIs (e.g. `untitled:`, `vscode-notebook-cell:`).
pub fn uri_to_path(uri: &lsp_types::Uri) -> Option<PathBuf>

/// Convert an absolute filesystem path to an LSP URI.
/// Panics if `path` is not absolute.
pub fn path_to_uri(path: &Path) -> lsp_types::Uri
```
