# v0.8 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                         | Status | Notes |
| -------------------------------------------- | ------ | ----- |
| 1 — Config: FrontmatterSchema type + parsing | Done   |       |
| 2 — Handler signatures: add config param     | Done   |       |
| 3 — Trigger functions                        | Todo   |       |
| 4 — Schema completions                       | Todo   |       |
| 5 — Schema diagnostics                       | Todo   |       |
| 6 — Integration tests                        | Todo   |       |

---

## Step 1 — Config: FrontmatterSchema type + parsing

Add the new runtime types (`SchemaField`, `FrontmatterSchema`), the
deserialization intermediates (`SchemaFieldOpts`, `FrontmatterSchemaOpts`), and
update `InitOptions`, `Config`, and `Config::from_params` in `src/server/mod.rs`.
No handler or diagnostic behavior changes in this step.

**Deliverables:**

- Add `SchemaFieldOpts` struct (deserialization intermediate, only used in `from_params`)
- Add `FrontmatterSchemaOpts` struct with `fields: HashMap<String, SchemaFieldOpts>`,
  `require_frontmatter: bool`, `warn_on_unknown_keys: bool` — all with
  `#[serde(default)]`
- Add `SchemaField` struct with `values: Option<Vec<String>>` and `required: bool`
- Add `FrontmatterSchema` struct with `fields: Vec<(String, SchemaField)>`,
  `require_frontmatter: bool`, `warn_unknown_keys: bool`; implement `Default`
  (empty fields, both flags `false`)
- Add `frontmatter_schema: Option<FrontmatterSchemaOpts>` to `InitOptions`
- Replace the old `frontmatter_schema: Vec<(String, SchemaField)>` in `Config`
  with `frontmatter_schema: FrontmatterSchema`
- In `Config::from_params`: when `frontmatter_schema` is `Some`, convert the
  `HashMap` to a `Vec` sorted alphabetically by key, copy `require_frontmatter`
  and `warn_on_unknown_keys`; when `None`, use `FrontmatterSchema::default()`
- Add `Default` to `Config` (required by step 2 so handler tests can pass
  `&Config::default()`)

**Unit tests** (inline `#[cfg(test)]` block in `src/server/mod.rs`):

| Test                                | What it verifies                                                                                                                                                                                                              |
| ----------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `config_schema_fields_parsed`       | `initializationOptions` with `frontmatterSchema.fields` containing `status` (values + required) and `type` (values only) → `config.frontmatter_schema.fields` has two entries with the correct `values` and `required` fields |
| `config_schema_fields_sorted`       | Fields given as `z`, `a`, `m` → stored as `[("a",..), ("m",..), ("z",..)]`                                                                                                                                                    |
| `config_schema_flags_default_false` | `frontmatterSchema` with only `fields`, no flags → `require_frontmatter` and `warn_unknown_keys` both `false`                                                                                                                 |
| `config_schema_flags_set`           | `frontmatterSchema` with `requireFrontmatter: true` and `warnOnUnknownKeys: true` → both flags `true` in `Config`                                                                                                             |
| `config_schema_absent_uses_default` | `initializationOptions` with no `frontmatterSchema` key → `config.frontmatter_schema` is the default (empty fields, both flags false)                                                                                         |

> **Manual checkpoint:** No editor behavior yet. Run `cargo test` and confirm
> all five new tests pass. Run `cargo clippy -- -D warnings` clean.

---

## Step 2 — Handler signatures: add config param

Add `config: &Config` to `handle_completion`, `compute_diagnostics`, and
`publish_diagnostics`. Update all call sites in `src/server/mod.rs`. This step
introduces no behavior change — the handlers ignore `config` until steps 4 and 5.

This step does not use TDD because it is purely mechanical plumbing with no
observable behavior. The full existing test suite must pass unchanged.

**Deliverables:**

- `handle_completion(params, index, config: &Config)` in `src/handlers.rs` — add
  parameter; it is unused in the body for now
- `compute_diagnostics(path, index, config: &Config)` — add parameter; unused for now
- `publish_diagnostics(paths, index, config: &Config, sender)` — add parameter;
  thread it through to `compute_diagnostics`
- Update every call site in `src/server/mod.rs` to pass `&config`
- Update every test in `src/handlers.rs` that calls `handle_completion` or
  `compute_diagnostics` directly to pass `&Config::default()`

**Unit tests:** None new. `cargo test` output must be identical to step 1.

> **Manual checkpoint:** No editor behavior change. Run `cargo test` and confirm
> the full suite passes. Run `cargo clippy -- -D warnings` clean.

---

## Step 3 — Trigger functions

Add `check_frontmatter_key_trigger` and `check_frontmatter_value_trigger` as
private functions in `src/handlers.rs`. Both are pure text-analysis routines with
no dependency on the index or config, so they can be fully tested in isolation
before the completion handler uses them.

**TDD cycle for this step:**

1. Write all unit tests below first, stubbing each function to return `None` so
   the tests compile and fail.
2. Run `cargo test` and confirm the new tests fail.
3. Implement both functions until all tests pass, then run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `fn check_frontmatter_value_trigger(content: &str, pos: Position) -> Option<(String, String, Range)>`
  in `src/handlers.rs`. Algorithm:
  - Return `None` if `content` does not start with `"---\n"`.
  - Find the first line from line 1 onward whose trimmed form is `"---"` — the
    closing delimiter. Return `None` if none found.
  - Return `None` if `pos.line == 0` or `pos.line >= closing_line`.
  - Locate the first `:` on `pos.line`. Return `None` if absent or if the
    cursor's byte offset is ≤ the colon position (cursor is in key territory).
  - Extract `key` as the text before the colon, trimmed. Return `None` if empty
    or starts with `#`.
  - Return `None` if the line starts with `tags:` (handled by `check_tag_trigger`).
  - Return `None` if the trimmed value after `: ` starts with `[`, `|`, or `>`.
  - `partial` = text from the first non-whitespace after `: ` to the cursor.
  - `replace_range` spans from the start of the partial to the cursor.

- `fn check_frontmatter_key_trigger(content: &str, pos: Position) -> Option<(String, Range)>`
  in `src/handlers.rs`. Algorithm:
  - Same frontmatter-block gate as above.
  - Return `None` if the trimmed line starts with `-` or `#`.
  - Return `None` if the line has a `:` and the cursor's byte offset is past it.
  - `partial` = text from the first non-whitespace on the line to the cursor.
  - `replace_range` spans from the first non-whitespace character to the cursor.

**Unit tests** (`src/handlers.rs`):

| Test                                                 | What it verifies                                                                    |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `check_frontmatter_value_trigger_basic`              | `"---\nstatus: dr\n---\n"` cursor after `dr` → `Some(("status", "dr", range))`      |
| `check_frontmatter_value_trigger_empty_partial`      | `"---\nstatus: \n---\n"` cursor right after `" "` → `Some(("status", "", range))`   |
| `check_frontmatter_value_trigger_before_colon`       | Cursor before the `:` on `status: draft` → `None`                                   |
| `check_frontmatter_value_trigger_no_frontmatter`     | File body with `status: draft` but no `---` block → `None`                          |
| `check_frontmatter_value_trigger_outside_block`      | Cursor on a body line `status: draft` that appears after the closing `---` → `None` |
| `check_frontmatter_value_trigger_tags_key`           | Cursor after `tags: ` → `None`                                                      |
| `check_frontmatter_value_trigger_inline_list`        | `status: [a, b]` with cursor inside the brackets → `None`                           |
| `check_frontmatter_key_trigger_basic`                | `"---\nstat\n---\n"` cursor after `stat` → `Some(("stat", range))`                  |
| `check_frontmatter_key_trigger_blank_line`           | Cursor on a blank frontmatter line → `Some(("", range))`                            |
| `check_frontmatter_key_trigger_on_list_item`         | `"---\n  - foo\n---\n"` cursor on list item → `None`                                |
| `check_frontmatter_key_trigger_in_value`             | Cursor after `: ` on `status: dr` → `None`                                          |
| `check_frontmatter_key_trigger_no_frontmatter`       | File with no `---` block → `None`                                                   |
| `check_frontmatter_key_trigger_on_closing_delimiter` | Cursor on the closing `---` line → `None`                                           |

> **Manual checkpoint:** No editor behavior yet. Run `cargo test` and confirm
> all trigger tests pass. Run `cargo clippy -- -D warnings` clean.

---

## Step 4 — Schema completions

Wire the two trigger functions into `handle_completion`. This is the first step
with observable behavior in an editor.

**TDD cycle for this step:**

1. Write all unit tests below first.
2. Run `cargo test` and confirm they fail.
3. Insert the two new branches in `handle_completion` until all tests pass, then
   run `cargo clippy -- -D warnings`.

**Deliverables:**

- Insert the frontmatter value completion branch between the existing tag trigger
  and anchor trigger (priority step 2 in the handler).
- Insert the frontmatter key completion branch after the directory trigger and
  before the final `return vec![]` (priority step 5).

**Unit tests** (`src/handlers.rs`):

| Test                                                   | What it verifies                                                                                       |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------ |
| `schema_key_completion_offers_all_unused_keys`         | Schema with `status` and `type`; note has empty frontmatter → both keys returned as `FIELD` items      |
| `schema_key_completion_excludes_used_keys`             | Note already has `status:` field → only `type` offered                                                 |
| `schema_key_completion_filters_by_partial`             | Partial `"sta"` typed → only `status` offered                                                          |
| `schema_key_completion_insert_text_has_colon`          | `status` item → `new_text` in `text_edit` is `"status: "`                                              |
| `schema_key_completion_empty_schema_returns_empty`     | No schema fields configured → empty vec                                                                |
| `schema_value_completion_offers_enum_values`           | Schema `status: ["draft","published"]`; cursor after `status: ` → both values offered as `VALUE` items |
| `schema_value_completion_filters_by_partial`           | Partial `"pub"` → only `"published"` offered                                                           |
| `schema_value_completion_partial_is_case_sensitive`    | Partial `"Pub"` with schema value `"published"` (lowercase) → nothing offered                          |
| `schema_value_completion_empty_partial_returns_all`    | Cursor right after `status: ` → all enum values offered                                                |
| `schema_value_completion_no_values_list_returns_empty` | Schema `title: {}` (no `values`); cursor after `title: ` → empty vec                                   |
| `schema_value_completion_unknown_key_returns_empty`    | `foobar: ` not in schema → empty vec                                                                   |
| `schema_value_completion_tags_key_skipped`             | Cursor after `tags: ` → tag trigger takes precedence; value trigger is not reached                     |

> **Manual checkpoint:** Configure `frontmatterSchema` in your editor's
> `initializationOptions` with a `status` key and values `["draft",
"published"]`. In the frontmatter of a note, type `stat` on a new line and
> invoke completion — `status: ` should appear as a `FIELD` item. Select it,
> then type `pub` after the colon and invoke completion — `published` should
> appear. Confirm that `Pub` (capital P) returns nothing (exact-case prefix
> match).

---

## Step 5 — Schema diagnostics

Add schema validation to `compute_diagnostics`. All three checks (required keys,
enum values, unknown keys) live in a single block appended after the existing
link-diagnostics loop.

**TDD cycle for this step:**

1. Write all unit tests below first.
2. Run `cargo test` and confirm they fail.
3. Implement the validation block until all tests pass, then run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- Append the schema validation block to `compute_diagnostics` in `src/handlers.rs`.
  Three sub-checks, all gated by `!schema.fields.is_empty() || schema.require_frontmatter || schema.warn_unknown_keys`:
  1. Notes with no frontmatter + `require_frontmatter: true` → warning at `(0,0)` per missing required key.
  2. Notes with frontmatter → required-key check at `(0,0)` + exact-case enum check at `field.value_range` + optional unknown-key check at `field.key_range`.

**Unit tests** (`src/handlers.rs`):

| Test                                                | What it verifies                                                                                     |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `schema_diag_required_key_absent`                   | Schema `status` required; note has `---\ntitle: x\n---\n` → one warning at (0,0) mentioning `status` |
| `schema_diag_required_key_present_no_warning`       | Note has `status: draft` → no required-key warning                                                   |
| `schema_diag_value_match_is_exact_case`             | Schema allows `["draft","published"]`; note has `status: Draft` → warning produced                   |
| `schema_diag_exact_value_match_no_warning`          | Schema allows `"draft"`; note has `status: draft` → no warning                                       |
| `schema_diag_no_frontmatter_require_off_no_warning` | Required `status` in schema; note has no `---` block; `require_frontmatter: false` → no diagnostic   |
| `schema_diag_no_frontmatter_require_on_warns`       | Same note; `require_frontmatter: true` → warning at (0,0) for `status`                               |
| `schema_diag_unknown_key_warn_off_no_diagnostic`    | Note has `foobar: x`; `warn_unknown_keys: false` (default) → no diagnostic                           |
| `schema_diag_unknown_key_warn_on_warns`             | Same note; `warn_unknown_keys: true` → warning at `foobar` key range                                 |
| `schema_diag_complex_value_skipped`                 | Field with `value: None` (block scalar) and enum constraint → no diagnostic                          |
| `schema_diag_key_match_is_case_insensitive`         | Schema key `Status` (capital); note has `status: draft` in values → no warning                       |
| `schema_empty_no_diagnostics`                       | No schema; arbitrary frontmatter → no schema diagnostics                                             |

> **Manual checkpoint:** In the same editor session, open a note with
> `---\nstatus: Draft\n---\n` (capital D). A warning squiggle should appear under
> `Draft`. Change it to `draft` — the warning should clear. Remove `status:`
> entirely — a warning should appear at the top of the file. Now enable
> `warnOnUnknownKeys: true`, add an unrecognised key `foobar: x`, and confirm a
> warning appears under `foobar`.

---

## Step 6 — Integration tests

End-to-end tests over the full LSP message loop. Always the last step.

**Deliverables:**

- New test functions in `tests/lsp.rs`. A helper initialises the server with a
  `frontmatterSchema` in `initializationOptions` to avoid repeating the full
  `initialize` request in every test.
- `cargo test` passes, `cargo clippy -- -D warnings` clean.

| Test                                                     | What it verifies                                                                                 |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `test_schema_key_completion`                             | `textDocument/completion` in a blank frontmatter key position → schema key items returned        |
| `test_schema_value_completion`                           | `textDocument/completion` after `status: ` with enum schema → allowed values returned            |
| `test_schema_required_key_missing_diagnostic`            | `didOpen` note with frontmatter but no required key → `publishDiagnostics` includes that warning |
| `test_schema_invalid_value_diagnostic`                   | `didOpen` note with value not in enum → `publishDiagnostics` includes the enum warning           |
| `test_schema_valid_note_no_diagnostic`                   | `didOpen` note satisfying all schema rules → no schema warnings in diagnostics                   |
| `test_schema_require_frontmatter_warns_on_missing_block` | `requireFrontmatter: true`; note with no `---` block → required-key warning published            |
| `test_schema_warn_unknown_keys`                          | `warnOnUnknownKeys: true`; note has key absent from schema → warning published                   |
| `test_no_schema_no_extra_diagnostics`                    | Server started without schema; note with arbitrary frontmatter → no schema diagnostics           |

> **Manual checkpoint (full session):** Open a real vault in your editor.
> Configure a schema with at least one required key, one key with enum values,
> `requireFrontmatter: false`, and `warnOnUnknownKeys: false`. Walk the golden
> path: key completions, enum value completions, wrong-value warning, missing
> required-key warning. Enable each flag in turn and confirm new warnings appear
> in the expected notes. Confirm link diagnostics, tag completions, backlinks
> code lens, go-to-definition, and same-file anchor features from earlier
> releases are unaffected.

---

## Done — v0.8 complete

| Story | Feature                                                                                   | Delivered in step |
| ----- | ----------------------------------------------------------------------------------------- | ----------------- |
| US-24 | Frontmatter key completions from schema                                                   | Step 4            |
| US-24 | Frontmatter value completions from schema (exact-case prefix match)                       | Step 4            |
| US-24 | Required-key diagnostics (with opt-in `requireFrontmatter` for notes without frontmatter) | Step 5            |
| US-24 | Enum-value diagnostics (exact-case equality)                                              | Step 5            |
| US-24 | Unknown-key diagnostics (opt-in via `warnOnUnknownKeys`)                                  | Step 5            |
