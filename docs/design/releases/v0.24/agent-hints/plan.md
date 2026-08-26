# v0.24 Implementation Plan — Agent Hints

Describes the order in which changes are made, what is tested after each step,
and the checkpoints for confirming the CLI's output against a real terminal.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                                | Status | Notes |
| ------------------------------------------------------ | ------ | ----- |
| 1 — Shared `hints` builders                            | Todo   |       |
| 2 — `lint --json` hints                                | Todo   |       |
| 3 — `apply --json` hints + error text                  | Todo   |       |
| 4 — `rename-*` gains `--json`                          | Todo   |       |
| 5 — `index --json` hints                               | Todo   |       |
| 6 — Integration tests                                  | Todo   |       |

---

## Step 1 — Shared `hints` builders

Introduce `src/cli/hints.rs` first — every later step calls into it, and it
has no dependency on any of them. Pure functions over `serde_json::Value`,
fully testable in isolation.

This step uses TDD:

1. Write all unit tests for this step first, in a `#[cfg(test)] mod tests`
   block in `src/cli/hints.rs` — stub each builder function (e.g.
   `fn none_clean() -> serde_json::Value { serde_json::json!({}) }`) so the
   module compiles.
2. Run `cargo test` and confirm the new tests **fail** (stubs don't match
   the expected `action`/`reason`/`op`/`target`/`anchor` shape).
3. Implement each builder per the design doc until tests pass, then run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/cli/hints.rs`: `none_clean`, `lint`, `manual`, `disambiguate`,
  `apply_repoint_link`, `apply_repoint_anchor` — all `pub(crate) fn(...) ->
  serde_json::Value`
- `mod hints;` added to `src/cli/mod.rs`

**Unit tests:**

| Test                                                     | What it verifies                                                                        |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `hints_none_clean_has_action_none_reason_clean`              | `none_clean()` serializes to `{"action":"none","reason":"clean"}`                          |
| `hints_lint_carries_given_reason`                            | `lint("confirm_clean")` has `action == "lint"`, `reason == "confirm_clean"`                |
| `hints_manual_carries_given_reason`                          | `manual("missing-frontmatter")` has `action == "manual"`, `reason == "missing-frontmatter"` |
| `hints_disambiguate_carries_given_reason`                    | `disambiguate("text_mismatch")` has `action == "disambiguate"`, `reason == "text_mismatch"` |
| `hints_apply_repoint_link_carries_target`                    | `apply_repoint_link("a.md")` has `action == "apply"`, `op == "repoint-link"`, `target == "a.md"` |
| `hints_apply_repoint_anchor_carries_anchor`                  | `apply_repoint_anchor("#slug")` has `op == "repoint-anchor"`, `anchor == "#slug"`            |

> **Manual checkpoint:** No editor or CLI checkpoint yet — nothing calls
> these builders until Step 2; covered entirely by the unit tests above.

---

## Step 2 — `lint --json` hints

Add `LintReport.hints` and the per-diagnostic `attach_hint` pass to
`src/cli/lint.rs`. Depends only on Step 1.

This step uses TDD:

1. Write all unit tests for this step first — add `hints: Vec<serde_json::Value>`
   to `LintReport` (stub `attach_hint` as a no-op) so the module compiles and
   the tests can construct diagnostics and call it.
2. Run `cargo test` and confirm the new tests **fail** (the no-op leaves
   `data`/`data.hint` unset).
3. Implement `attach_hint` per the design doc's `match`, wire the report-level
   `hints` (`[hints::none_clean()]` when `problem_count == 0`, else `[]`),
   and call `attach_hint(d, suggest_n)` once per diagnostic in `run` before
   building `LintReport`. Run `cargo clippy -- -D warnings`.

**Deliverables:**

- `hints: Vec<serde_json::Value>` field on `LintReport` in `src/cli/lint.rs`
- `fn attach_hint(d: &mut Diagnostic, suggest_n: usize)` in `src/cli/lint.rs`
- `run` calls `attach_hint` on every diagnostic in `files` before
  constructing `LintReport`, and sets `hints` from `problem_count`

**Unit tests:**

| Test                                                                 | What it verifies                                                                        |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `lint_report_hints_is_none_clean_when_zero_diagnostics`                | `LintReport.hints` is `[{"action":"none","reason":"clean"}]` when `problem_count == 0`     |
| `lint_report_hints_is_empty_when_diagnostics_present`                  | `LintReport.hints` is `[]` when `problem_count > 0`                                        |
| `attach_hint_broken_link_no_suggest_hints_rerun_with_suggest`          | `attach_hint` with `suggest_n == 0` on a `broken-link` diagnostic (no `data`) sets `data.hint == {"action":"lint","reason":"rerun_with_suggest"}` |
| `attach_hint_broken_link_suggest_zero_candidates_hints_disambiguate`   | `attach_hint` with `suggest_n > 0` but `data == None` sets `data.hint == {"action":"disambiguate","reason":"no_candidates"}` |
| `attach_hint_broken_link_unambiguous_candidate_hints_apply`            | A `broken-link` diagnostic with `data.suggestions == [{"target":"a.md",...}]` and no `text_mismatch` gets `data.hint == {"action":"apply","op":"repoint-link","target":"a.md"}` |
| `attach_hint_broken_anchor_unambiguous_candidate_hints_apply`          | Same, for `broken-anchor`, using `apply_repoint_anchor` and the suggestion's `#slug` target |
| `attach_hint_text_mismatch_hints_disambiguate`                         | A diagnostic with `data.text_mismatch == true` gets `data.hint == {"action":"disambiguate","reason":"text_mismatch"}` regardless of `suggestions` content |
| `attach_hint_frontmatter_codes_hint_manual_with_code_as_reason`        | Each of `missing-frontmatter`/`missing-required-field`/`invalid-field-value`/`unknown-field` gets `data.hint == {"action":"manual","reason":"<that code>"}` |
| `attach_hint_unrecognized_code_leaves_data_untouched`                  | A diagnostic with an unrecognized `code` has `data` unchanged after `attach_hint`           |

> **Manual checkpoint:** In a scratch vault with one broken link, run
> `knap lint --json --suggest` and confirm the diagnostic's `data.hint` shows
> `{"action":"apply","op":"repoint-link","target":"..."}`; then fix the link
> and confirm plain `knap lint --json` prints `"hints":
> [{"action":"none","reason":"clean"}]`.

---

## Step 3 — `apply --json` hints + error text

Add `ApplyReport.hints` and enrich the round-trip validation error text in
`src/cli/apply.rs`. Depends only on Step 1 (`hints::lint`).

This step uses TDD:

1. Write all unit tests for this step first — add `hints: Vec<serde_json::Value>`
   to `ApplyReport` (stub as always `vec![]`) and leave the two
   `validate_link_round_trip(...).context(...)` calls unchanged, so the
   module compiles.
2. Run `cargo test` and confirm the new/changed tests **fail** (`hints` is
   `vec![]` even for a non-dry-run batch; the error message doesn't yet
   mention `lint --suggest`).
3. Set `hints` from `dry_run` per the design doc, and update both
   `.context(...)` message strings to the enriched text. Run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `hints: Vec<serde_json::Value>` field on `ApplyReport` in `src/cli/apply.rs`
- `run` sets `hints` to `vec![hints::lint("confirm_clean")]` when `!dry_run`,
  else `vec![]`
- Both `RepointLink`/`RepointAnchor` arms in `apply_one` update their
  `.context(...)` message to the re-fetch-`range` text from the design doc

**Unit tests:**

| Test                                                                 | What it verifies                                                                            |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `apply_report_hints_confirm_clean_when_not_dry_run`                    | `ApplyReport.hints` is `[{"action":"lint","reason":"confirm_clean"}]` after a non-dry-run batch  |
| `apply_report_hints_empty_under_dry_run`                               | `ApplyReport.hints` is `[]` under `--dry-run`                                                    |
| `apply_one_repoint_link_error_message_names_lint_suggest_rerun`        | `apply_one_repoint_link_rejects_range_that_eats_closing_paren`'s existing error now also contains "re-fetch range from a fresh `knap lint --suggest --json`" |
| `apply_one_repoint_anchor_error_message_names_lint_suggest_rerun`      | Same, for `apply_one_repoint_anchor_rejects_range_that_eats_closing_paren`                       |

> **Manual checkpoint:** Pipe a one-op `repoint-link` batch with a correct
> `range` into `knap apply --json` and confirm `"hints":
> [{"action":"lint","reason":"confirm_clean"}]` in the output; then pipe the
> same batch with `--dry-run` and confirm `"hints": []`. Separately, pipe a
> batch with a deliberately-wrong `range` (eating the closing `)`, as in the
> existing test fixture) and confirm the printed error now says to re-fetch
> `range` from a fresh `knap lint --suggest --json`.

---

## Step 4 — `rename-*` gains `--json`

Add the `--json` flag to `RenameFile`/`RenameHeading`/`RenameTag` and a
`RenameReport` to `src/cli/rename.rs`. Depends only on Step 1.

This step uses TDD:

1. Write all unit tests for this step first, in `src/cli/rename.rs` — stub
   `run_file`/`run_heading`/`run_tag`'s new `json: bool` parameter as
   accepted but ignored (always the existing text path), so the tests
   compile against the new signature and fail on the JSON assertions.
2. Run `cargo test` and confirm the new tests **fail**.
3. Add `RenameReport`, branch each `run_*` function on `json` to print
   either the existing text line or a serialized `RenameReport`, add the
   `--json` flag to all three `Commands` variants in `src/cli/mod.rs`, and
   thread it through the three `Commands::RenameFile`/`RenameHeading`/
   `RenameTag` match arms. Run `cargo clippy -- -D warnings`.

**Deliverables:**

- `struct RenameReport { op: &'static str, summary: String, files_touched:
  usize, hints: Vec<serde_json::Value> }` in `src/cli/rename.rs`
- `run_file`/`run_heading`/`run_tag` in `src/cli/rename.rs` each gain a
  `json: bool` parameter; under `true`, build and print a `RenameReport`
  (`hints` always `vec![]`) instead of today's `println!` line
- `#[arg(long)] json: bool` added to `Commands::RenameFile`/`RenameHeading`/
  `RenameTag` in `src/cli/mod.rs`, threaded through `run`'s match arms

**Unit tests:**

| Test                                                                 | What it verifies                                                                             |
| ---------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `rename_report_json_serializes_op_summary_files_touched_empty_hints`   | A `RenameReport` for each of `"rename-file"`/`"rename-heading"`/`"rename-tag"` serializes with the right `op` and `hints: []` |
| `run_file_json_true_prints_json_not_text`                              | `rename::run_file(old, new, json: true)` (captured stdout) parses as JSON with `op == "rename-file"`, no plain-text line present |
| `run_file_json_false_preserves_existing_text_output`                   | `rename::run_file(old, new, json: false)` still prints today's `"{old} → {new} (N file(s) touched)"` line, byte-for-byte unchanged |
| `run_heading_json_true_prints_json_not_text`                           | Same as `run_file`'s JSON case, for `run_heading`                                                  |
| `run_tag_json_true_prints_json_not_text`                               | Same as `run_file`'s JSON case, for `run_tag`                                                      |

> **Manual checkpoint:** In a scratch vault, run `knap rename-file --json
> old.md new.md` and confirm the output is valid JSON with
> `"op":"rename-file"` and `"hints":[]`; run the same command without
> `--json` on another file pair and confirm the text line is unchanged from
> before this step.

---

## Step 5 — `index --json` hints

Wrap `IndexReport`/`NoteSummary` in `IndexOutput` in `src/cli/index.rs`.
Depends only on Step 1.

This step uses TDD:

1. Write all unit tests for this step first — introduce `IndexOutput<T>`
   and `has_broken_link` with `has_broken_link` stubbed to always return
   `false`, so the module compiles and the "broken link present" tests fail.
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement `has_broken_link` for real, wrap both the file-target and
   directory-target `--json` branches in `run` with `IndexOutput`. Run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `struct IndexOutput<T: Serialize> { #[serde(flatten)] report: T, hints:
  Vec<serde_json::Value> }` in `src/cli/index.rs`
- `fn has_broken_link(links: &[index::LinkSummary]) -> bool` in
  `src/cli/index.rs`
- `run`'s `--json` branches (both file-target and directory-target) wrap
  their existing `NoteSummary`/`IndexReport` value in `IndexOutput` before
  serializing

**Unit tests:**

| Test                                                                 | What it verifies                                                                                        |
| ---------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `has_broken_link_true_when_any_link_unresolved`                        | `has_broken_link` returns `true` given a `LinkSummary` slice with one `resolved: None` entry                |
| `has_broken_link_false_when_all_resolved`                              | Returns `false` when every `LinkSummary.resolved` is `Some`                                                 |
| `index_output_hints_broken_links_present_for_file_target`              | `IndexOutput<NoteSummary>` for a note with an unresolved link has `hints == [{"action":"lint","reason":"broken_links_present"}]` |
| `index_output_hints_empty_for_file_target_all_resolved`                | Same, all links resolved → `hints == []`                                                                     |
| `index_output_hints_broken_links_present_for_directory_target`         | `IndexOutput<IndexReport>` with at least one note having a broken link has the same hint                     |
| `index_output_top_level_shape_unchanged_by_flatten`                    | Serializing `IndexOutput<NoteSummary>`/`IndexOutput<IndexReport>` still puts `path`/`notes`/etc. at the top level, not nested under a `report` key |

> **Manual checkpoint:** In a scratch vault with one note containing a
> broken link, run `knap index <that-file> --json` and confirm `"hints":
> [{"action":"lint","reason":"broken_links_present"}]`; run `knap index .
> --json` over the whole vault and confirm the same hint appears; fix the
> link and confirm both now print `"hints": []`.

---

## Step 6 — Integration tests

End-to-end tests over the real `knap` binary as a subprocess, confirming
every hint documented above actually reaches stdout/stderr through the full
CLI, not just the in-process unit tests. Always last, since it exercises the
fully wired-up binary from Steps 1–5.

**Deliverables:**

- New tests added to `tests/cli.rs` (using the existing `knap()` subprocess
  helper and fixture-vault conventions already in that file)
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                                    | What it verifies                                                                                       |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `lint_json_clean_workspace_has_none_clean_hint`             | `knap lint --json` on a workspace with no problems has the `none`/`clean` report-level hint                  |
| `lint_json_suggest_broken_link_has_apply_hint`               | `knap lint --json --suggest` on a fixture with one unambiguous broken link has `data.hint.action == "apply"` |
| `apply_json_non_dry_run_has_confirm_clean_hint`              | `knap apply --json` applying a batch has `hints == [{"action":"lint","reason":"confirm_clean"}]`             |
| `apply_json_dry_run_has_empty_hints`                         | `knap apply --dry-run --json` has `hints == []`                                                              |
| `apply_stale_range_error_text_mentions_lint_suggest`         | `knap apply` given a stale `range` fails with stderr containing the re-fetch-range hint text                 |
| `rename_file_json_flag_emits_valid_json`                     | `knap rename-file --json <old> <new>` stdout parses as JSON with `op == "rename-file"`                       |
| `index_json_file_target_broken_link_has_lint_hint`           | `knap index <file> --json` against a fixture with one broken link has the `broken_links_present` hint        |

> **Manual checkpoint (full session):** From a scratch vault with a
> deliberately broken link, walk the whole loop by hand: `knap lint --json
> --suggest` (confirm `data.hint`) → build a `repoint-link` batch from its
> `target` → `knap apply --json` (confirm `hints`) → `knap lint --json`
> (confirm the clean hint). Confirm nothing from earlier releases regressed
> — `knap lsp`'s `textDocument/publishDiagnostics` output (checked via an
> editor or `knap check`) carries no `hint` field, since that path never
> calls the new CLI-only code.

---

## Done — v0.24 agent-hints complete

| Story  | Feature                                                          | Delivered in step |
| ------ | ----------------------------------------------------------------- | ------------------- |
| US-D24 | `knap lint --json` gains a report-level and per-diagnostic `hint` | Step 2               |
| US-D25 | `knap apply --json` gains a report-level `hint`; error text enriched | Step 3            |
| US-D26 | `knap rename-file`/`rename-heading`/`rename-tag` gain `--json`    | Step 4               |
| US-D27 | `knap index --json` gains a report-level `hint`                   | Step 5               |
