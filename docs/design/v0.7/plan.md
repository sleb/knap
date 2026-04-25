# v0.7 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                              | Status | Notes |
| --------------------------------- | ------ | ----- |
| 1 — Capability + routing skeleton | Done   |       |
| 2 — US-25: Backlinks code lens    | Todo   |       |
| 3 — Integration tests             | Todo   |       |

---

## Step 1 — Capability advertisement and routing skeleton

Wire up the protocol plumbing before writing any handler logic.

**Deliverables:**

- `code_lens_provider: Some(CodeLensOptions { resolve_provider: Some(false) })`
  added to `ServerCapabilities` in `src/server/mod.rs`
- `"textDocument/codeLens"` arm added to `dispatch_request`; deserializes
  `CodeLensParams`, calls `handlers::handle_code_lens(params, index)`, responds
  with the result
- `pub fn handle_code_lens(params: CodeLensParams, index: &NoteIndex) -> Vec<CodeLens>`
  added to `src/handlers.rs`; stub returns `vec![]` for now

**Unit tests:**

| Test                    | What it verifies            |
| ----------------------- | --------------------------- |
| `code_lens_unknown_uri` | URI not in index → `vec![]` |

> **Manual checkpoint:** open a note in the editor. No crash, no error. The
> `codeLensProvider` capability should appear in the server's `initialize`
> response (visible in the editor's LSP log).

---

## Step 2 — US-25: Backlinks code lens

Implement the handler that counts inbound links and emits a code lens.

**Deliverables:**

- `handle_code_lens` extended: counts notes that contain a link resolving to
  the current path (on-demand scan via `index.get_all_notes()` + `resolve()`);
  returns `vec![]` for zero backlinks, otherwise one `CodeLens` at `(0, 0)`
- `NoteIndex` gets a `references(&path) -> Vec<PathBuf>` helper (or equivalent
  inline logic in the handler — decide at implementation time)
- Lens title: `"↑ N backlink"` / `"↑ N backlinks"` (singular/plural)
- Lens command: `editor.action.findReferences` with the document URI and
  `Position { line: 0, character: 0 }` as arguments

**Unit tests:**

| Test                           | What it verifies                                                       |
| ------------------------------ | ---------------------------------------------------------------------- |
| `code_lens_no_backlinks`       | Indexed note with no inbound links → one lens titled `"↑ 0 backlinks"` |
| `code_lens_single_backlink`    | One inbound link → title `"↑ 1 backlink"` (singular)                   |
| `code_lens_multiple_backlinks` | Three inbound links → title `"↑ 3 backlinks"` (plural)                 |
| `code_lens_position_is_zero`   | Lens range is always `(0,0)–(0,0)`                                     |

> **Manual checkpoint:** open a note that other notes link to — `↑ N backlinks`
> appears at the top; clicking opens the references panel. Open a note with no
> inbound links — `↑ 0 backlinks` appears, confirming the feature is active.

---

## Step 3 — Integration tests

**Deliverables:**

- `tests/code_lens.rs` with all integration tests from steps 1–2
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                           | What it verifies                                        |
| ------------------------------ | ------------------------------------------------------- |
| `code_lens_round_trip`         | Note with 2 inbound links → one lens with correct title |
| `code_lens_no_backlinks_empty` | Note with no inbound links → empty response             |

> **Manual checkpoint (full session):** verify the backlinks lens appears and
> updates correctly as files are opened and links change. Confirm v0.6 code
> actions and earlier features are unaffected.

---

## Done — v0.7 complete

| Story | Feature             | Delivered in step |
| ----- | ------------------- | ----------------- |
| US-25 | Backlinks code lens | Step 2            |
