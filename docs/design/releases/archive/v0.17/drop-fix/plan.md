# v0.17 Implementation Plan — Drop `knap fix`

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the CLI should be manually verified.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                        | Status | Notes |
| ------------------------------------------- | ------ | ----- |
| 1 — Remove the fix mechanism (code + tests) | Done   |       |
| 2 — Update living docs                      | Done   |       |

---

## Step 1 — Remove the fix mechanism (code + tests)

This is one atomic change: the CLI surface, the domain logic it alone called,
and every test that exercised either. Splitting it across steps would leave
an intermediate commit either failing `cargo clippy -D warnings` (orphaned
`suggest_link_fix`/`suggest_anchor_fix`/`unambiguous_winner` flagged as dead
code) or referencing a deleted module — neither is a state worth commit ting
on its own.

Follows the removal-flavored TDD cycle: write the regression tests that
prove the surface is gone **first**, confirm they fail (because the surface
still exists), then delete the surface until they pass.

**Deliverables:**

- Add regression tests (below) to `tests/cli.rs` and `src/cli/apply.rs`.
- Run `cargo test fix_subcommand_no_longer_exists lint_fix_flag_no_longer_exists change_op_fix_variant_no_longer_deserializes` and confirm all three **fail**.
- Delete `src/cli/fix.rs`.
- Edit `src/cli/mod.rs`:
  - Remove `mod fix;`.
  - Remove the `fix: bool` field (and its doc comment) from `Commands::Lint`.
  - Remove the `Commands::Fix { path: PathBuf, dry_run: bool }` variant.
  - Drop `/fix` from the `Apply` variant's doc comment
    (`rename-file`/`rename-heading`/`rename-tag`/`fix` → `rename-file`/
    `rename-heading`/`rename-tag`).
  - In `run()`: drop `fix` from the `Commands::Lint { .. }` destructure and
    from the `lint::run(...)` call's argument list; delete the
    `Commands::Fix { path, dry_run } => fix::run(&path, dry_run)` arm.
- Edit `src/cli/lint.rs`:
  - Remove the `fixes_applied: Option<Vec<String>>` field (and its doc
    comment) from `LintReport`.
  - Remove the `fix: bool` parameter from `run()`'s signature.
  - Remove the `let mut fixes_applied = None; if fix { .. }` block.
  - Remove `fixes_applied` from the `LintReport { .. }` construction.
  - Remove the `if let Some(descriptions) = &fixes_applied { .. }` text-mode
    printing block.
  - Delete the now-unreferenced `absolute()` helper (its only caller was
    inside the deleted `if fix` block).
  - Trim `run`'s doc comment: drop the sentences about `--fix`/`fixes_applied`/
    `cli::fix`, keep the ones about `compute_diagnostics_with_suggestions`.
- Edit `src/cli/apply.rs`:
  - Remove `use crate::cli::fix;` (keep `use crate::cli::rename;`, i.e.
    change `use crate::cli::{fix, rename};` to `use crate::cli::rename;`).
  - Remove the `Fix { path: PathBuf }` variant from `ChangeOp`.
  - Remove `default_fix_path()` and its doc comment.
  - Remove the `ChangeOp::Fix { .. } => "fix",` arm from `ChangeOp::kind()`.
  - Remove the `ChangeOp::Fix { path } => { .. }` arm from `apply_one`.
  - Update the `ChangeOp` doc comment (drop `/fix` from the op list).
  - Reword the two comments that reference `fix`'s stub creation as their
    motivating example (near `diff_and_sync` and its regression test) to not
    name a command that no longer exists — e.g. "a brand-new _empty_ file"
    without the `fix`-specific parenthetical.
- Edit `src/handlers.rs`:
  - Remove `suggest_anchor_fix` (with its doc comment).
  - Remove `suggest_link_fix` (with its doc comment).
  - Remove `unambiguous_winner` (with its doc comment) — dead once both
    callers above are gone.
  - Reword `rank_anchor_candidates`'s doc comment: drop "Shared by
    `suggest_anchor_fix` (which only wants the unambiguous winner) and" →
    "Used by `knap lint --suggest`, which wants the whole ranked list to
    show the agent."
  - Reword `rank_link_candidates`'s doc comment: drop "`knap fix` uses the
    unambiguous winner, `knap lint --suggest` shows the whole ranked list" →
    "used by `knap lint --suggest` to show the whole ranked list."
- Delete the now-orphaned test fixtures: `tests/fixtures/fix_broken_link/`,
  `tests/fixtures/fix_ambiguous_broken_link/`, `tests/fixtures/fix_ambiguous_anchor/`.
- Remove these tests from `tests/cli.rs`: `lint_fix_applies_unambiguous_fixes_and_reports_post_fix_state`,
  `lint_fix_leaves_ambiguous_diagnostics_with_suggestions`,
  `fix_declines_repoint_when_text_mismatch_leaves_stub_fallback`,
  `lint_fix_reports_stub_fallback_not_wrong_repoint_for_mismatch_case`,
  `lint_without_fix_does_not_touch_disk`, `fix_creates_missing_file`,
  `fix_repoints_unambiguous_broken_link`,
  `fix_creates_stub_when_broken_link_is_ambiguous`,
  `fix_replaces_unambiguous_broken_anchor`, `fix_skips_ambiguous_anchor`,
  `fix_dry_run_makes_no_changes`, `apply_mixed_batch_rename_tag_and_fix`.
- Remove these tests from `src/cli/apply.rs`: `change_op_deserializes_fix_default_path`,
  `apply_one_fix_reports_no_safe_fixes_found_when_clean`.
- Remove these tests from `src/handlers.rs`: `suggest_anchor_fix_picks_unique_closest`,
  `suggest_anchor_fix_none_on_tied_distance`, `suggest_anchor_fix_none_when_no_headings`,
  `suggest_anchor_fix_declines_when_text_mismatch`, `suggest_link_fix_picks_unique_closest`,
  `suggest_link_fix_none_on_tied_distance`, `suggest_link_fix_excludes_the_linking_note_itself`,
  `suggest_link_fix_declines_the_trial_4_sync_835_case`,
  `suggest_link_fix_still_repoints_when_signals_agree`,
  `unambiguous_winner_none_on_tied_combined_score`,
  `unambiguous_winner_none_when_text_mismatch_even_with_strict_winner`,
  `unambiguous_winner_some_when_signals_agree` (keep `ranked_candidate`,
  the `text_mismatch_*` tests, and everything under
  `// ── compute_diagnostics_with_suggestions ──`).

**Regression tests (write first):**

```rust
// tests/cli.rs, near the other "── errors ──"-style CLI-surface tests

#[test]
fn fix_subcommand_no_longer_exists() {
    let output = knap()
        .args(["fix", "."])
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unrecognized subcommand") && stderr.contains("fix"),
        "stderr was: {stderr}"
    );
}

#[test]
fn lint_fix_flag_no_longer_exists() {
    let output = knap()
        .args(["lint", ".", "--fix"])
        .output()
        .expect("failed to run knap");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected argument") && stderr.contains("--fix"),
        "stderr was: {stderr}"
    );
}
```

```rust
// src/cli/apply.rs, in `mod tests`, near the other `change_op_deserializes_*` tests

#[test]
fn change_op_fix_variant_no_longer_deserializes() {
    let result = serde_json::from_str::<ChangeOp>(r#"{"op":"fix"}"#);
    assert!(
        result.is_err(),
        "\"fix\" should no longer be a valid apply op, got: {result:?}"
    );
}
```

**Unit/integration tests:**

| Test                                           | What it verifies                                                  |
| ---------------------------------------------- | ----------------------------------------------------------------- |
| `fix_subcommand_no_longer_exists`              | `knap fix .` is rejected by clap as an unrecognized subcommand    |
| `lint_fix_flag_no_longer_exists`               | `knap lint . --fix` is rejected by clap as an unexpected argument |
| `change_op_fix_variant_no_longer_deserializes` | `{"op":"fix"}` no longer deserializes into `ChangeOp`             |

Plus every pre-existing test in `tests/cli.rs`, `src/cli/apply.rs`, and
`src/handlers.rs` not listed for removal above, unchanged and still passing —
in particular `lint_suggest_attaches_ranked_candidates_to_broken_link`,
`lint_suggest_reports_text_mismatch_for_decoy_and_correct_candidate`,
`apply_batch_repoints_broken_link_at_diagnostic_range`,
`apply_batch_repoints_broken_anchor_at_diagnostic_range`, and the
`code_actions_*`/`text_mismatch_*`/`diagnostics_with_suggestions_*` tests in
`src/handlers.rs`, proving the kept mechanisms (`--suggest`, `repoint-link`,
`repoint-anchor`, the LSP quick-fix code actions) are untouched by this
removal.

> **Manual checkpoint:** In a scratch vault, run `knap fix .` — expect clap's
> usage error listing `lsp`, `lint`, `index`, `parse`, `rename-file`,
> `rename-heading`, `rename-tag`, `check`, `version`, `apply` as the valid
> subcommands, with no `fix` among them. Run `knap lint . --fix` — expect a
> clap "unexpected argument '--fix'" error. Run `knap lint . --suggest` on a
> vault with a broken link — expect the ranked `suggestions` in `--json`
> output, unchanged from before this release.

Run `cargo test` (full suite green) and `cargo clippy --all-targets -- -D
warnings` (no dead-code warnings on the now-removed functions) before moving
on.

Commit:

```bash
git add -A
git commit -m "v0.17 drop-fix Step 1: remove knap fix, lint --fix, apply's fix op"
```

---

## Step 2 — Update living docs

Update every current-state doc that describes the removed surface. Archived
release docs under `docs/design/releases/archive/` are historical records of
what those releases shipped and are left untouched.

**Deliverables:**

- `README.md`:
  - In the two-faces bullet list, change `` `knap lint` / `knap index` /
`knap fix` / `knap rename-*` / `knap apply` `` to `` `knap lint` /
`knap index` / `knap rename-*` / `knap apply` ``, and drop "fix or" from
    "verify its own edits, fix or rename with the same guarantees".
  - Delete the entire `## Fixing (`knap fix`)` section.
  - In `## Linter (`knap lint`)`: drop `[--fix]` from the `Usage:` line;
    delete the ``- `--fix` —`` bullet; reword the `--suggest` bullet's
    "This is the same ranking `knap fix` uses to decide whether a fix is
    unambiguous — `--suggest` just shows the whole ranked list instead of
    collapsing it to one answer, so an agent that's already running `knap
lint --json` to verify an edit gets the candidates for the ambiguous
    cases `fix` declined to touch, in the same call, instead of needing a
    separate `knap fix --dry-run` round-trip." to: "`--suggest` shows the
    whole ranked list rather than collapsing it to one answer, so an agent
    already running `knap lint --json` to verify an edit gets every
    candidate for the ambiguous cases in the same call."
  - In `## Batch apply (`knap apply`)`: drop `` /`fix` `` from the op list
    (`` `rename-file`/`rename-heading`/`rename-tag`/`fix`/`repoint-link`/
`repoint-anchor` `` → `` `rename-file`/`rename-heading`/`rename-tag`/
`repoint-link`/`repoint-anchor` ``); remove `{"op":"fix"}` from the
    example batch and `applied fix: applied 2 fix(es) in 2 file(s)` from its
    output (adjust the surrounding counts to match a `rename-tag`-only
    example); drop the ``- `rename-file`/`rename-heading`/`rename-tag`/
`fix` — same field names`` bullet's `` /`fix` ``; reword "rather than
    letting `fix` re-derive one" to "rather than deriving one automatically".
- `docs/ARCHITECTURE.md`:
  - Drop `fix.rs` from the `src/cli/` module list.
  - In the subcommand table: drop `[--fix]` and the "`--fix` added v0.13"
    clause from the `lint` row; delete the `fix` row entirely.
  - Delete the paragraph describing `fix`'s config resolution,
    `cli::fix::plan_fixes`/`apply`, and `lint --fix`'s post-fix rebuild
    (the passage from "`fix` resolves config the same way `lint` does" through
    "No editor is needed for any of them.").
  - Trim the `lint --suggest`/`lint --fix` paragraph immediately after: keep
    the `--suggest` sentences, drop the `lint --fix` sentences ("`lint --fix`
    runs `cli::fix::plan_fixes`/`apply` over the whole target root ...
    `fixes_applied` field listing what was applied.").
  - In the `apply` paragraph: drop `` /`fix` `` from the op list, drop
    `` `targets_for`+`plan_fixes`/`apply`/ `` from the dispatch-target list,
    and reword "rather than re-deriving a target the way `fix` does" to
    "rather than re-deriving a target".
  - In the Edit Applicator section's "Used only by" sentence, drop
    "; `fix` — v0.13".
- `docs/USER_STORIES.md`:
  - Add a scope note at the top (matching the existing wiki-link one):
    `> **Scope note (v0.17):** knap dropped `knap fix`, `knap lint --fix`,
and `knap apply`'s `fix`op — a blind bulk auto-apply across a whole
vault with no per-edit review. Story US-D14 was removed as a result;
US-D17, US-D18, and US-D20 were trimmed to drop references to it.`knap lint --suggest`and`knap apply`'s `repoint-link`/`repoint-anchor`ops (pick a candidate, then apply it) remain the recommended flow. See`docs/design/releases/v0.17/drop-fix/design.md`.`
  - Delete story US-D14 in full.
  - In US-D17, drop the sentence starting "I can also pass `--fix` to have
    `knap lint` apply every safe fix first ... `fixes_applied` in `--json`
    output lists what was applied." and the sentence "`--fix` is the ..."
    that follows it, keeping the rest of the story about `--suggest`.
  - In US-D18, drop `` , `fix` `` from the op list and change "as any
    `rename-*`/`fix` operations" to "as any `rename-*` operations".
  - In US-D20, drop "`knap fix`'s auto-apply and" so the story opens with
    "As an agent, `knap lint --suggest`'s ranked candidates ..."; drop the
    clause "and `knap fix`/`knap lint --fix` decline to auto-apply when the
    two signals disagree", keeping the rest of the sentence about
    `text_mismatch`.
- `skill/knap/SKILL.md`:
  - Frontmatter `description`: drop `fix,` from `(lint, fix, index,
rename-*, apply)` → `(lint, index, rename-*, apply)`.
  - Drop the ``(read-only — no `--fix`)`` parenthetical after `` `knap
lint --suggest --json` `` → just `` `knap lint --suggest --json`
(read-only). ``.
  - Reword "that reintroduces exactly the false-positive risk `--fix` was
    dropped from this loop to avoid, minus even `--fix`'s tie-safety
    (`--fix` declines to auto-apply when the top two candidates are within a
    tie; a script that always takes `suggestions[0]` doesn't)." to: "that
    reintroduces exactly the false-positive risk a blind bulk auto-apply
    carries, with none of the unambiguous-only discipline the ranking is
    built for — a script that always takes `suggestions[0]` has no
    tie-safety at all."
  - Reword "This is the same ranking `knap fix` uses to decide, not just the
    leftovers." to "This is the ranking `--suggest` exposes in full, not
    just a filtered leftover list."; reword "Two candidates this close
    together by combined score means `knap fix` would leave this one alone;
    if `suggestions[0]` were strictly closer than `suggestions[1]`, `fix`
    would already have applied it." to "Two candidates this close together
    by combined score count as a tie — treat it as ambiguous and don't
    repoint on ranking alone."; reword "A diagnostic with no `data` field at
    all had zero candidates in the workspace — `knap fix`'s create-a-stub
    case." to "A diagnostic with no `data` field at all had zero candidates
    in the workspace."
  - In "## Full reference", drop `` /`knap fix` `` from "the edit-verify loop
    `knap lint`/`knap fix`/`knap rename-*`/`knap apply` were built for" →
    "`knap lint`/`knap rename-*`/`knap apply`".
- `docs/design/components/handlers.md`:
  - `### compute_create_missing_file_fix(), compute_anchor_fix() (v0.13)`:
    reword "so the headless `knap fix` CLI subcommand computes the exact
    same `WorkspaceEdit` these code actions do, without a cursor or a live
    LSP session." to "extracted for reuse across every caller that computes
    the same edit `handle_code_actions` does." Drop the `compute_anchor_fix`
    bullet's "`knap fix` calls it once, for the single heading
    `suggest_anchor_fix` (below) picks." clause — reword to note it's also
    the execution side of `knap apply`'s `repoint-anchor` op.
  - Delete the entire `### suggest_anchor_fix(), edit_distance() (v0.13,
text-aware since v0.16)` section's `suggest_anchor_fix` content; keep
    `edit_distance` documented (move its description into the "Text-aware
    ranking" section below it, since `edit_distance` is still used there and
    by `rank_anchor_candidates`/`rank_link_candidates`).
  - In `### Text-aware ranking`: drop `unambiguous_winner` from the code
    block and its bullet — it has no callers left; keep
    `RankedCandidate`/`combined_distance`/`normalized_distance`/
    `text_mismatch`. Reword "both `suggest_link_fix`/`suggest_anchor_fix`
    decline to pick a winner when the two signals disagree" to
    "`compute_diagnostics_with_suggestions` reports the disagreement via
    `text_mismatch` rather than picking a winner".
  - Retitle `### compute_link_fix(), suggest_link_fix() (v0.13, text-aware
since v0.16)` to `### compute_link_fix() (v0.13, text-aware since
v0.16)`, drop the `suggest_link_fix` signature and bullet, and reword
    "Used by `cli::fix::plan_fixes` (shared by `knap fix` and `knap lint
--fix`): tried first for a broken link, falling back to
    `compute_create_missing_file_fix` only when no candidate is unambiguous."
    to "The execution side of `knap apply`'s `repoint-link` op
    (`src/cli/apply.rs`) — the caller supplies the already-chosen
    `new_target`."

**Unit tests:** none — doc-only step.

> **Manual checkpoint:** `grep -rn "knap fix\|lint --fix\|op.:.fix" README.md
docs/ARCHITECTURE.md docs/USER_STORIES.md skill/knap/SKILL.md
docs/design/components/handlers.md` returns nothing. `cargo doc --no-deps`
> still builds clean (doc comments referencing deleted items would fail
> intra-doc links if any were missed).

Run `cargo test` and `cargo clippy --all-targets -- -D warnings` once more
(doc-only changes shouldn't affect either, but confirms nothing was
accidentally touched in code) before committing.

Commit:

```bash
git add -A
git commit -m "v0.17 drop-fix Step 2: update README/ARCHITECTURE/USER_STORIES/SKILL/handlers docs"
```

---

## Done — v0.17 complete

| Story            | Delivered in step |
| ---------------- | ----------------- |
| US-D14 (removed) | Step 1            |
| US-D17 (amended) | Steps 1–2         |
| US-D18 (amended) | Steps 1–2         |
| US-D20 (amended) | Steps 1–2         |

At release time, `/knap-release` adds the v0.17.0 `CHANGELOG.md` entry (under
`### Removed`) and `docs/ROADMAP.md` milestone referencing this design doc,
bumps `Cargo.toml`/`README.md`'s version badge, and archives this folder to
`docs/design/releases/archive/v0.17/drop-fix/`.
