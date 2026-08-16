# v0.17 Design — Drop `knap fix`

Removes the stories in the v0.17 release:

| Story            | Change                                                                                                                 |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------- |
| US-D14 (removed) | `knap fix [path] [--dry-run]` — deleted outright                                                                       |
| US-D17 (amended) | Drops the `--fix`-collapses-the-loop half of the story; `--suggest` itself is unchanged                                |
| US-D18 (amended) | `knap apply`'s op list drops `fix`; `rename-file`/`rename-heading`/`rename-tag`/`repoint-link`/`repoint-anchor` remain |
| US-D20 (amended) | Drops the "`knap fix`'s auto-apply ... decline to auto-apply" half; `--suggest`'s `text_mismatch` flag is unchanged    |

---

## Goal

An agent can no longer ask knap to blindly rewrite links, anchors, and stub
files across a whole vault in one unreviewed call. `knap fix`, `knap lint
--fix`, and `knap apply`'s `{"op":"fix"}` all shared one mechanism: pick the
single "unambiguous" candidate a ranking algorithm produced and write it to
disk with nobody — human or agent — looking at the specific edit first. That
mechanism guesses; guesses are sometimes wrong, and a bulk pass multiplies
one wrong guess across every broken link/anchor in the target root in a
single call. `skill/knap/SKILL.md` already steered agents away from `--fix`
for exactly this reason ("that reintroduces exactly the false-positive risk
`--fix` was dropped from this loop to avoid") — this release removes the
capability instead of just the advice not to use it.

What stays: every mechanism where a human or agent reviews a specific
candidate before it's written. The LSP's "Create note"/"Change anchor to..."
quick-fix code actions still fire once per cursor position, for a human to
accept or ignore. `knap lint --suggest` still surfaces the full ranked
candidate list (with `distance`, `text_distance`, and `text_mismatch`) for
an agent to read and judge. `knap apply`'s `repoint-link`/`repoint-anchor`
ops still exist — they take an exact `target`/`anchor` the caller already
chose, at an exact `range`; they don't derive one. Nothing about the ranking
algorithm itself (`rank_link_candidates`, `rank_anchor_candidates`,
`text_mismatch`) changes — only the two functions that turned a ranking into
an unreviewed auto-apply decision (`suggest_link_fix`, `suggest_anchor_fix`,
and the `unambiguous_winner` helper both of them were the only callers of)
go away.

---

## Removed

**CLI surface:**

- `knap fix [path] [--dry-run]` subcommand — `src/cli/fix.rs` deleted
  entirely (`PlannedFix`, `run`, `targets_for`, `plan_fixes`, `apply`,
  `merge_fixes`, `absolute`, and their unit tests)
- `knap lint --fix` flag — `src/cli/mod.rs`'s `Commands::Lint::fix` field,
  `src/cli/lint.rs`'s `fix` parameter, its `if fix { .. }` block, and the
  `fixes_applied` field on `LintReport`
- `knap apply`'s `{"op":"fix"}` operation — `src/cli/apply.rs`'s
  `ChangeOp::Fix` variant, `default_fix_path`, and its `apply_one` match arm

**Domain logic, now orphaned (each had exactly one caller, all removed
above):**

- `handlers::suggest_anchor_fix` — picked the one heading `knap fix` would
  repoint an anchor to
- `handlers::suggest_link_fix` — picked the one note `knap fix` would
  repoint a link to
- `handlers::unambiguous_winner` — the only caller of both was the two
  functions above; `compute_diagnostics_with_suggestions` (kept, backs
  `lint --suggest`) computes `text_mismatch` itself and never called
  `unambiguous_winner`

**Not removed — still used by the ranking `lint --suggest` exposes:**
`RankedCandidate`, `combined_distance`, `normalized_distance`,
`edit_distance`, `text_mismatch`, `rank_anchor_candidates`,
`rank_link_candidates`, `compute_diagnostics_with_suggestions`.

**Not removed — still used by the LSP quick-fix code actions and by
`knap apply`'s `repoint-link`/`repoint-anchor` ops, which pass an explicit,
already-chosen target/anchor rather than deriving one:**
`compute_create_missing_file_fix`, `compute_anchor_fix`, `compute_link_fix`.

---

## Testing

### Removed (tested the deleted surface)

| File               | Test                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/cli/fix.rs`   | `targets_for_file_path_returns_single_target`, `targets_for_directory_path_returns_all_notes`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `src/cli/apply.rs` | `change_op_deserializes_fix_default_path`, `apply_one_fix_reports_no_safe_fixes_found_when_clean`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `src/handlers.rs`  | `suggest_anchor_fix_picks_unique_closest`, `suggest_anchor_fix_none_on_tied_distance`, `suggest_anchor_fix_none_when_no_headings`, `suggest_anchor_fix_declines_when_text_mismatch`, `suggest_link_fix_picks_unique_closest`, `suggest_link_fix_none_on_tied_distance`, `suggest_link_fix_excludes_the_linking_note_itself`, `suggest_link_fix_declines_the_trial_4_sync_835_case`, `suggest_link_fix_still_repoints_when_signals_agree`, `unambiguous_winner_none_on_tied_combined_score`, `unambiguous_winner_none_when_text_mismatch_even_with_strict_winner`, `unambiguous_winner_some_when_signals_agree` |
| `tests/cli.rs`     | `lint_fix_applies_unambiguous_fixes_and_reports_post_fix_state`, `lint_fix_leaves_ambiguous_diagnostics_with_suggestions`, `fix_declines_repoint_when_text_mismatch_leaves_stub_fallback`, `lint_fix_reports_stub_fallback_not_wrong_repoint_for_mismatch_case`, `lint_without_fix_does_not_touch_disk`, `fix_creates_missing_file`, `fix_repoints_unambiguous_broken_link`, `fix_creates_stub_when_broken_link_is_ambiguous`, `fix_replaces_unambiguous_broken_anchor`, `fix_skips_ambiguous_anchor`, `fix_dry_run_makes_no_changes`, `apply_mixed_batch_rename_tag_and_fix`                                  |

Fixture directories `tests/fixtures/fix_broken_link`,
`tests/fixtures/fix_ambiguous_broken_link`, `tests/fixtures/fix_ambiguous_anchor`
become unused once the tests above are gone and are deleted with them.
`fix_repoint_broken_link`, `fix_unambiguous_anchor`, `fix_text_mismatch_link`
stay — each still backs a kept `--suggest`/`repoint-link`/`repoint-anchor`
test.

### Added (regression coverage for the removal itself)

| File               | Test                                           | What it verifies                                                    |
| ------------------ | ---------------------------------------------- | ------------------------------------------------------------------- |
| `tests/cli.rs`     | `fix_subcommand_no_longer_exists`              | `knap fix .` fails at the clap layer (unrecognized subcommand)      |
| `tests/cli.rs`     | `lint_fix_flag_no_longer_exists`               | `knap lint . --fix` fails at the clap layer (unexpected argument)   |
| `src/cli/apply.rs` | `change_op_fix_variant_no_longer_deserializes` | `serde_json::from_str::<ChangeOp>(r#"{"op":"fix"}"#)` returns `Err` |

---

## Docs

No parser, index, config, or protocol changes — this release only removes a
CLI/handler surface and the docs that describe it. Living docs (not
`docs/design/releases/archive/`, which stays as a historical record of what
past releases shipped) get updated in Step 2 of the plan: `README.md`,
`docs/ARCHITECTURE.md`, `docs/USER_STORIES.md`, `skill/knap/SKILL.md`,
`docs/design/components/handlers.md`. `CHANGELOG.md`/`docs/ROADMAP.md` get
their v0.17 entries at release time via `/knap-release`, same as every prior
release.
