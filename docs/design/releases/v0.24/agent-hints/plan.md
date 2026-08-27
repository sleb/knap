# v0.24 Implementation Plan — Agent Hints

Describes the order in which changes are made, what is tested after each step,
and the checkpoints for confirming the CLI's output against a real terminal.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                                | Status | Notes |
| ------------------------------------------------------ | ------ | ----- |
| 1 — `pub(crate)` codes + shared `hint` builders         | Done   |       |
| 2 — `lint` hint                                         | Done   |       |
| 3 — `apply` hint + error text                           | Done   |       |
| 4 — `rename-*` gains `--json` + hint                    | Done   |       |
| 5 — `index` hint                                        | Done   |       |
| 6 — Integration tests                                   | Done   |       |

---

## Step 1 — `pub(crate)` codes + shared `hint` builders

Introduce `src/cli/hint.rs` first — every later step calls into it, and it
has no dependency on any of them. Pure functions over `bool`s, fully testable
in isolation.

This step uses TDD:

1. Add `pub(crate)` to the six `CODE_*` `const` declarations in
   `src/handlers.rs` (`CODE_BROKEN_LINK`, `CODE_BROKEN_ANCHOR`,
   `CODE_MISSING_FRONTMATTER`, `CODE_MISSING_REQUIRED_FIELD`,
   `CODE_INVALID_FIELD_VALUE`, `CODE_UNKNOWN_FIELD`) — a visibility-only
   change, no behavior change; `cargo test` still passes untouched.
2. Write all unit tests for this step first, in a `#[cfg(test)] mod tests`
   block in `src/cli/hint.rs` — stub each function to return `None` (`lint`,
   `apply`, `index`) or a placeholder string (`rename`), so the module
   compiles.
3. Run `cargo test` and confirm the new tests **fail** (stubs don't match
   the expected `Some`/`None`/text-content shape).
4. Implement each function per the design doc until tests pass, then run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `pub(crate)` added to the six `CODE_*` constants in `src/handlers.rs`
- `src/cli/hint.rs`: `lint(has_repointable: bool, has_manual: bool) ->
  Option<String>`, `apply(dry_run: bool) -> Option<String>`, `rename() ->
  Option<String>`, `index(has_broken_link: bool) -> Option<String>` — all
  `pub(crate)`
- `mod hint;` added to `src/cli/mod.rs`

**Unit tests:**

| Test                                          | What it verifies                                                            |
| -------------------------------------------------- | -------------------------------------------------------------------------------- |
| `hint_lint_none_when_neither_flag_set`              | `hint::lint(false, false)` is `None`                                             |
| `hint_lint_repointable_only`                        | `hint::lint(true, false)` mentions `apply`/`rename-*`, not "frontmatter"          |
| `hint_lint_manual_only`                             | `hint::lint(false, true)` mentions "frontmatter", not `apply`/`rename-*`          |
| `hint_lint_both_flags_joins_both_clauses`           | `hint::lint(true, true)` contains both clauses                                    |
| `hint_apply_some_when_not_dry_run`                  | `hint::apply(false)` is `Some` and mentions `knap lint`                           |
| `hint_apply_none_when_dry_run`                      | `hint::apply(true)` is `None`                                                    |
| `hint_rename_is_always_some`                        | `hint::rename()` is `Some` and mentions `rename-*` and `knap lint`                |
| `hint_index_some_when_broken_link`                  | `hint::index(true)` is `Some` and mentions `knap lint --suggest`                  |
| `hint_index_none_when_no_broken_link`               | `hint::index(false)` is `None`                                                   |

> **Manual checkpoint:** No editor or CLI checkpoint yet — nothing calls
> these functions until Step 2; covered entirely by the unit tests above.

---

## Step 2 — `lint` hint

Add `LintReport.hint` and the code-set computation to `src/cli/lint.rs`.
Depends only on Step 1.

This step uses TDD:

1. Write all unit tests for this step first — add `hint:
   Option<String>` (with `#[serde(skip_serializing_if = "Option::is_none")]`)
   to `LintReport`, stub the value as always `None`, so the module compiles
   and the "hint present" tests fail.
2. Run `cargo test` and confirm the new tests **fail**.
3. Compute `has_repointable`/`has_manual` from `files`' diagnostic `code`s,
   call `hint::lint(has_repointable, has_manual)`, wire the result into
   `LintReport` and into the text-mode `println!` block (after the existing
   `problem(s) in N file(s)` line, before the `blocking_count > 0` bail
   check). Run `cargo clippy -- -D warnings`.

**Deliverables:**

- `#[serde(skip_serializing_if = "Option::is_none")] hint: Option<String>`
  field on `LintReport` in `src/cli/lint.rs`
- `run` computes `has_repointable`/`has_manual` from `files`, calls
  `hint::lint`, and prints the result in both `--json` and text mode

**Unit tests:**

| Test                                              | What it verifies                                                                        |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------ |
| `lint_report_hint_absent_when_clean`                    | `LintReport` serialized for zero diagnostics has no `hint` key                              |
| `lint_report_hint_present_for_broken_link`              | `LintReport` for a workspace with one `broken-link` diagnostic has a `hint` key mentioning `apply` |

> **Manual checkpoint:** In a scratch vault with one broken link, run `knap
> lint --json` and confirm the top-level `"hint"` key mentions `apply` and
> `rename-*`; run plain `knap lint` (no `--json`) and confirm a `hint: ...`
> line prints after the `problem(s) in N file(s)` line. Fix the link and
> confirm both `--json` (no `hint` key) and text mode (no `hint:` line)
> go back to today's clean-workspace output.

---

## Step 3 — `apply` hint + error text

Add `ApplyReport.hint` and enrich the round-trip validation error text in
`src/cli/apply.rs`. Depends only on Step 1 (`hint::apply`).

This step uses TDD:

1. Write all unit tests for this step first — add `hint: Option<String>`
   to `ApplyReport` (stub as always `None`) and leave the two
   `validate_link_round_trip(...).context(...)` calls unchanged, so the
   module compiles.
2. Run `cargo test` and confirm the new/changed tests **fail** (`hint` is
   `None` even for a non-dry-run batch; the error message doesn't yet
   mention `lint --suggest`).
3. Set `hint` from `hint::apply(dry_run)`, print it in text mode after the
   existing `N operation(s), N file(s) touched` line, and update both
   `.context(...)` message strings to the enriched text. Run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `#[serde(skip_serializing_if = "Option::is_none")] hint: Option<String>`
  field on `ApplyReport` in `src/cli/apply.rs`
- `run` sets `hint = hint::apply(dry_run)` and prints it in both `--json`
  and text mode
- Both `RepointLink`/`RepointAnchor` arms in `apply_one` update their
  `.context(...)` message to the re-fetch-`range` text from the design doc

**Unit tests:**

| Test                                                                 | What it verifies                                                                            |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `apply_report_hint_absent_under_dry_run`                               | `ApplyReport` serialized under `--dry-run` has no `hint` key                                     |
| `apply_report_hint_present_when_not_dry_run`                           | Same, non-dry-run, has a `hint` key                                                              |
| `apply_one_repoint_link_error_message_names_lint_suggest_rerun`        | `apply_one_repoint_link_rejects_range_that_eats_closing_paren`'s existing error now also contains "re-fetch range from a fresh `knap lint --suggest --json`" |
| `apply_one_repoint_anchor_error_message_names_lint_suggest_rerun`      | Same, for `apply_one_repoint_anchor_rejects_range_that_eats_closing_paren`                       |

> **Manual checkpoint:** Pipe a one-op `repoint-link` batch with a correct
> `range` into `knap apply --json` and confirm the output has a `"hint"`
> key mentioning `knap lint`; then pipe the same batch with `--dry-run` and
> confirm no `"hint"` key. Separately, pipe a batch with a deliberately-wrong
> `range` (eating the closing `)`, as in the existing test fixture) and
> confirm the printed error now says to re-fetch `range` from a fresh `knap
> lint --suggest --json`.

---

## Step 4 — `rename-*` gains `--json` + hint

Add the `--json` flag to `RenameFile`/`RenameHeading`/`RenameTag` and a
`RenameReport` to `src/cli/rename.rs`. Depends only on Step 1
(`hint::rename`).

This step uses TDD:

1. Write all unit tests for this step first, in `src/cli/rename.rs` — stub
   `run_file`/`run_heading`/`run_tag`'s new `json: bool` parameter as
   accepted but ignored (always the existing text path, no new hint line),
   so the tests compile against the new signature and fail on the
   JSON/hint-line assertions.
2. Run `cargo test` and confirm the new tests **fail**.
3. Add `RenameReport`, branch each `run_*` function on `json` to print
   either the existing text line (plus one new `hint::rename()` line) or a
   serialized `RenameReport`, add the `--json` flag to all three
   `Commands` variants in `src/cli/mod.rs`, and thread it through the three
   `Commands::RenameFile`/`RenameHeading`/`RenameTag` match arms. Run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `struct RenameReport { op: &'static str, summary: String, files_touched:
  usize, hint: Option<String> }` in `src/cli/rename.rs`
- `run_file`/`run_heading`/`run_tag` in `src/cli/rename.rs` each gain a
  `json: bool` parameter; under `true`, build and print a `RenameReport`
  (`hint: hint::rename()`) instead of today's `println!` line; under
  `false`, print today's line plus one new `hint::rename()` line
- `#[arg(long)] json: bool` added to `Commands::RenameFile`/`RenameHeading`/
  `RenameTag` in `src/cli/mod.rs`, threaded through `run`'s match arms

**Unit tests:**

| Test                                                                 | What it verifies                                                                             |
| ---------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `rename_report_json_serializes_op_summary_files_touched_and_hint`      | A `RenameReport` for each of `"rename-file"`/`"rename-heading"`/`"rename-tag"` serializes with the right `op` and a non-empty `hint` |
| `run_file_json_true_prints_json_not_text`                              | `rename::run_file(old, new, json: true)` (captured stdout) parses as JSON with `op == "rename-file"`, no plain-text line present |
| `run_file_json_false_preserves_existing_text_output_plus_hint_line`    | `rename::run_file(old, new, json: false)` still prints today's `"{old} → {new} (N file(s) touched)"` line, plus one new `hint: ...` line |
| `run_heading_json_true_prints_json_not_text`                           | Same as `run_file`'s JSON case, for `run_heading`                                                  |
| `run_tag_json_true_prints_json_not_text`                               | Same as `run_file`'s JSON case, for `run_tag`                                                      |

> **Manual checkpoint:** In a scratch vault, run `knap rename-file --json
> old.md new.md` and confirm the output is valid JSON with
> `"op":"rename-file"` and a non-empty `"hint"`; run `knap rename-file
> old2.md new2.md` (no `--json`) on another file pair and confirm the
> existing text line prints unchanged, followed by a new `hint: ...` line.

---

## Step 5 — `index` hint

Wrap `IndexReport`/`NoteSummary` in `IndexOutput` in `src/cli/index.rs`.
Depends only on Step 1 (`hint::index`).

This step uses TDD:

1. Write all unit tests for this step first — introduce `IndexOutput<T>`
   and `has_broken_link` with `has_broken_link` stubbed to always return
   `false`, so the module compiles and the "hint present" tests fail.
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement `has_broken_link` for real, wrap both the file-target and
   directory-target `--json` branches in `run` with `IndexOutput`, and add
   the matching text-mode line to `print_note` (file target) and to the
   end of the whole-workspace listing loop (directory target). Run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `struct IndexOutput<T: Serialize> { #[serde(flatten)] report: T,
  #[serde(skip_serializing_if = "Option::is_none")] hint: Option<String> }`
  in `src/cli/index.rs`
- `fn has_broken_link(links: &[index::LinkSummary]) -> bool` in
  `src/cli/index.rs`
- `run`'s `--json` branches (both file-target and directory-target) wrap
  their existing `NoteSummary`/`IndexReport` value in `IndexOutput` before
  serializing; text-mode branches print the same hint text where applicable

**Unit tests:**

| Test                                                                 | What it verifies                                                                                        |
| ---------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `has_broken_link_true_when_any_link_unresolved`                        | `has_broken_link` returns `true` given a `LinkSummary` slice with one `resolved: None` entry                |
| `has_broken_link_false_when_all_resolved`                              | Returns `false` when every `LinkSummary.resolved` is `Some`                                                 |
| `index_output_hint_present_for_file_target_with_broken_link`           | `IndexOutput<NoteSummary>` for a note with an unresolved link has a `hint` key                              |
| `index_output_hint_absent_for_file_target_all_resolved`                | Same, all links resolved → no `hint` key                                                                    |
| `index_output_hint_present_for_directory_target_with_broken_link`      | `IndexOutput<IndexReport>` with at least one note having a broken link has a `hint` key                     |
| `index_output_top_level_shape_unchanged_by_flatten`                    | Serializing `IndexOutput<NoteSummary>`/`IndexOutput<IndexReport>` still puts `path`/`notes`/etc. at the top level, not nested under a `report` key |

> **Manual checkpoint:** In a scratch vault with one note containing a
> broken link, run `knap index <that-file> --json` and confirm a `"hint"`
> key mentioning `knap lint --suggest`; run `knap index <that-file>` (no
> `--json`) and confirm a `hint: ...` line prints after the note's
> `referenced by:` block. Run `knap index . --json` over the whole vault and
> confirm the same hint appears; fix the link and confirm both `--json` (no
> `hint` key) and text mode (no `hint:` line) return to today's output.

---

## Step 6 — Integration tests

End-to-end tests over the real `knap` binary as a subprocess, confirming
every hint documented above actually reaches stdout through the full CLI,
not just the in-process unit tests. Always last, since it exercises the
fully wired-up binary from Steps 1–5.

**Deliverables:**

- New tests added to `tests/cli.rs` (using the existing `knap()` subprocess
  helper and fixture-vault conventions already in that file)
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                                    | What it verifies                                                                                       |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `lint_json_clean_workspace_has_no_hint`                     | `knap lint --json` on a workspace with no problems has no `hint` key                                         |
| `lint_json_broken_link_has_hint`                             | `knap lint --json` on a fixture with a broken link has a `hint` key mentioning `apply`                       |
| `lint_text_broken_link_prints_hint_line`                     | `knap lint` (no `--json`) stdout contains a `hint: ` line                                                     |
| `apply_json_non_dry_run_has_hint`                            | `knap apply --json` applying a batch has a `hint` key                                                         |
| `apply_json_dry_run_has_no_hint`                             | `knap apply --dry-run --json` has no `hint` key                                                               |
| `apply_stale_range_error_text_mentions_lint_suggest`         | `knap apply` given a stale `range` fails with stderr containing the re-fetch-range hint text                  |
| `rename_file_json_flag_emits_valid_json_with_hint`           | `knap rename-file --json <old> <new>` stdout parses as JSON with `op == "rename-file"` and a `hint` key       |
| `rename_file_text_prints_hint_line`                          | `knap rename-file <old> <new>` (no `--json`) stdout contains a `hint: ` line                                  |
| `index_json_file_target_broken_link_has_hint`                | `knap index <file> --json` against a fixture with one broken link has a `hint` key                            |

> **Manual checkpoint (full session):** From a scratch vault with a
> deliberately broken link, walk the whole loop by hand: `knap lint --json`
> (confirm `hint` mentions `apply`/`rename-*`) → `knap lint --json
> --suggest` → build a `repoint-link` batch from `data.suggestions[0]` →
> `knap apply --json` (confirm `hint` mentions `knap lint`) → `knap lint
> --json` (confirm no `hint` key). Confirm nothing from earlier releases
> regressed — `knap lsp`'s `textDocument/publishDiagnostics` output (checked
> via an editor or `knap check`) carries no `hint` field, since that path
> never calls the new CLI-only code.

---

## Done — v0.24 agent-hints complete

| Story  | Feature                                                          | Delivered in step |
| ------ | ----------------------------------------------------------------- | -------------------- |
| US-D24 | `knap lint` gains a report-level `hint` (text and `--json`)       | Step 2                |
| US-D25 | `knap apply` gains a report-level `hint`; error text enriched     | Step 3                |
| US-D26 | `knap rename-file`/`rename-heading`/`rename-tag` gain `--json` + `hint` | Step 4          |
| US-D27 | `knap index` gains a report-level `hint`                          | Step 5                |
