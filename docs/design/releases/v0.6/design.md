# v0.6 Design — Backlinks

Covers the stories in the v0.6 release:

| Story | Feature                                                        |
| ----- | -------------------------------------------------------------- |
| US-25 | Backlinks code lens — `↑ N backlinks` at the top of every note |

---

## Goal

Surface incoming connections passively. When a writer opens any note, a code
lens at line 1 shows how many other notes link to it. Clicking the lens opens
the references panel, listing every backlink. No command is needed; the count
appears automatically as part of the editor's gutter annotations.

---

## `textDocument/codeLens` Protocol

The editor sends `CodeLensParams` when it wants lenses for an open document:

- `text_document` — the open file

The server returns `Vec<CodeLens>`. Each `CodeLens` carries:

- `range` — where the lens is anchored (must span a single line)
- `command` — the title shown inline and the action triggered on click

Capability advertisement:

```rust
code_lens_provider: Some(CodeLensOptions { resolve_provider: Some(false) }),
```

`resolve_provider: false` means each `CodeLens` returned from the initial
request already has its `command` filled in. No `codeLens/resolve` round-trip
is needed because all backlink data is in the index at request time.

---

## Handler

```rust
pub(crate) fn handle_code_lens(params: CodeLensParams, index: &NoteIndex) -> Vec<CodeLens>
```

Logic:

1. Resolve `params.text_document.uri` → `path`; return `vec![]` if not a `file://` URI.
2. If `index.get_note(&path)` is `None`, return `vec![]` (file not yet indexed).
3. Let `backlinks = index.links_to(&path)`.
4. If `backlinks.is_empty()`, return `vec![]` — no lens for notes with zero backlinks.
5. Build one `Location` per backlink:
   ```rust
   Location { uri: path_to_uri(&l.source_path), range: l.md_link.range }
   ```
6. Build a `Command`:
   ```rust
   Command {
       title: format!("↑ {} backlink{}", count, if count == 1 { "" } else { "s" }),
       command: "editor.action.showReferences".to_string(),
       arguments: Some(vec![
           serde_json::to_value(path_to_uri(&path)).unwrap(),
           serde_json::to_value(Position { line: 0, character: 0 }).unwrap(),
           serde_json::to_value(&locations).unwrap(),
       ]),
   }
   ```
7. Return a single `CodeLens`:
   ```rust
   CodeLens {
       range: Range {
           start: Position { line: 0, character: 0 },
           end:   Position { line: 0, character: 0 },
       },
       command: Some(command),
       data: None,
   }
   ```

The lens is placed at line 0 so it appears above the document content in all
editors that render code lenses. Collapsing it to a zero-width range is the
standard convention for document-level lenses.

---

## Click Behaviour

`editor.action.showReferences` is VS Code's built-in command for opening the
references panel programmatically. The three arguments are:

| Position | Type       | Value                                        |
| -------- | ---------- | -------------------------------------------- |
| 0        | `Uri`      | URI of the current document                  |
| 1        | `Position` | `{line: 0, character: 0}` (the lens anchor)  |
| 2        | `Location[]` | Pre-computed list of backlink locations    |

Bundling the locations into the command arguments means VS Code displays them
instantly without issuing a second `textDocument/references` request.

Zed does not yet render code lenses. The lens will be ignored there until Zed
adds support; no special-casing is needed server-side.

---

## What Is Not in This Release

- **Lens on headings** — `↑ N anchor links` on individual headings is v0.9
  (US-54). v0.6 delivers only the file-level backlinks lens.
- **Lens for zero backlinks** — orphan notes are not annotated here; that is
  the scope of v0.9's `US-38` (backlog). Emitting a `↑ 0 backlinks` lens would
  add noise without actionable information.
- **codeLens/resolve** — omitted because all data is already available. A
  resolve provider would add latency for no benefit.

---

## Testing

### Unit tests (`src/handlers.rs`)

| Test | What it verifies |
| ---- | ---------------- |
| `code_lens_single_backlink` | One incoming link → lens with title `"↑ 1 backlink"` and one location in args |
| `code_lens_multiple_backlinks` | Three incoming links → title `"↑ 3 backlinks"`, three locations in args |
| `code_lens_no_backlinks` | No incoming links → empty vec returned |
| `code_lens_unknown_file` | URI not in index → empty vec returned |
| `code_lens_range_is_line_zero` | Returned lens range is `{line:0,char:0}–{line:0,char:0}` |
| `code_lens_command_name` | `command.command == "editor.action.showReferences"` |

### Integration tests (`tests/lsp.rs`)

| Test | What it verifies |
| ---- | ---------------- |
| `test_code_lens_backlinks` | `textDocument/codeLens` on a file with two inbound links → one lens, correct count |
| `test_code_lens_no_backlinks` | `textDocument/codeLens` on an orphan file → empty array |
| `test_code_lens_updates_after_index_change` | After adding a new linking note via `didChange`, lens count reflects the update |
