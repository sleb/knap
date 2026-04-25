# v0.6 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                         | Status  | Notes                           |
| -------------------------------------------- | ------- | ------------------------------- |
| 1 — Capability + routing skeleton            | Done    |                                 |
| 2 — US-18: Create missing file action        | Done    |                                 |
| 3 — US-29: Fix broken anchor actions         | Done    |                                 |
| 4 — Integration tests                        | Done    |                                 |
| 5 — US-30: newNoteDir config                 | Done    |                                 |
| 6 — US-31: Zed extension init options schema | Blocked | Needs zed_extension_api > 0.7.0 |

---

## Step 1 — Capability advertisement and routing skeleton

Wire up the protocol plumbing before writing any handler logic. This lets the
client start sending `textDocument/codeAction` requests immediately and ensures
the dispatch path is exercised from day one.

**Deliverables:**

- `code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions { code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]), resolve_provider: Some(false), .. }))` added to `ServerCapabilities` in `src/server/mod.rs`
- `"textDocument/codeAction"` arm added to `dispatch_request` in
  `src/server/mod.rs`; deserializes `CodeActionParams`, calls
  `handlers::handle_code_action(params, index)`, responds with the result
- `pub fn handle_code_action(params: CodeActionParams, index: &NoteIndex) -> Vec<CodeAction>` added to `src/handlers.rs`; stub returns `vec![]` for now
- Necessary imports added (`CodeAction`, `CodeActionKind`,
  `CodeActionOptions`, `CodeActionParams`, `CodeActionProviderCapability`)

**Unit tests:**

| Test                                  | What it verifies                       |
| ------------------------------------- | -------------------------------------- |
| `code_action_no_link_at_cursor`       | Cursor not on a wiki-link → `vec![]`   |
| `code_action_resolved_link_no_action` | Valid `[[found]]` at cursor → `vec![]` |

**Integration test** (add to new `tests/code_actions.rs`):

| Test                      | What it verifies                                                   |
| ------------------------- | ------------------------------------------------------------------ |
| `no_action_on_valid_link` | Full round-trip: valid link at cursor → empty code action response |

> **Manual checkpoint:** open a note in the editor and invoke Quick Fix (or the
> lightbulb) on any text. No crash, no error — the server responds with an empty
> list. The `code_action_provider` capability should appear in the server's
> `initialize` response (visible in the editor's LSP log).

---

## Step 2 — US-18: Create missing file action

Implement the code action that creates a new note when the cursor is on a
broken `[[link]]`.

**Deliverables:**

- `handle_code_action` extended: when `resolve(&link.stem)` returns `Broken`,
  compute the new file path (`current_file.parent() / stem + current_extension`)
  and return one `CodeAction`:
  - `title`: `"Create note '{stem}.{ext}'"`
  - `kind`: `CodeActionKind::QUICKFIX`
  - `edit`: `WorkspaceEdit` with `document_changes: DocumentChanges::Operations([CreateFile { uri, options: CreateFileOptions { overwrite: false, ignore_if_exists: true } }])`
- Extension is inferred from the current note's own path extension (fallback
  `"md"` if absent)

**Unit tests:**

| Test                                     | What it verifies                                                                                  |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `code_action_broken_link_creates_file`   | Broken `[[missing]]` → one action, title `"Create note 'missing.md'"`, edit contains `CreateFile` |
| `code_action_broken_link_same_extension` | Current file is `note.mdx` → new file URI ends with `.mdx`                                        |
| `code_action_ambiguous_no_action`        | Ambiguous link → `vec![]` (not actionable)                                                        |

**Integration test** (add to `tests/code_actions.rs`):

| Test                            | What it verifies                                                                                                                        |
| ------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `create_file_action_round_trip` | Broken link at cursor → response contains one `CodeAction` whose `edit` has `document_changes` with a `CreateFile` for the expected URI |

> **Manual checkpoint:** in a workspace, write `[[does-not-exist]]` in a note.
> Position the cursor on the link and invoke Quick Fix. A single action
> "Create note 'does-not-exist.md'" should appear. Selecting it should create
> an empty file at the expected path. The broken-link diagnostic should clear on
> the next file-watcher event (or after a didOpen on the new file).

---

## Step 3 — US-29: Fix broken anchor actions

Implement the code actions that replace a broken `[[note#anchor]]` with a
working heading from the target file.

**Deliverables:**

- `handle_code_action` extended: when `resolve(&link.stem)` returns
  `Found(target_path)` and `link.anchor` is `Some(anchor)` where no heading in
  the target note matches `anchor` (case-insensitive):
  - Look up the target note in the index (`index.get_note(&target_path)`)
  - For each heading `h` in `target_note.headings`, return one `CodeAction`:
    - `title`: `"Change anchor to '#{}'"` with `h.text`
    - `kind`: `CodeActionKind::QUICKFIX`
    - `edit`: `WorkspaceEdit` with `changes` replacing
      `link.anchor_range` (the anchor text only) with `h.text`
  - If the target note has no headings, or is not in the index, return `vec![]`
- The `link.anchor_range` must be `Some` for a wiki-link with an anchor; use
  `.expect("anchor_range present when anchor is Some")` — the parser invariant
  guarantees this

**Unit tests:**

| Test                                        | What it verifies                                                                                |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `code_action_broken_anchor_lists_headings`  | `[[note#Bad]]` with `note.md` having `## Alpha` and `## Beta` → two actions with correct titles |
| `code_action_broken_anchor_edit_range`      | Edit range equals `link.anchor_range`; `new_text` is the heading text (no `#` prefix)           |
| `code_action_no_headings_no_anchor_actions` | Target exists but has no headings → `vec![]`                                                    |
| `code_action_valid_anchor_no_action`        | `[[note#Good]]` where `Good` heading exists → `vec![]`                                          |

**Integration test** (add to `tests/code_actions.rs`):

| Test                           | What it verifies                                                                                                                   |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| `fix_anchor_action_round_trip` | `[[note#Bad]]` at cursor, target has two headings → response contains two `CodeAction`s; each has a `TextEdit` at the anchor range |

> **Manual checkpoint:** write `[[existing-note#BadHeading]]` where
> `existing-note.md` exists and has several headings. Position the cursor on
> the link and invoke Quick Fix. A list of "Change anchor to '#…'" options
> should appear, one per heading. Selecting one should rewrite the link in place.
> Verify the broken-anchor diagnostic clears on the next save.

---

## Step 4 — Integration tests and polish

Ensure the full test suite is green and every story is verified end-to-end.

**Deliverables:**

- All unit and integration tests pass: `cargo test`
- Clippy clean: `cargo clippy -- -D warnings`
- `tests/code_actions.rs` contains all three integration tests from steps 1–3

> **Manual checkpoint (full session):** run through all v0.6 features in a real
> editor session:
>
> 1. Broken `[[link]]` → Quick Fix → file created, diagnostic clears.
> 2. `[[note#BadAnchor]]` → Quick Fix → heading picker appears, selecting one
>    rewrites the anchor, diagnostic clears.
> 3. Cursor on a valid `[[link]]` → Quick Fix → no actions (or lightbulb absent).
> 4. Confirm v0.5 tag features and v0.4 hover are unaffected.

---

## Step 6 — US-31: Zed extension initialization options schema

Add `language_server_initialization_options_schema` to `zed-knap/src/lib.rs`.
This is a pure `zed-knap` change — the knap server is untouched.

**Deliverables:**

- `language_server_initialization_options_schema` overridden in `KnapExtension`
  impl; returns a `serde_json::Value` describing all `InitOptions` fields
- `additionalProperties: false` so unknown keys are flagged by the editor
- `KNAP_LOG=debug` removed from the server env; `eprintln!` info logs added for
  binary source (local path or GitHub release version)
- `extension.wasm` rebuilt

> **Manual checkpoint:** in Zed's `settings.json`, type an unknown key inside
> `initialization_options` for knap — the editor should show a warning. Type a
> known key like `newNoteDir` — autocompletion should offer it and show the
> description.

---

## Done — v0.6 complete

At this point all v0.6 user stories are implemented and tested:

| Story | Feature                                                | Delivered in step |
| ----- | ------------------------------------------------------ | ----------------- |
| US-18 | Create missing file from broken `[[link]]`             | Step 2            |
| US-29 | Fix broken anchor by picking a heading                 | Step 3            |
| US-30 | `newNoteDir` config for Quick Fix create-note actions  | Step 5            |
| US-31 | Zed extension JSON schema for `initialization_options` | Step 6            |

Final check before tagging: run `cargo test`, run
`cargo clippy -- -D warnings`, then do a full manual end-to-end session covering
all stories. Confirm all v0.5 and earlier features remain unaffected.
