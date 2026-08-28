# v0.25 Implementation Plan — Directory-Proximity Ranking

Describes the order in which changes are made, what is tested after each
step, and the checkpoints where the fix is verified against the reproduced
Trial 10 failure.

The guiding principle: each step produces something testable. No step lays
down untested code for the next step to build on.

---

## Status

| Step                                                     | Status | Notes |
| ---------------------------------------------------------- | ------ | ----- |
| 1 — Directory-proximity ranking signal                  | Done   |       |
| 2 — `same_basename` diagnostic flag                     | Done   |       |
| 3 — CLI output for `same_basename`                       | Done   |       |
| 4 — Update docs                                          | Done   |       |
| 5 — Full verification + Trial 11 re-run                  | Partial | Quality gate (test/clippy/fmt/release build) done; Trial 11 benchmark re-run deferred at user's request |

---

## Step 1 — Directory-proximity ranking signal

This is the core fix (US-D28) and comes first because everything else in
this feature — the `same_basename` flag, the skill wording — is only
meaningful once the ranking itself can be trusted to use every signal it has.
This step reproduces Trial 10's `gateway.md` failure as a regression test
before touching the ranking code, per the bug-fix-plan convention.

**TDD flag** — this step follows the write-tests-first cycle:

1. Write all unit tests below first, stubbing `dir_distance`'s signature and
   `combined_distance`'s new `dir_distance: Option<usize>` parameter (and the
   `RankedCandidate.dir_distance` field) so the crate compiles.
2. Run `cargo test` and confirm the new tests fail — in particular,
   `rank_link_candidates_prefers_same_directory_over_closer_raw_path_distance`
   must fail against the *old* two-term `combined_distance` before the dir
   term is wired in, proving the test actually exercises the bug.
3. Implement `dir_distance`, generalize `combined_distance` to the weighted-
   average-of-present-terms form, wire `dir_distance` through
   `rank_link_candidates`, and leave `rank_anchor_candidates` passing
   `dir_distance: None`. Refactor `relative_path` to call `dir_distance`
   internally instead of recomputing `common` inline. Then run
   `cargo clippy -- -D warnings` and confirm clean.

**Deliverables:**

- `dir_distance(from_dir: &Path, to: &Path) -> usize` in `src/handlers.rs`.
- `relative_path` refactored to call `dir_distance` for its up-count.
- `normalized_dir_distance(levels: usize) -> f64` in `src/handlers.rs`.
- `PATH_WEIGHT`/`TEXT_WEIGHT`/`DIR_WEIGHT` all set to `1.0 / 3.0`.
- `combined_distance` generalized to a weighted average over whichever of
  path/text/dir terms are present, taking a new `dir_distance: Option<usize>`
  parameter.
- `RankedCandidate<T>` gains a `dir_distance: Option<usize>` field.
- `rank_link_candidates` computes and passes `dir_distance(source_dir,
  &n.path)` per candidate.
- `rank_anchor_candidates` passes `dir_distance: None` (no behavior change).

**Unit tests:**

| Test                                                                          | What it verifies                                                                                              |
| ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| `dir_distance_same_directory_is_zero`                                        | `dir_distance` returns `0` when `to`'s parent equals `from_dir`                                                |
| `dir_distance_counts_up_levels`                                              | `dir_distance` returns `1`/`2` for one/two parent-directory climbs                                             |
| `relative_path_same_dir` / `relative_path_parent_dir` / `relative_path_subdirectory` | existing tests (unmodified) still pass — confirms the `dir_distance` refactor didn't change `relative_path`'s output |
| `combined_distance_with_all_three_terms_averages_equally`                    | equal thirds produce the plain average of all three normalized terms when each is `Some`                       |
| `combined_distance_falls_back_to_path_and_text_when_dir_distance_is_none`    | `dir_distance: None` reproduces the exact pre-change 50/50 path/text blend (the anchor case)                    |
| `combined_distance_falls_back_to_path_and_dir_when_text_distance_is_none`    | `text_distance: None`, `dir_distance: Some` blends path+dir 50/50, not path alone                               |
| `rank_link_candidates_prefers_same_directory_over_closer_raw_path_distance` | three same-basename candidates at different directory depths (mirrors `gateway.md`'s `storage.md`/`../notes/storage.md`/`../topics/storage.md` shape) — same-directory candidate ranks first despite worse raw `path_distance` |
| `rank_anchor_candidates_unaffected_by_dir_distance_signal`                  | anchor ranking output is byte-identical before/after this change                                                |

> **Manual checkpoint:** In a scratch vault, seed the exact `gateway.md`
> shape by hand (a broken link `../projects/storage-removed.md` in one note,
> plus `storage.md` in the same directory and `notes/storage.md` /
> `topics/storage.md` elsewhere). Run `knap lint . --json --suggest`; confirm
> `suggestions[0].target` is the same-directory `storage.md`, not one of the
> `../`-prefixed decoys.

---

## Step 2 — `same_basename` diagnostic flag

Builds on Step 1's ranking (US-D29). Even with directory proximity added,
some same-basename ties are genuinely ambiguous (three `storage.md` files by
construction, in the benchmark vault) — this step makes that tie visible to
the caller instead of implicit, the same shift `text_mismatch` made for the
text-disagreement case in v0.16.

**TDD flag** — same cycle as Step 1: write the tests below first (stub
`same_basename`'s signature), confirm they fail, then implement.

**Deliverables:**

- `same_basename(suggestions: &[FixSuggestion]) -> bool` in
  `src/handlers.rs`.
- `compute_diagnostics_with_suggestions`'s `broken-link` branch: compute
  `same_basename(&suggestions)` and set `data["same_basename"] = true` when
  it returns `true` (omitted, like `text_mismatch`, when `false`).
- `broken-anchor` branch: no change — never sets `same_basename`.

**Unit tests:**

| Test                                                                          | What it verifies                                                                                    |
| ------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------- |
| `same_basename_true_when_two_suggestions_share_a_stem`                       | returns `true` for `["../notes/storage.md", "storage.md"]`-shaped input                             |
| `same_basename_false_when_all_targets_distinct`                              | returns `false` when every target has a unique stem                                                  |
| `same_basename_false_for_single_suggestion`                                  | returns `false` with fewer than 2 suggestions                                                        |
| `compute_diagnostics_with_suggestions_sets_same_basename_flag`               | `data.same_basename: true` appears on a `broken-link` diagnostic whose top-`N` suggestions share a stem |
| `compute_diagnostics_with_suggestions_omits_same_basename_when_absent`      | no `same_basename` key when suggestions' stems are all distinct                                       |
| `compute_diagnostics_with_suggestions_never_sets_same_basename_for_anchors` | `broken-anchor` diagnostics never carry `same_basename`, even with duplicate heading text              |

> **Manual checkpoint:** Same scratch vault as Step 1's checkpoint. Run
> `knap lint . --json --suggest` again; confirm the `broken-link`
> diagnostic's `data` now includes `"same_basename": true` (all three
> candidates are named `storage.md`).

---

## Step 3 — CLI output for `same_basename`

Surfaces Step 2's flag in human-readable `knap lint --suggest` output, not
just `--json`. Small and mechanical — no TDD cycle needed, but still gets a
direct test since it's user-facing output.

**Deliverables:**

- `print_suggestions` in `src/cli/lint.rs`: after the existing
  `text_mismatch` note, print `    (top suggestions share a filename — path
  ranking alone can't tell them apart; verify by directory or content)` when
  `data.same_basename` is `true`.

**Unit tests:**

| Test                                                        | What it verifies                                                                 |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `print_suggestions_notes_same_basename` (or CLI snapshot test, matching whatever pattern the existing `text_mismatch` CLI test uses) | the same-basename note line appears in non-JSON `knap lint --suggest` output when `data.same_basename` is set |

> **Manual checkpoint:** Same scratch vault. Run `knap lint . --suggest`
> (no `--json`); confirm the note line appears under the `broken-link`
> diagnostic, formatted like the existing `text_mismatch` note.

---

## Step 4 — Update docs

Bring every doc that describes `--suggest`'s ranking behavior into agreement
with Steps 1–3.

**Deliverables:**

- `docs/design/components/handlers.md`: extend the "Text-aware ranking"
  section with `dir_distance()`, the generalized `combined_distance()`
  signature, and `same_basename()`; update `compute_diagnostics_with_suggestions()`'s
  entry to mention `same_basename` alongside `text_mismatch`.
- `skills/knap/reference/lint.md`: add the `same_basename` paragraph, the
  "when every suggestion looks wrong, don't settle — use `knap index --json`
  or `grep`" guidance (Trial 10 Opportunity 3), and a new JSON example
  showing `same_basename: true`.
- `docs/USER_STORIES.md`: add US-D28 and US-D29 under "Fixing", immediately
  after US-D20.
- `CHANGELOG.md`: new `v0.25` entry for the directory-proximity ranking term
  and the `same_basename` flag (alongside the `skill-restructure` entries
  already there).

**Unit tests:** none — docs only.

> **Manual checkpoint:** `grep -rn "same_basename\|dir_distance" src/
> skills/knap/reference/lint.md docs/design/components/handlers.md
> docs/USER_STORIES.md CHANGELOG.md` shows every doc in agreement with the
> code (no doc still describing only the two-term ranking).

---

## Step 5 — Full verification + Trial 11 re-run

Confirm the fix is total, then confirm it against the actual benchmark
failure it was built to close (Trial 10 Opportunity 4).

**Deliverables:**

- `cargo test` passes, `cargo clippy -- -D warnings` clean, full workspace.
- Release build (`cargo build --release`, matching how prior trials verified
  the `PATH`-resolved `knap` binary).
- `scripts/bench-setup-trial.sh --seed 1 --notes 200` re-run at N=3 per
  condition under the standing explicit-mention protocol (`f9399f3`) —
  identical seed/parameters to Trials 3–10, confirmed byte-identical setup
  the same way Trial 10 confirmed it.
- Result recorded as **Trial 11** in
  `docs/design/experiments/agentic-efficiency-benchmark.md`, following the
  "Status: superseded" cross-reference pattern already used for Trials
  8→9→10 — check specifically whether `projects/gateway.md`'s three-way
  `storage.md` collision now resolves to the correct same-directory target
  in all 3 knap-assisted runs, not just whether `knap lint`'s
  `problem_count` stays at zero (it already did, pre-fix).

| Test                     | What it verifies                                                              |
| --------------------------- | ---------------------------------------------------------------------------------- |
| Full `cargo test` suite  | no regression anywhere in the crate, not just the new tests above                |
| Trial 11, N=3 per condition | seeded-fix accuracy returns to 20/20 across all 3 knap-assisted runs, `gateway.md`'s `storage.md` link specifically resolves correctly in every run |

> **Manual checkpoint (full session):** Re-run the exact seed used since
> Trial 3. Confirm `knap lint` reports zero problems in all 6 vaults (as
> before), and additionally confirm — via the same
> `BENCH_MANIFEST.json`-based ground-truth check Trial 10 used — that no
> knap-assisted run mis-resolves `projects/gateway.md`'s "Storage" link or
> any other seeded defect. If any run still misses, treat it as a new
> finding (a different decoy shape, or an agent-judgment lapse like the
> `sync-835` case) rather than assuming this fix was insufficient.

---

## Done — v0.25 (directory-proximity-ranking) complete

| Story  | Feature                                                | Delivered in step |
| ------ | --------------------------------------------------------- | ------------------ |
| US-D28 | Directory-proximity signal in link ranking             | Step 1             |
| US-D29 | `same_basename` flag on `--suggest` diagnostics         | Steps 2–3          |
