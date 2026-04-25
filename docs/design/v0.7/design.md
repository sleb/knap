# v0.7 Design — Backlinks

Covers the story in the v0.7 release:

| Story | Feature                                                      |
| ----- | ------------------------------------------------------------ |
| US-25 | Optional backlinks panel / virtual document for current note |

---

## Goal

Surface all files that link to the current note passively — without the user
having to invoke Find References manually. The user should be able to open any
note and immediately see how many things point to it and what they are.

---

## Approach: Code Lens

A **code lens** is a standard LSP feature (`textDocument/codeLens`) that renders
an annotation above a line of text. It's supported by all major editors (Zed,
VS Code, Neovim with nvim-lspconfig, Helix) and requires no editor-specific
extensions.

knap will emit one code lens at line 0 of every indexed note:

```
↑ 3 backlinks          ← code lens annotation (clickable)
# My Note Title
...
```

- **Zero backlinks:** `"↑ 0 backlinks"` is emitted. Suppressing the lens on
  orphan notes would leave the user wondering whether the feature is working.
- **One or more backlinks:** `"↑ N backlinks"` at position `(0, 0)`.
- **Clicking the lens** triggers Find References at position `(0, 0)`, opening
  the references panel with the full list of backlinks.

### Why code lens over alternatives

| Option                     | Verdict                                                                                                                                  |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| **Code lens** (chosen)     | Standard LSP, works everywhere, passive display, click-to-expand is natural UX                                                           |
| Virtual document           | Not standard LSP — requires custom URI scheme and `workspace/executeCommand` plumbing; each editor handles it differently. Out of scope. |
| Inlay hints                | Designed for short inline annotations inside a line, not document-level metadata. Awkward fit.                                           |
| Find References (existing) | Already works but requires user action — not passive                                                                                     |

---

## LSP Protocol

### Capability

```rust
code_lens_provider: Some(CodeLensOptions {
    resolve_provider: Some(false),
}),
```

### Request: `textDocument/codeLens`

**Params:** `CodeLensParams { text_document: TextDocumentIdentifier }`

**Response:** `Vec<CodeLens>`

Each `CodeLens`:

```rust
CodeLens {
    range: Range { start: Position { line: 0, character: 0 }, end: Position { line: 0, character: 0 } },
    command: Some(Command {
        title: format!("↑ {} backlink{}", count, if count == 1 { "" } else { "s" }),
        command: "editor.action.findReferences".to_string(),
        arguments: Some(vec![
            serde_json::to_value(&params.text_document.uri).unwrap(),
            serde_json::to_value(Position { line: 0, character: 0 }).unwrap(),
        ]),
    }),
    data: None,
}
```

`editor.action.findReferences` is the VS Code / Zed command for showing
references at a given position. Editors that don't support this command will
display the code lens title as non-clickable text — still useful as a count.

---

## Handler

```rust
pub fn handle_code_lens(params: CodeLensParams, index: &NoteIndex) -> Vec<CodeLens>
```

Algorithm:

1. Resolve `params.text_document.uri` → `path` via `uri_to_path`. Return `vec![]`
   on non-file URIs.
2. Look up the note at `path` in the index. Return `vec![]` if not found (file
   not indexed — e.g. a non-note file opened in the editor).
3. Count backlinks: `index.references(&path).len()`.
4. Return one `CodeLens` at `(0, 0)` with title `"↑ N backlink(s)"` and command
   `editor.action.findReferences`. Always emit the lens for indexed notes,
   including zero-backlink ones.

### `NoteIndex` changes

`handle_code_lens` needs to query _inbound_ links for a path — which notes
contain a link to this note. The index already tracks outbound links per note
(used for rename). We need either:

- **Option A:** Expose `index.references(&path) -> Vec<&Path>` — derives the
  list on demand by scanning all notes for links resolving to `path`. Simple,
  no new index structure. O(N) per request but N (number of notes) is small in
  practice.
- **Option B:** Maintain a reverse index (`HashMap<PathBuf, HashSet<PathBuf>>`)
  updated on every `index()`/`remove()`. O(1) lookup, more memory, more
  complex to keep consistent.

**Decision: Option A for v0.7.** The on-demand scan reuses the existing
`resolve()` logic and keeps the index simple. If performance becomes a concern
with very large vaults it can be upgraded to Option B without changing the
handler.

---

## Routing

Add to `dispatch_request` in `src/server/mod.rs`:

```rust
"textDocument/codeLens" => {
    let lenses = serde_json::from_value::<CodeLensParams>(req.params)
        .ok()
        .map(|params| handlers::handle_code_lens(params, index))
        .unwrap_or_default();
    connection.sender.send(Message::Response(Response::new_ok(req.id, lenses)))?;
}
```

---

## Testing

### Unit tests (`src/handlers.rs` inline)

| Test                           | What it verifies                                                       |
| ------------------------------ | ---------------------------------------------------------------------- |
| `code_lens_no_backlinks`       | Indexed note with no inbound links → one lens titled `"↑ 0 backlinks"` |
| `code_lens_single_backlink`    | One note links here → lens title `"↑ 1 backlink"` (singular)           |
| `code_lens_multiple_backlinks` | Three notes link here → lens title `"↑ 3 backlinks"` (plural)          |
| `code_lens_position_is_zero`   | Lens range is always `(0,0)–(0,0)` regardless of note content          |
| `code_lens_unknown_uri`        | URI not in index → `vec![]`                                            |

### Integration tests (`tests/code_lens.rs`)

| Test                             | What it verifies                                               |
| -------------------------------- | -------------------------------------------------------------- |
| `code_lens_round_trip`           | Note with 2 inbound links → one lens with correct title        |
| `code_lens_zero_backlinks_shown` | Note with no inbound links → one lens titled `"↑ 0 backlinks"` |
