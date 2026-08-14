# v0.16 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the CLI's output should be manually verified.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                                           | Status | Notes |
| -------------------------------------------------------------- | ------ | ----- |
| 1 — Combined-distance data model and helpers                   | Todo   |       |
| 2 — `rank_link_candidates`/`rank_anchor_candidates`            | Todo   |       |
| 3 — `unambiguous_winner`/`text_mismatch` + `suggest_*_fix`     | Todo   |       |
| 4 — `FixSuggestion.text_distance` + diagnostic `text_mismatch` | Todo   |       |
| 5 — Skill doc update                                           | Todo   |       |
| 6 — Integration tests                                          | Todo   |       |

---

## Step 1 — Combined-distance data model and helpers

Lays down `RankedCandidate<T>`, `normalized_distance`, and `combined_distance`
in isolation — pure functions with no caller yet, so they're fully testable
before anything in the codebase depends on them.

**Deliverables:**

- `src/handlers.rs`: `struct RankedCandidate<T> { candidate: T, path_distance: usize, text_distance: Option<usize>, combined: f64 }`
- `src/handlers.rs`: `const PATH_WEIGHT: f64 = 0.5;` / `const TEXT_WEIGHT: f64 = 0.5;`
- `src/handlers.rs`: `fn normalized_distance(distance: usize, a: &str, b: &str) -> f64`
- `src/handlers.rs`: `fn combined_distance(path_distance: usize, path_a: &str, path_b: &str, text_distance: Option<usize>, text_a: &str, text_b: &str) -> f64`

This step uses TDD:

1. Write all unit tests for this step first — stub the four items above so
   the test file compiles (`normalized_distance`/`combined_distance` can
   return `0.0` unconditionally as a stub).
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement until tests pass, then run `cargo clippy -- -D warnings`.

**Unit tests:**

| Test                                                            | What it verifies                                                                             |
| --------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `normalized_distance_divides_by_longer_string_length`           | `normalized_distance(2, "abcd", "wxyz")` == `0.5`                                            |
| `normalized_distance_handles_empty_strings_without_div_by_zero` | Both inputs empty → returns `0.0`, no panic (the `.max(1)` floor)                            |
| `combined_distance_falls_back_to_path_term_when_no_text_signal` | `text_distance: None` → result equals `normalized_distance(path_distance, path_a, path_b)`   |
| `combined_distance_blends_both_terms_when_text_signal_present`  | `text_distance: Some(d)` → result equals `PATH_WEIGHT * path_norm + TEXT_WEIGHT * text_norm` |

> **Manual checkpoint:** No editor-observable behavior yet — covered entirely
> by unit tests in this step; nothing calls these functions until Step 2.

---

## Step 2 — `rank_link_candidates`/`rank_anchor_candidates` gain the text signal

Wires `RankedCandidate`/`combined_distance` into the two existing ranking
functions, changing their signatures and return types. Both functions are
private (`fn`, not `pub(crate) fn`), so this step's tests exercise them
directly from within `src/handlers.rs`'s own test module, same as today.

**Deliverables:**

- `src/handlers.rs`: `fn file_stem(path: &Path) -> &str` — small helper, lossy `Path::file_stem`, `""` on a path with none
- `src/handlers.rs`: `rank_link_candidates(broken_target: &str, link_text: &str, source: &Path, index: &NoteIndex) -> Vec<RankedCandidate<String>>`
- `src/handlers.rs`: `rank_anchor_candidates<'a>(broken_slug: &str, link_text: &str, target_note: &'a parser::Note) -> Vec<RankedCandidate<&'a parser::Heading>>`
- Update the two existing call sites inside `src/handlers.rs` (`suggest_link_fix`,
  `suggest_anchor_fix`, `compute_diagnostics_with_suggestions`) to pass
  `link_text` through and adapt to the new return type — kept compiling as a
  straight pass-through for now; their own signature/behavior changes are
  Steps 3–4.

TDD cycle, same as Step 1: write tests against the new signatures first
(they won't compile until the signature change lands, which is fine — the
signature change and its own unit tests land together in this step, not
split across a red/green boundary that can't compile), confirm the new
distance-related assertions fail before this step's ranking-order logic is
implemented, then implement.

**Unit tests:**

| Test                                                                 | What it verifies                                                                                                           |
| -------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `rank_link_candidates_orders_by_combined_not_raw_path_distance`      | A candidate with worse raw `path_distance` but matching link text ranks above a same-shape decoy with better path distance |
| `rank_link_candidates_skips_text_term_for_empty_link_text`           | Empty `link_text` → every candidate's `text_distance` is `None`, order matches the pre-release path-only ranking           |
| `rank_link_candidates_compares_text_against_file_stem_not_full_path` | A deeply nested candidate's text distance is unaffected by its directory depth                                             |
| `rank_anchor_candidates_orders_by_combined_including_link_text_term` | A heading matching link text outranks a same-shape decoy heading with a closer broken-slug distance                        |

> **Manual checkpoint:** No editor-observable behavior yet — `suggest_*_fix`
> and `compute_diagnostics_with_suggestions` still compile and behave as
> before this step (pass-through only); covered by unit tests. First
> observable CLI behavior lands in Step 3.

---

## Step 3 — `unambiguous_winner`/`text_mismatch` and `suggest_link_fix`/`suggest_anchor_fix`

Replaces the inline tie-check in both `suggest_*_fix` functions with the
shared `unambiguous_winner` helper (which folds in `text_mismatch`), and adds
the `link_text: &str` parameter to both public signatures. This is the step
where `knap fix`'s auto-apply behavior actually changes.

**Deliverables:**

- `src/handlers.rs`: `fn unambiguous_winner<T: PartialEq>(ranked: &[RankedCandidate<T>]) -> Option<&T>`
- `src/handlers.rs`: `fn text_mismatch<T: PartialEq>(ranked: &[RankedCandidate<T>]) -> bool`
- `src/handlers.rs`: `suggest_link_fix(broken_target: &str, link_text: &str, source: &Path, index: &NoteIndex) -> Option<String>` (new parameter)
- `src/handlers.rs`: `suggest_anchor_fix<'a>(broken_slug: &str, link_text: &str, target_note: &'a parser::Note) -> Option<&'a parser::Heading>` (new parameter)
- `src/cli/fix.rs:95` — pass `&link.text` into `suggest_link_fix`
- `src/cli/fix.rs:133` — pass `&link.text` into `suggest_anchor_fix`
- Update every existing `suggest_link_fix`/`suggest_anchor_fix` test call site
  in `src/handlers.rs`'s test module for the new parameter (pass `""` where a
  test doesn't care about the text signal, preserving today's behavior)

TDD cycle: write the new/updated unit tests first (including the Trial 4
regression test), confirm `unambiguous_winner_none_when_text_mismatch_even_with_strict_winner`
and the regression test fail against a stub `unambiguous_winner` that always
returns the strict-winner without checking `text_mismatch`, then implement
the real gate.

**Unit tests:**

| Test                                                                 | What it verifies                                                                                                   |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `unambiguous_winner_none_on_tied_combined_score`                     | Two candidates tied on `combined` → `None`, same contract as the pre-release tie case                              |
| `unambiguous_winner_none_when_text_mismatch_even_with_strict_winner` | Combined score has a strict single winner, but `text_mismatch` is true → `None`                                    |
| `unambiguous_winner_some_when_signals_agree`                         | Combined winner and text-only winner are the same candidate → `Some(winner)`                                       |
| `text_mismatch_false_when_no_candidate_has_a_text_signal`            | All `text_distance: None` → `false`, never blocks auto-apply on link text alone                                    |
| `text_mismatch_true_when_top_combined_and_top_text_disagree`         | Top-`combined` candidate differs from the `min_by_key(text_distance)` candidate → `true`                           |
| `suggest_link_fix_declines_the_trial_4_sync_835_case`                | Regression: target near `sync-800.md` by path distance, link text `"Sync 835"` → returns `None`, not `sync-800.md` |
| `suggest_link_fix_still_repoints_when_signals_agree`                 | Unchanged pre-release case: unambiguous path winner whose name also matches link text → still repoints             |
| `suggest_anchor_fix_declines_when_text_mismatch`                     | Mirrors the link regression test for the anchor path                                                               |

> **Manual checkpoint:** In a scratch vault with a `sync-835.md`/`sync-800.md`
> decoy pair and a broken link reading `[Sync 835](sync-800-old.md)`, run
> `knap fix --dry-run .`. Before this step it plans a repoint to
> `sync-800.md`; after this step it plans creating a stub instead (declines
> to repoint) — confirm the printed plan says `create` for that link, not
> `repoint`.

---

## Step 4 — `FixSuggestion.text_distance` and diagnostic `text_mismatch`

Wires the new signals into `lint --suggest`'s JSON output — the step where
an agent (not just `knap fix`'s auto-apply) can actually see the two signals
and the disagreement flag.

**Deliverables:**

- `src/handlers.rs`: `FixSuggestion` gains `text_distance: Option<usize>`
- `src/handlers.rs`: `compute_diagnostics_with_suggestions` passes `link.text`
  into `rank_link_candidates`/`rank_anchor_candidates`, sets each
  `FixSuggestion.text_distance` from `RankedCandidate.text_distance`, and
  attaches `data.text_mismatch: true` only when `text_mismatch(&ranked)` is
  true (omitted entirely when false)

TDD cycle: write the JSON-shape tests first against the current (pre-step)
`FixSuggestion`/`compute_diagnostics_with_suggestions` — confirm they fail
(missing field, missing/always-present key), then implement.

**Unit tests:**

| Test                                                                  | What it verifies                                                                         |
| --------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `compute_diagnostics_with_suggestions_includes_text_distance_field`   | Every `FixSuggestion` in `data.suggestions` carries `text_distance` (`Some` or `None`)   |
| `compute_diagnostics_with_suggestions_sets_text_mismatch_on_data`     | A diagnostic whose ranking has `text_mismatch` true carries `data.text_mismatch == true` |
| `compute_diagnostics_with_suggestions_omits_text_mismatch_when_false` | A diagnostic with agreeing signals has no `text_mismatch` key in `data` at all           |

> **Manual checkpoint:** In the same scratch vault as Step 3's checkpoint,
> run `knap lint . --json --suggest`. Confirm the `broken-link` diagnostic
> for `[Sync 835](sync-800-old.md)` shows `"text_mismatch": true` in `data`,
> and every entry in `data.suggestions` now has a `text_distance` key
> alongside `distance`.

---

## Step 5 — Skill doc update

No source change — updates `skill/knap/SKILL.md` to document the new
`text_distance`/`text_mismatch` fields so an agent reading the skill knows
what they mean and how to act on them.

**Deliverables:**

- `skill/knap/SKILL.md`: `--suggest` example JSON gains `text_distance` on
  each suggestion and a `text_mismatch: true` example on the mismatched one
- `skill/knap/SKILL.md`: "How to pick, not just that you must" paragraph
  extended with the `data.text_mismatch: true` hard-stop guidance (see the
  design doc's Skill Changes section for the exact wording)

No unit tests — this is documentation prose, not code. Verified by re-reading
the doc against Step 4's actual JSON output for consistency.

> **Manual checkpoint:** Diff the updated `--suggest` example in
> `skill/knap/SKILL.md` against the real `knap lint . --json --suggest`
> output from Step 4's checkpoint on the scratch vault — field names and
> shapes must match exactly.

---

## Step 6 — Integration tests

End-to-end tests over the full CLI invocation, using a fixture vault with
the `sync-835.md`/`sync-800.md`-shaped decoy pair from Steps 3–4's manual
checkpoints, committed as a test fixture rather than hand-built per test.
Always the last step.

**Deliverables:**

- `tests/cli.rs`: fixture vault under `tests/fixtures/` (or reuse/extend an
  existing lint-suggest fixture if one already covers a similar shape) with
  the decoy pair and a link-text-mismatched broken link
- All three integration tests below
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                                                 | What it verifies                                                                                                                                 |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `lint_suggest_reports_text_mismatch_for_decoy_and_correct_candidate` | `knap lint --json --suggest` on the fixture reports `text_mismatch: true` for the mismatched diagnostic                                          |
| `fix_declines_repoint_when_text_mismatch_leaves_stub_fallback`       | `knap fix` on the fixture creates a stub instead of repointing to the decoy                                                                      |
| `lint_fix_reports_stub_fallback_not_wrong_repoint_for_mismatch_case` | `knap lint --fix --suggest --json` on the fixture shows the stub in `fixes_applied`, and the still-open diagnostic still carries `text_mismatch` |

> **Manual checkpoint (full session):** Regenerate the Trial 4 benchmark
> vault (`examples/gen_bench_vault.rs --seed 1 --notes 200 --broken-links 12
--broken-anchors 8`) and run `knap lint --json --suggest .` over it.
> Confirm the 4 specific mismatches Trial 4 found by hand (`reference/
deployment.md` "Workflow", `reference/billing.md` "Sync 835",
> `projects/gateway.md` "Storage", `projects/retrospective.md` "Incident
> 954") all carry `text_mismatch: true`, and that `knap fix --dry-run .`'s
> plan no longer proposes repointing any of the four to the wrong decoy.
> Earlier releases' behavior (rename-*, plain `knap lint`, `knap apply`) is
> unaffected — spot-check one `rename-file` round trip still works.

---

## Done — v0.16 complete

| Story  | Feature                                                                           | Delivered in step |
| ------ | --------------------------------------------------------------------------------- | ----------------- |
| US-D20 | Text-aware combined ranking, `text_mismatch` flag, auto-apply decline on mismatch | Steps 1–4         |
