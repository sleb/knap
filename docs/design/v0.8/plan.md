# v0.8 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                             | Status | Notes |
| -------------------------------- | ------ | ----- |
| 1 — Schema types + init plumbing | Todo   |       |
| 2 — Parser extension             | Todo   |       |
| 3 — Schema-driven completions    | Todo   |       |
| 4 — Schema-driven diagnostics    | Todo   |       |
| 5 — Integration tests            | Todo   |       |

---

## Step 1 — Schema types and `initializationOptions` plumbing

Wire up the schema before writing any feature logic. After this step the server
accepts a `frontmatterSchema` in `initializationOptions` without crashing;
behavior is otherwise unchanged.

**Deliverables:**

- `FrontmatterFieldSchema` and `FrontmatterSchema` types added to
  `src/server/mod.rs` (or a new `src/schema.rs` if cleaner)
- `InitOptions.frontmatter_schema: Option<FrontmatterSchema>` field added
- `Config.frontmatter_schema: Option<FrontmatterSchema>` field added;
  populated from `InitOptions` in `Config::from_params`
- `compute_diagnostics` signature extended to accept
  `schema: Option<&FrontmatterSchema>` — passes `None` for now, all callers
  updated
- `handle_completion` signature extended to accept
  `schema: Option<&FrontmatterSchema>` — passes `None` for now, all callers
  updated

**Unit tests:**

| Test                    | What it verifies                                                    |
| ----------------------- | ------------------------------------------------------------------- |
| `init_opts_no_schema`   | Missing `frontmatterSchema` → `Config.frontmatter_schema` is `None` |
| `init_opts_with_schema` | Full schema in `initializationOptions` → fields parsed correctly    |

> **Manual checkpoint:** open the editor, confirm the server starts cleanly when
> `initializationOptions` includes a `frontmatterSchema` block. Check the LSP
> log shows no parse errors.

---

## Step 2 — Parser extension: extract all frontmatter fields

Teach the parser to record every key-value pair so downstream features can
inspect frontmatter without re-parsing the raw text.

**Deliverables:**

- `FrontmatterField { key, key_range, value: Option<String>, value_range: Option<LspRange> }`
  added to `src/parser/mod.rs`
- `Frontmatter.fields: Vec<FrontmatterField>` added
- `extract_frontmatter_fields(block: &str, block_start: usize, line_index: &LineIndex)`
  implemented; handles:
  - `key: scalar` → `value: Some(scalar)`, `value_range: Some(...)`
  - `key:` (empty), `key: |`, `key: >`, `key: [...]` → `value: None`
  - Lines without `:` → skipped
- `parse()` updated to call `extract_frontmatter_fields` and populate
  `frontmatter.fields`

**Unit tests** (in `src/parser/tests.rs`):

| Test                          | What it verifies                              |
| ----------------------------- | --------------------------------------------- | --------------- |
| `fields_scalar_values`        | `key: value` → correct key, value, and ranges |
| `fields_empty_value`          | `key:` → `value: None`                        |
| `fields_block_scalar_skipped` | `key:                                         | `→`value: None` |
| `fields_inline_list_skipped`  | `key: [a, b]` → `value: None`                 |
| `fields_quoted_value`         | `key: "hello"` → value without quotes         |
| `fields_key_range_correct`    | key_range covers only the key name            |
| `fields_value_range_correct`  | value_range covers only the scalar text       |
| `fields_no_frontmatter`       | No `---` block → `fields` is empty            |

> **Manual checkpoint:** run `cargo run -- parse <file>` on a note with
> frontmatter and verify the parse output includes all keys.

---

## Step 3 — Schema-driven completions

Extend `handle_completion` to offer key-name and enum-value completions when a
schema is configured.

**Deliverables:**

- `check_schema_value_trigger(content, pos, schema) -> Option<Vec<CompletionItem>>`
  — returns enum completions when cursor is after `key: ` in frontmatter and
  `key` has enum values; skips `tags:` (handled by existing trigger)
- `check_schema_key_trigger(content, pos, schema, existing_fields) -> Option<Vec<CompletionItem>>`
  — returns key-name completions (kind: `PROPERTY`) when cursor is on a blank
  or keyless frontmatter line; excludes keys already present in `existing_fields`
- Both helpers called in `handle_completion` after the existing tag and
  wiki-link checks; `schema: None` → both return `None`, behavior unchanged

**Unit tests** (in `src/handlers.rs`):

| Test                                     | What it verifies                                            |
| ---------------------------------------- | ----------------------------------------------------------- |
| `schema_value_completion_enum`           | Cursor after `status: ` → enum value items returned         |
| `schema_value_completion_no_enum`        | Key in schema with no `enum` → no value completions         |
| `schema_value_completion_unknown_key`    | Key not in schema → no value completions                    |
| `schema_key_completion`                  | Blank line in frontmatter → absent schema keys offered      |
| `schema_key_completion_excludes_present` | Key already in frontmatter → not offered in key completions |
| `schema_no_schema_unchanged`             | `schema: None` → existing completion behaviour unchanged    |

> **Manual checkpoint:** configure a schema with a `status` enum in the editor.
> Open a note, type `status: ` in the frontmatter — enum values appear. Type a
> blank line in the frontmatter — key names appear.

---

## Step 4 — Schema-driven diagnostics

Extend `compute_diagnostics` to check frontmatter against the schema.

**Deliverables:**

- Three new diagnostic paths in `compute_diagnostics`:
  1. **Unknown key**: field key not in `schema.properties` → Warning at `key_range`
  2. **Invalid enum value**: value not in `enum_values` → Warning at `value_range`
     (falls back to `key_range` when `value_range` is `None`)
  3. **Missing required key**: key in `schema.required` absent from frontmatter →
     Warning at `(0,0)–(0,3)` (the opening `---` delimiter)
- `schema: None` → no new diagnostics; existing link/anchor diagnostics unchanged
- All `publish_diagnostics` / `compute_diagnostics` call sites updated to thread
  the schema through from `Config`

**Unit tests** (in `src/handlers.rs`):

| Test                             | What it verifies                                                    |
| -------------------------------- | ------------------------------------------------------------------- |
| `schema_diag_unknown_key`        | Key not in schema → Warning at key_range                            |
| `schema_diag_invalid_enum_value` | Value not in enum → Warning at value_range                          |
| `schema_diag_missing_required`   | Required key absent → Warning at (0,0)–(0,3)                        |
| `schema_diag_no_schema`          | `schema: None` → zero schema diagnostics                            |
| `schema_diag_valid_note`         | All keys valid, all required present → no schema diagnostics        |
| `schema_diag_no_frontmatter`     | Note has no frontmatter, `required` non-empty → diagnostic at (0,0) |

> **Manual checkpoint:** save a note that violates the schema (unknown key,
> bad value, missing required key). Confirm diagnostics appear in the editor.
> Fix each violation and confirm the diagnostic clears.

---

## Step 5 — Integration tests

End-to-end tests over the full LSP message loop.

**Deliverables:**

- `tests/frontmatter_schema.rs` with all integration tests

| Test                                      | What it verifies                                                   |
| ----------------------------------------- | ------------------------------------------------------------------ |
| `schema_value_completion_round_trip`      | Cursor after `status: ` in frontmatter → enum items in response    |
| `schema_key_completion_round_trip`        | Blank line in frontmatter → schema key items in response           |
| `schema_diag_invalid_value_round_trip`    | Invalid enum value → diagnostic in `publishDiagnostics` after open |
| `schema_diag_missing_required_round_trip` | Missing required key → diagnostic in `publishDiagnostics`          |

- `cargo test` passes, `cargo clippy -- -D warnings` clean

> **Manual checkpoint (full session):** open the editor on a vault with the
> schema configured. Verify completions and diagnostics work together: fix a
> broken frontmatter field, confirm the diagnostic clears and completions work
> at that position. Confirm v0.7 backlinks lens and v0.6 code actions are
> unaffected.

---

## Done — v0.8 complete

| Story | Feature                        | Delivered in step |
| ----- | ------------------------------ | ----------------- |
| US-24 | Frontmatter schema completions | Step 3            |
| US-24 | Frontmatter schema diagnostics | Step 4            |
