# v0.6 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                      | Status | Notes |
| ----------------------------------------- | ------ | ----- |
| 1 — Handler, capability, dispatch wiring  | Done   |       |
| 2 — Integration tests                     | Done   |       |

---

## Step 1 — Handler, capability, dispatch wiring

Implement `handle_code_lens`, advertise the capability, and route
`textDocument/codeLens` requests. No index changes needed — `links_to()` already
provides backlink data.

**Deliverables:**

- `src/handlers.rs` — add imports:
  ```rust
  use lsp_types::{CodeLens, CodeLensParams, Command};
  ```

- `src/handlers.rs` — add `handle_code_lens`:
  ```rust
  pub(crate) fn handle_code_lens(params: CodeLensParams, index: &NoteIndex) -> Vec<CodeLens>
  ```

  Full logic as specified in the design doc. Key points:
  - Return `vec![]` for unknown URIs or files not in the index.
  - Return `vec![]` when `backlinks.is_empty()`.
  - Lens `range` is `{line:0,char:0}–{line:0,char:0}`.
  - `command.command = "editor.action.showReferences"`.
  - `command.arguments = Some([uri, position, locations])` — all serialized
    with `serde_json::to_value(...).unwrap()`.

- `src/server/mod.rs` — add import:
  ```rust
  use lsp_types::{CodeLensOptions, CodeLensParams};
  ```

- `src/server/mod.rs` — add to `ServerCapabilities` in `run`:
  ```rust
  code_lens_provider: Some(CodeLensOptions { resolve_provider: Some(false) }),
  ```

- `src/server/mod.rs` — add to `dispatch_request`:
  ```rust
  "textDocument/codeLens" => {
      let lenses = serde_json::from_value::<CodeLensParams>(req.params)
          .ok()
          .map(|params| handlers::handle_code_lens(params, index))
          .unwrap_or_default();
      connection
          .sender
          .send(Message::Response(Response::new_ok(req.id, lenses)))?;
  }
  ```

**Unit tests** (in `src/handlers.rs` test block):

| Test | What it verifies |
| ---- | ---------------- |
| `code_lens_single_backlink` | One incoming link → lens with title `"↑ 1 backlink"` and one location in args |
| `code_lens_multiple_backlinks` | Three incoming links → title `"↑ 3 backlinks"`, three locations in args |
| `code_lens_no_backlinks` | No incoming links → empty vec returned |
| `code_lens_unknown_file` | URI not in index → empty vec returned |
| `code_lens_range_is_line_zero` | Returned lens range is `{line:0,char:0}–{line:0,char:0}` |
| `code_lens_command_name` | `command.command == "editor.action.showReferences"` |

> **Manual checkpoint:** Open a vault in VS Code with the knap extension active.
> Open a note that has at least two other notes linking to it. Confirm a code
> lens appears above line 1 reading `↑ N backlinks`. Click the lens; confirm
> the references panel opens and lists the correct source files. Open a note
> with no inbound links; confirm no lens appears.

---

## Step 2 — Integration tests

End-to-end tests over the full LSP message loop. Always the last step.

**Deliverables:**

- `tests/lsp.rs` additions — all integration tests listed below
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test | What it verifies |
| ---- | ---------------- |
| `test_code_lens_backlinks` | `textDocument/codeLens` on a file with two inbound links → one lens, correct count and command |
| `test_code_lens_no_backlinks` | `textDocument/codeLens` on an orphan file → empty array |
| `test_code_lens_updates_after_index_change` | After adding a new linking note via `didChange`, lens count reflects the update |

> **Manual checkpoint (full session):** Open a vault with at least three notes
> where note A is linked from B and C. (1) Open A and confirm `↑ 2 backlinks`
> lens; click it and confirm B and C appear in the panel. (2) Add a link to A in
> a third note D; on save confirm the lens updates to `↑ 3 backlinks`. (3) Open
> an orphan note; confirm no lens is shown. (4) Confirm all v0.1–v0.5
> capabilities are unaffected (completions, go-to-definition, find-references,
> diagnostics, tags).

---

## Done — v0.6 complete

| Story | Feature                              | Delivered in step |
| ----- | ------------------------------------ | ----------------- |
| US-25 | Backlinks code lens at top of note   | Step 1            |
