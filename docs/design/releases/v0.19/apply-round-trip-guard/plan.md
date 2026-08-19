# v0.19 Implementation Plan — Apply Round-Trip Guard

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the CLI should be manually verified.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                           | Status | Notes |
| ---------------------------------------------- | ------ | ----- |
| 1 — `validate_link_round_trip` helper          | Done   |       |
| 2 — Wire the guardrail into `apply_one`        | Done   |       |
| 3 — Name the failing operation in batch errors | Done   |       |
| 4 — Integration tests                          | Done   |       |
| 5 — SKILL.md amendment                         | Done   |       |

---

## Step 1 — `validate_link_round_trip` helper

Add the standalone re-parse check with no caller wired up yet, so its
behavior is nailed down in isolation before `apply_one` depends on it. This
is the regression-test step: the tests below must fail against the
not-yet-written function first, to prove they actually exercise the check
rather than passing vacuously.

This step uses TDD:

1. Write all three unit tests below first — stub
   `fn validate_link_round_trip(path: &Path, content: &str, expected_line: u32, matches: impl Fn(&parser::MarkdownLink) -> bool) -> anyhow::Result<()>`
   as `todo!()` (or `Ok(())`, whichever compiles) so the file compiles.
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement the function (re-parse via `parser::parse`, search
   `note.md_links` for a match on `expected_line` and `matches`) until the
   tests pass, then run `cargo clippy -- -D warnings`.

**Deliverables:**

- `validate_link_round_trip` added to `src/cli/apply.rs`, private, not yet
  called from `apply_one`

**Unit tests:**

| Test                                                     | What it verifies                                                                                                                                                       |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `validate_link_round_trip_accepts_well_formed_link`      | `Ok(())` when the expected line holds a `MarkdownLink` satisfying `matches`                                                                                            |
| `validate_link_round_trip_rejects_missing_link_at_line`  | `Err` when `content` has no link at `expected_line` at all — construct this input as a truncated `[text](target` with no closing `)`, the exact shape Trial 6 produced |
| `validate_link_round_trip_rejects_link_with_wrong_field` | `Err` when a link exists at the line but `matches` returns `false` (right shape, wrong target/anchor value)                                                            |

> **Manual checkpoint:** No editor checkpoint — this is a pure function with
> no CLI surface yet; covered entirely by the unit tests above.

---

## Step 2 — Wire the guardrail into `apply_one`

Call `validate_link_round_trip` from the `RepointLink` and `RepointAnchor`
arms of `apply_one`, right after `edit::apply` writes the file, rejecting the
operation with a `.context(...)` before it returns `Ok`. This is where the
guardrail actually starts protecting `knap apply` batches.

TDD:

1. Write all five unit tests below first (extending the ones already in
   `src/cli/apply.rs`'s `#[cfg(test)] mod tests`), including one exercising
   the `<...>`-escaped-target edge case — the target/anchor value the test
   asserts on is the _decoded_ form the parser reports back, not the raw
   string handed to `compute_link_fix`.
2. Run `cargo test` and confirm the new tests **fail** (the two "rejects a
   corrupt range" tests should currently pass silently with a corrupted file
   on disk instead of erroring — that's the bug this step fixes).
3. Implement the two `apply_one` arm changes until all five tests pass, then
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `apply_one`'s `ChangeOp::RepointLink` arm re-reads the file after
  `edit::apply` and calls `validate_link_round_trip`, erroring with
  `.context("repoint-link produced unparseable markdown")` on failure
- `apply_one`'s `ChangeOp::RepointAnchor` arm does the same with
  `.context("repoint-anchor produced unparseable markdown")`

**Unit tests:**

| Test                                                                   | What it verifies                                                                                                                                                    |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apply_one_repoint_link_rejects_range_that_eats_closing_paren`         | A `range` extended past the target into the closing `)` makes `apply_one` return `Err` instead of writing the corrupted text                                        |
| `apply_one_repoint_anchor_rejects_range_that_eats_closing_paren`       | Same, for `RepointAnchor` — mirrors Trial 6's actual failure shape (`+1`/`+3` skew eating the closing `)`)                                                          |
| `apply_one_repoint_link_accepts_target_needing_angle_bracket_escaping` | A `target` containing a space (wrapped `<...>` by `compute_link_fix`) still validates successfully                                                                  |
| `apply_one_repoint_link_still_succeeds_on_a_correct_range`             | Existing correct-range behavior (already covered by `apply_one_repoint_link_replaces_target_range_with_new_target`) is unaffected — no regression on the happy path |
| `apply_one_repoint_anchor_still_succeeds_on_a_correct_range`           | Same, for `RepointAnchor` (already covered by `apply_one_repoint_anchor_replaces_range_with_bare_slug`) — confirm it still passes unmodified                        |

> **Manual checkpoint:** In a scratch vault with a link whose visible text
> doesn't match its target (e.g. `[Overview](topics/template-90.md#dashboard-overview)`
> where the anchor is actually broken), run
> `echo '[{"op":"repoint-anchor","file":"a.md","range":{"start":{"line":0,"character":99},"end":{"line":0,"character":199}},"anchor":"real-anchor"}]' | knap apply`
> with a deliberately-wrong range (past the end of the line) and confirm the
> command exits non-zero with a "produced unparseable markdown" message,
> and that `a.md` on disk is byte-for-byte unchanged from before the command
> ran.

---

## Step 3 — Name the failing operation in batch errors

Change `run`'s operation loop from a plain iteration to `enumerate()`, so the
error context names both the operation's 1-based position and its kind. Small
and mechanical, but needed before Step 4's integration tests can assert on
the error text.

No TDD cycle — this is a one-line context-string change with no new branch to
regression-test in isolation; it's exercised end-to-end by Step 4's
integration test that asserts on the error message.

**Deliverables:**

- `run`'s `for op in &ops { ... }` loop becomes
  `for (i, op) in ops.iter().enumerate() { ... }`, with the `.with_context`
  closure changed to `format!("operation {} ({})", i + 1, op.kind())`

**Unit tests:**

| Test                                                         | What it verifies                                                                                                                          |
| ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `run_error_names_the_failing_operation_by_position_and_kind` | For a batch where the 2nd operation is a corrupt `repoint-anchor`, the returned error's message contains `"operation 2 (repoint-anchor)"` |

> **Manual checkpoint:** No editor checkpoint — the error-message shape is a
> CLI/stderr concern, verified in Step 4's integration tests and this step's
> unit test.

---

## Step 4 — Integration tests

End-to-end tests over the `knap apply` binary via stdin, alongside the
existing `apply_batch_*`/`apply_*` tests in `tests/cli.rs`.

**Deliverables:**

- Two new `#[test]` functions added to `tests/cli.rs`
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                                         | What it verifies                                                                                                                                                                                                                                                                                    |
| ------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apply_cli_rejects_corrupt_repoint_anchor_and_exits_nonzero` | Running `knap apply` with a batch whose lone `repoint-anchor` op carries a range that eats the closing `)` exits non-zero, and the target file's content on disk is byte-for-byte identical to before the command ran (same `snapshot_dir` pattern as `apply_all_or_nothing_rolls_back_on_failure`) |
| `apply_cli_reports_which_operation_failed_in_stderr`         | For a batch with a valid op followed by a corrupt `repoint-link` op, stderr contains `"operation 2 (repoint-link)"`                                                                                                                                                                                 |

> **Manual checkpoint (full session):** Reproduce Trial 6's exact failure
> shape: build a small vault with a broken anchor, run
> `knap lint . --json --suggest` to get its real diagnostic `range`, then
> pipe a batch to `knap apply --json` using that range shifted by
> `+1`/`+3` on `start.character`/`end.character` (the skew the trial's
> transcript found). Confirm the command errors and the file is untouched.
> Then run the same batch with the diagnostic's _actual_ `range`, unmodified,
> and confirm it succeeds and `knap lint .` reports clean afterward — the
> golden path from Trial 6's other 3 correctly-applied anchors must still
> work.

---

## Step 5 — SKILL.md amendment

Documentation-only step: replace the sentence in the "`lint --suggest` →
pick → `apply` round trip" example with the explicit instruction and worked
error example from the design doc. No code changes, so no unit/integration
tests — verified by proofreading against the design doc's exact text and
confirming the new example's JSON/output are copy-paste consistent with
Step 4's actual CLI output.

**Deliverables:**

- `skill/knap/SKILL.md`'s round-trip example section updated per the design
  doc's [Skill Documentation Changes](design.md#skill-documentation-changes)

**Unit tests:** none — documentation only.

> **Manual checkpoint:** Diff the new `SKILL.md` section against the design
> doc's proposed text; confirm the error example's JSON batch and error
> output match what `apply_cli_reports_which_operation_failed_in_stderr`
> (Step 4) actually produces, not a hand-typed approximation.

---

## Done — v0.19 complete

| Story  | Feature                                                                                  | Delivered in step |
| ------ | ---------------------------------------------------------------------------------------- | ----------------- |
| US-D21 | `knap apply` rejects a `repoint-link`/`repoint-anchor` op producing unparseable markdown | Step 2            |
| US-D15 | `SKILL.md` instructs copying `range` verbatim, never recomputing it                      | Step 5            |
