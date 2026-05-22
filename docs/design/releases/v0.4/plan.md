# v0.4 Implementation Plan

---

## Status

| Step                                   | Status | Notes |
| -------------------------------------- | ------ | ----- |
| 1 — Thread config + add `newNoteDir`   | Done   |       |
| 2 — Code action infrastructure + US-18 | Done   |       |
| 3 — US-29: fix broken anchor           | Done   |       |
| 4 — US-31: JSON schema                 | Done   |       |
| 5 — Integration tests                  | Done   |       |

---

## Approach

All steps follow TDD:

1. Write all unit tests for the deliverable first — stub the function signature
   if needed to compile
2. Run `cargo test` and confirm the new tests **fail** before writing any
   implementation
3. Implement until all tests pass, then run `cargo clippy -- -D warnings`

Step 5 must follow the same cycle: write the integration tests, confirm they
fail, then make them pass.

---

## Step 1 — Thread config + add `newNoteDir`

Prerequisite for Step 2: `handle_code_actions` needs both `index` and `config`.
This step makes no observable behavioral change — it is pure plumbing and a
config field that starts unused.

**Deliverables:**

- `src/server/mod.rs` — `InitOptions`:
  ```rust
  new_note_dir: Option<String>,
  ```
- `src/server/mod.rs` — `Config`:
  ```rust
  new_note_dir: Option<String>,
  ```
- `src/server/mod.rs` — `Config::from_params`: wire `opts.new_note_dir`
- `src/server/mod.rs` — `dispatch_request(req, connection, index, config: &Config)`:
  add `config` parameter (passed through to handlers in later steps)
- `src/server/mod.rs` — call site in `run`: pass `&config`

**Unit tests** (in `src/server/tests.rs` or a new `src/server/mod.rs` inline block):

| Test                         | What it verifies                                                      |
| ---------------------------- | --------------------------------------------------------------------- |
| `config_new_note_dir_parsed` | `{"newNoteDir": "0-Inbox"}` → `Config.new_note_dir = Some("0-Inbox")` |
| `config_new_note_dir_absent` | No field → `Config.new_note_dir = None`                               |

> **Manual checkpoint:** none — this step has no user-visible effect.

---

## Step 2 — Code action infrastructure + US-18 (create missing file)

Implement `textDocument/codeAction`, delivering US-18 and US-30.

**Deliverables:**

- `src/handlers.rs` — add `handle_code_actions`:

  ```rust
  pub fn handle_code_actions(
      params: CodeActionParams,
      index: &NoteIndex,
      config: &Config,
  ) -> Vec<CodeActionOrCommand>
  ```

  Logic:
  - Resolve path from `params.text_document.uri`; return `vec![]` if not `file://`
  - Look up `note` via `index.get_note(&path)`; return `vec![]` if absent
  - Let `cursor = params.range.start`
  - For each `link` in `note.md_links` where `link.target` is non-empty and
    `link.range` contains `cursor`:
    - If `index.resolve(&path, &link.target) == Broken`:
      push a "Create note" `CodeAction` (see below)
  - Return collected actions

  New-file path logic (see design doc for full rationale):

  ```rust
  fn new_note_path(link_target: &str, source: &Path, config: &Config) -> PathBuf {
      match config.new_note_dir.as_deref().zip(config.index_roots.first()) {
          Some((dir, root)) => {
              let stem = Path::new(link_target).file_name().unwrap_or_default();
              root.join(dir).join(stem)
          }
          None => index::normalize_path(&source.parent().unwrap_or(source).join(link_target)),
      }
  }
  ```

  The `CodeAction`:

  ```rust
  CodeAction {
      title: "Create note".to_string(),
      kind: Some(CodeActionKind::QUICKFIX),
      edit: Some(WorkspaceEdit {
          document_changes: Some(DocumentChanges::Operations(vec![
              DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
                  uri: path_to_uri(&new_path),
                  options: Some(CreateFileOptions {
                      ignore_if_exists: Some(true),
                      overwrite: None,
                  }),
                  annotation_id: None,
              })),
          ])),
          ..Default::default()
      }),
      ..Default::default()
  }
  ```

- `src/server/mod.rs` — add to `ServerCapabilities`:

  ```rust
  code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
  ```

- `src/server/mod.rs` — add `"textDocument/codeAction"` arm to `dispatch_request`:
  deserialize `CodeActionParams`, call `handle_code_actions(params, index, config)`,
  send the result

- `src/handlers.rs` — add `fn range_contains(range: Range, pos: Position) -> bool`
  helper (same-line containment using LSP inclusive-start/exclusive-end semantics)

**Unit tests:**

| Test                                        | What it verifies                                                  |
| ------------------------------------------- | ----------------------------------------------------------------- |
| `code_actions_create_note_for_broken_link`  | Cursor on broken link → one `CreateFile` action returned          |
| `code_actions_no_action_for_valid_link`     | Cursor on existing link → empty list                              |
| `code_actions_no_action_off_link`           | Cursor not on any link → empty list                               |
| `code_actions_new_note_dir_respected`       | With `newNoteDir = "inbox"`, new file is at `{root}/inbox/{stem}` |
| `code_actions_default_path_relative`        | No `newNoteDir`, new file resolves relative to source             |
| `code_actions_create_note_ignore_if_exists` | Returned `CreateFile` has `ignore_if_exists: Some(true)`          |
| `code_actions_skips_anchor_only_links`      | Link with empty `target` → no action                              |

> **Manual checkpoint:** Open a note with a broken link such as
> `[ideas](ideas.md)`. Place the cursor anywhere on the link and trigger code
> actions (lightbulb / `Cmd+.`). Confirm "Create note" appears. Select it;
> confirm `ideas.md` is created next to the current file (visible in the file
> tree). Trigger the action again on the same link — no error (idempotent).
> Repeat with `newNoteDir = "0-Inbox"` in config; confirm the file appears in
> the inbox folder regardless of where the linking note lives.

---

## Step 3 — US-29: Fix broken anchor

Extend `handle_code_actions` to offer heading-pick actions for broken anchors.

**Deliverables:**

- `src/handlers.rs` — extend the per-link loop in `handle_code_actions`:
  - After the broken-file check, add a branch for `Found(target_path)`:
    - If `link.anchor` is `Some(anchor)` and `slug(anchor)` doesn't match any
      heading in the target note:
      - For each `heading` in `index.get_note(&target_path).headings`:
        - If `link.anchor_range` is `Some(anchor_range)`:
          ```rust
          CodeAction {
              title: format!("Change anchor to \"{}\"", slug(&heading.text)),
              kind: Some(CodeActionKind::QUICKFIX),
              edit: Some(WorkspaceEdit {
                  changes: Some(HashMap::from([(
                      path_to_uri(&path),
                      vec![TextEdit { range: anchor_range, new_text: slug(&heading.text) }],
                  )])),
                  ..Default::default()
              }),
              ..Default::default()
          }
          ```

**Unit tests:**

| Test                                              | What it verifies                                                             |
| ------------------------------------------------- | ---------------------------------------------------------------------------- |
| `code_actions_fix_anchor_offers_headings`         | Broken anchor + target has 2 headings → 2 actions returned                   |
| `code_actions_fix_anchor_edit_replaces_range`     | Each action's TextEdit targets `link.anchor_range` with `slug(heading.text)` |
| `code_actions_fix_anchor_no_headings_empty`       | Target note has no headings → no actions                                     |
| `code_actions_fix_anchor_valid_anchor_skipped`    | Anchor matches a heading → no fix actions offered                            |
| `code_actions_fix_anchor_no_anchor_range_skipped` | `anchor_range` is `None` → that link is skipped                              |

> **Manual checkpoint:** In a vault, create `b.md` with headings `# Introduction`
> and `# Summary`. In `a.md`, write `[see](b.md#nonexistent)`. Open `a.md` in
> an editor; confirm a warning diagnostic appears on `#nonexistent`. Place
> cursor on the link and trigger code actions. Confirm two actions appear:
> "Change anchor to \"introduction\"" and "Change anchor to \"summary\"".
> Select one; confirm the anchor in `a.md` is rewritten and the diagnostic
> clears. Verify that a valid link like `[see](b.md#introduction)` shows no
> anchor fix actions.

---

## Step 4 — US-31: JSON schema for Zed

**Deliverables:**

- `schemas/initialization_options.json` — JSON Schema (Draft-07) for the
  `initializationOptions` block. Properties: `extensions` (array of strings,
  default `["md"]`) and `newNoteDir` (string). `additionalProperties: false`.
- `docs/GETTING_STARTED.md` — add a short "Config reference" section pointing
  to the schema and showing the Zed `$schema` usage example.

No Rust code changes. No automated tests.

> **Manual checkpoint:** In Zed's `settings.json`, add a `$schema` key pointing
> to the local schema file (e.g. `file:///path/to/knap/schemas/initialization_options.json`).
> Confirm that typing inside the `initialization_options` block shows completions
> for `extensions` and `newNoteDir`, and that an unknown key is flagged with an
> inline error.

---

## Step 5 — Integration tests

End-to-end tests over the full LSP message loop. Always the last step.

**Deliverables:**

- `tests/lsp.rs` additions — all integration tests listed below
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                           | What it verifies                                                      |
| ---------------------------------------------- | --------------------------------------------------------------------- |
| `test_code_action_create_note`                 | Broken link → `textDocument/codeAction` returns a `CreateFile` action |
| `test_code_action_create_note_in_new_note_dir` | With `newNoteDir`, `CreateFile` URI points into the inbox folder      |
| `test_code_action_fix_anchor`                  | Broken anchor + target has headings → actions with correct TextEdit   |
| `test_code_action_empty_for_valid_link`        | Cursor on valid link → empty list, not null                           |
| `test_code_action_empty_off_link`              | Cursor not on a link → empty list                                     |

> **Manual checkpoint (full session):** Open a vault in an editor. (1) Create a
> note with `[missing](new.md)`. Trigger code actions and select "Create note";
> confirm the file appears in the tree and the diagnostic clears on next index
> cycle. (2) Create `b.md` with `# Intro`. In `a.md` write `[link](b.md#wrong)`.
> Trigger code actions; confirm "Change anchor to \"intro\"" appears and
> selecting it fixes the link. (3) Confirm all v0.1–v0.3 capabilities are
> unaffected.

---

## Done — v0.4 complete

| Story | Feature                                             | Delivered in step |
| ----- | --------------------------------------------------- | ----------------- |
| US-30 | `newNoteDir` config field                           | Step 1            |
| US-18 | Code action: create missing file                    | Step 2            |
| US-29 | Code action: fix broken anchor — pick from headings | Step 3            |
| US-31 | JSON schema for `initialization_options`            | Step 4            |
