# v0.13 Implementation Plan — Agent Ergonomics

Describes the order in which changes are made, what is tested after each
step, and the checkpoints where the CLI should be manually verified.

The guiding principle: each step produces something testable. No step lays
down untested code for the next step to build on.

---

## Status

| Step                                                          | Status | Notes |
| ---------------------------------------------------------------- | ------ | ----- |
| 1 — Stable diagnostic `code`s                                    | Done   |       |
| 2 — `NoteIndex::note_report`                                     | Done   |       |
| 3 — Extract `compute_create_missing_file_fix` + `compute_anchor_fix` | Done   |       |
| 4 — `suggest_anchor_fix` + `edit_distance`                       | Done   |       |
| 5 — `knap lint --fail-on`                                        | Done   |       |
| 6 — `knap lint --since`                                          | Done   |       |
| 7 — `knap index <file>` scopes to one note                       | Done   |       |
| 8 — `knap fix`                                                   | Done   |       |
| 9 — `SKILL.md` + integration tests + docs                        | Done   |       |

---

## Step 1 — Stable diagnostic `code`s

Comes first because it touches only `compute_diagnostics`, has no
dependency on anything else in this release, and every other JSON-output
change in this release (`blocking_count` in Step 5) sits next to it in the
same file.

**Deliverables:**

- `src/handlers.rs`: six `CODE_*` constants next to `DIAG_SOURCE`; add
  `NumberOrString` to the `use lsp_types::{...}` list; add `code: Some(...)`
  to all six `Diagnostic { .. }` literals in `compute_diagnostics`, per the
  mapping table in the design doc.

Write the unit tests first against the current (code-less) diagnostics —
confirm they fail — then add the `code` fields until green.

**Unit tests:**

| Test                                                                   | What it verifies                                          |
| -------------------------------------------------------------------------- | ---------------------------------------------------------- |
| `diagnostic_broken_link_has_broken_link_code`                             | `code == "broken-link"`                                    |
| `diagnostic_broken_anchor_has_broken_anchor_code`                         | `code == "broken-anchor"`                                  |
| `diagnostic_missing_frontmatter_has_missing_frontmatter_code`             | No frontmatter block at all → `code == "missing-frontmatter"` |
| `diagnostic_missing_required_field_has_missing_required_field_code`      | Frontmatter exists, one required key absent → `code == "missing-required-field"` |
| `diagnostic_invalid_value_has_invalid_field_value_code`                  | `code == "invalid-field-value"`                             |
| `diagnostic_unknown_key_has_unknown_field_code`                          | `code == "unknown-field"`                                   |

Existing `compute_diagnostics`/`schema_diag_*` tests must stay green
unmodified — this step only adds a field, it never changes which
diagnostics fire, their range, or their message.

> **Manual checkpoint:** In an editor connected to `knap lsp`, open a note
> with a broken link; the Problems panel entry (VS Code) now shows a code
> (`knap(broken-link)` or similar, depending on the editor's rendering)
> alongside the existing message — same diagnostic as before this step,
> just with a code attached.

---

## Step 2 — `NoteIndex::note_report`

Pulls the per-note summary construction out of `report()` into its own
method, so Step 7's `knap index <file>` can call it directly instead of
building (and discarding) every other note's summary too. Pure refactor —
no new behavior for `report()`.

**Deliverables:**

- `src/index/mod.rs`: private `fn note_summary(&self, note: &Note) ->
NoteSummary` holding today's per-note closure body; public `fn
note_report(&self, path: &Path) -> Option<NoteSummary>` calling
  `self.get_note(path).map(|n| self.note_summary(n))`; `report()` rewritten
  to call `note_summary` per note instead of inlining the closure.
- `NoteSummary`, `HeadingSummary`, `LinkSummary` gain
  `#[cfg_attr(test, derive(PartialEq, Debug))]`.

Write the new unit tests first against the extracted method; confirm they
fail to compile until `note_report`/`note_summary` exist.

**Unit tests:**

| Test                                    | What it verifies                                                              |
| ------------------------------------------ | -------------------------------------------------------------------------------- |
| `note_report_matches_report_entry`         | `idx.note_report(path)` equals the corresponding entry in `idx.report().notes`    |
| `note_report_none_for_unindexed_path`      | An unindexed path → `None`, no panic                                             |

Existing `report`-related tests (if any) must stay green unmodified.

> **Manual checkpoint:** none — no CLI surface yet. Verified purely by
> `cargo test`.

---

## Step 3 — Extract `compute_create_missing_file_fix` + `compute_anchor_fix`

Same extraction pattern v0.12 used for heading/tag rename: pull the two
"safe" code-action bodies out of the cursor/selection-driven
`handle_code_actions` into position-independent functions that both the
existing LSP handler and Step 8's `knap fix` call. Refactor with a
regression net — `handle_code_actions`'s behavior must not change.

**Deliverables:**

- `src/handlers.rs`: add `compute_create_missing_file_fix(link, source,
config) -> WorkspaceEdit` (today's `ResolvedLink::Broken` arm body) and
  `compute_anchor_fix(source, anchor_range, new_anchor) -> WorkspaceEdit`
  (today's per-heading anchor-replace body).
- `handle_code_actions` calls both instead of inlining their bodies; still
  offers every heading in the `Found` arm's loop (only `knap fix` needs a
  single answer — that's Step 4).

Write the new unit tests first against the extracted functions.

**Unit tests:**

| Test                                                    | What it verifies                                                                             |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------ |
| `compute_create_missing_file_fix_matches_prior_action_shape` | Output's `document_changes` matches what `handle_code_actions` built inline before this step (regression net) |
| `compute_anchor_fix_replaces_anchor_range`                    | Single `TextEdit` at `anchor_range`, `new_text == new_anchor`                                    |

Existing `handle_code_actions` tests (`"Create note"`, `"Change anchor to..."`)
must stay green unmodified — proves the extraction didn't change LSP
behavior.

> **Manual checkpoint:** In an editor connected to `knap lsp`, trigger
> "Create note" on a broken link and "Change anchor to..." on a broken
> anchor exactly as before this step; both produce identical results (this
> step is a pure refactor of the LSP path).

---

## Step 4 — `suggest_anchor_fix` + `edit_distance`

The one genuinely new piece of domain logic in this release: picking a
single best-guess heading for a broken anchor. Comes after Step 3 because
`knap fix` (Step 8) needs both this and `compute_anchor_fix` together, but
this function has no dependency on the extraction itself — ordered here so
Step 8 can wire up all its inputs in one pass.

**Deliverables:**

- `src/handlers.rs`: private `fn edit_distance(a: &str, b: &str) -> usize`
  (Levenshtein, byte-wise) and `pub(crate) fn suggest_anchor_fix<'a>(broken_slug:
&str, target_note: &'a parser::Note) -> Option<&'a parser::Heading>`, per
  the design doc.

Write the unit tests first — stub both functions (`edit_distance` to
`unimplemented!()`, `suggest_anchor_fix` to `None`) so the file compiles,
confirm the tests fail, then implement.

**Unit tests:**

| Test                                              | What it verifies                                                      |
| ------------------------------------------------------ | -------------------------------------------------------------------------- |
| `edit_distance_identical_strings_is_zero`              | `edit_distance("x", "x") == 0`                                             |
| `edit_distance_counts_substitutions`                    | `edit_distance("abc", "abd") == 1`                                        |
| `suggest_anchor_fix_picks_unique_closest`              | Headings at distinct distances → the closest one is returned              |
| `suggest_anchor_fix_none_on_tied_distance`             | Two headings equally close → `None`                                       |
| `suggest_anchor_fix_none_when_no_headings`             | Target note has zero headings → `None`                                    |

> **Manual checkpoint:** none — pure function, no CLI surface yet. Verified
> purely by `cargo test`.

---

## Step 5 — `knap lint --fail-on <severity>`

First CLI-facing step; needs nothing from Steps 2–4, only Step 1's
diagnostics (which already exist regardless of `code`). Ordered before
`--since`/`index <file>`/`fix` because it's the smallest CLI change — a pure
filter over data `lint` already computes — and de-risks the `LintReport`
shape change (`blocking_count`) before the other CLI steps build on the same
file.

**Deliverables:**

- `src/cli/mod.rs`: `FailOn` enum; `Lint` variant gains `fail_on: FailOn`
  (`default_value = "warning"`).
- `src/cli/lint.rs`: `severity_rank`, `FailOn::rank`, `LintReport.blocking_count`;
  `run`'s bail condition switches to `blocking_count > 0`; text-mode summary
  line extended only when `blocking_count != problem_count`.

**Unit tests:**

| Test                                                          | What it verifies                                                                    |
| ------------------------------------------------------------------ | ---------------------------------------------------------------------------------------- |
| `severity_rank_orders_error_above_warning_above_info_above_hint`  | Rank values strictly increase across `ERROR, WARNING/None, INFORMATION, HINT`             |
| `fail_on_default_matches_todays_behavior`                          | `FailOn::Warning.rank()` admits every diagnostic `compute_diagnostics` emits today       |

**Integration tests:**

| Test                                                        | What it verifies                                                        |
| ------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| `lint_fail_on_error_passes_when_only_warnings_present`         | `--fail-on error` on `lint_basic` (WARNING-only) → exit 0                     |
| `lint_fail_on_warning_matches_default_behavior`                | `--fail-on warning` and the flag omitted both exit 1 on `lint_basic`, identical to today |

> **Manual checkpoint:** `knap lint tests/fixtures/lint_basic --fail-on
error` exits 0 (`echo $?`); `knap lint tests/fixtures/lint_basic` (no flag)
> still exits 1, unchanged from before this release.

---

## Step 6 — `knap lint --since <git-ref>`

**Deliverables:**

- `src/cli/mod.rs`: `Lint` variant gains `since: Option<String>`.
- `src/cli/lint.rs`: `changed_paths_since`, `git_output`, per the design
  doc; `run` retains only targets whose canonicalized path is in the
  changed set when `since` is `Some`.

No pure-logic unit tests this step — `changed_paths_since` shells out to a
real `git` process and reads the real filesystem, so it's exercised by the
integration tests below rather than mocked (mirrors the project's "don't
mock the NoteIndex" testing guideline: here the thing to avoid mocking is
`git` itself).

**Integration tests:**

| Test                                                | What it verifies                                                                                                                     |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `lint_since_scopes_to_git_changed_files`               | git-initialized fixture, two clean notes committed, then a broken link introduced in only one; `--since <that commit>` reports only the touched file |
| `lint_since_includes_untracked_new_files`              | A new, uncommitted note with a broken link is included by `--since <ref>`                                                                    |
| `lint_since_outside_git_repo_errors`                   | Non-zero exit, stderr mentions git, when `--since` is used outside a git worktree                                                             |

> **Manual checkpoint:** In a scratch vault under git, commit a clean note,
> break a link in it without committing, run `knap lint . --since HEAD`;
> confirm the broken link is reported. Revert the change, commit again, run
> the same command; confirm `0 problem(s) in 0 file(s)`.

---

## Step 7 — `knap index <file>` scopes to one note

Uses Step 2's `note_report`. No new flag — `Index`'s existing `path:
PathBuf` already accepts a file, but today that silently indexes and prints
the file's whole *parent directory*, not the file itself (an undocumented,
surprising quirk of `config::for_path`'s file handling). This step gives
file input real meaning, and fixes a correctness gap along the way: since
`NoteSummary` carries `backlinks`, a single-file query still needs the
*whole vault* indexed (unlike `lint`, which only needs the target file's own
outgoing links) — so this reuses the `cwd`-rooted fix
`rename-heading`/`rename-tag` already established in v0.12, rather than
indexing off `file.parent()`.

**Deliverables:**

- `src/cli/index.rs`: `run` resolves config from `cwd` (`config::for_path(Path::new("."), None)`)
  when `path.is_file()`, vs. today's `config::for_path(path, None)` when
  `path` is a directory (unchanged). File-path branch: canonicalized-path
  lookup against `idx.all_notes()` (bail loudly if not found); `--json`
  serializes `idx.note_report(&note.path)` alone; text mode extracts the
  existing per-note print loop body into `fn print_note(note: &Note, idx:
&NoteIndex)`, called once for the file case or once per note for the
  existing directory listing.

No pure-logic unit tests this step (covered by Step 2's `note_report`
tests) — covered by the integration tests below.

**Integration tests:**

| Test                                                       | What it verifies                                                                                                          |
| --------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `index_file_path_prints_single_note_neighborhood`               | `knap index <file> --json` parses as one `NoteSummary` object, not an `IndexReport`                                        |
| `index_file_path_includes_backlinks_from_outside_its_directory` | A note in a subdirectory, linked from a note in a sibling directory; indexing the nested file (from the vault root) still reports the backlink |
| `index_unindexed_file_path_errors`                              | Non-zero exit for a file path the index doesn't have                                                                       |

> **Manual checkpoint:** `cd tests/fixtures/index_basic && knap index <a
note>.md --json`; confirm the output is a single JSON object with
> `headings`/`links`/`backlinks`/`tags` keys, not a `{ "notes": [...], "tags":
{...} }` envelope, and that `knap index .` (directory) still prints the
> full listing unchanged.

---

## Step 8 — `knap fix [path] [--dry-run]`

Uses Steps 3 and 4 together — the last step with new domain logic, so it
comes after everything it depends on.

**Deliverables:**

- `src/cli/mod.rs`: `Commands::Fix { path: PathBuf, dry_run: bool }`
  (`path` defaults to `.`).
- `src/cli/fix.rs`: `PlannedFix`; `run(path, dry_run)` — `config::for_path`
  + `index::build` (same target selection as `lint`), walk every target
  note's links building a `PlannedFix` per `compute_create_missing_file_fix`
  or (`suggest_anchor_fix` + `compute_anchor_fix`) result, skipping
  ambiguous anchors; `--dry-run` prints the plan; otherwise merges every
  fix's edit into one `document_changes` list (wrapping `changes`-shaped
  results the same way `rename-file` already does) and calls `edit::apply`.

No pure-logic unit tests this step (the fix-selection logic is Steps 3–4's
`compute_*`/`suggest_anchor_fix`, already tested; `PlannedFix` assembly and
disk writing are exercised end-to-end below) — covered by the integration
tests.

**Integration tests:**

| Test                                          | What it verifies                                                                          |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `fix_creates_missing_file`                          | Broken-link fixture → `knap fix` creates the stub file; a subsequent `knap lint` is clean         |
| `fix_replaces_unambiguous_broken_anchor`            | Broken anchor with one clearly-closest heading → rewritten; subsequent `knap lint` is clean       |
| `fix_skips_ambiguous_anchor`                        | Broken anchor with two equally-close headings → left alone; still flagged by `knap lint` after    |
| `fix_dry_run_makes_no_changes`                      | `--dry-run` prints the plan; fixture directory is byte-for-byte unchanged                          |

> **Manual checkpoint:** In a scratch vault with one broken link and one
> broken-but-unambiguous anchor, run `knap fix --dry-run` (confirm two
> planned-fix lines print, files unchanged), then `knap fix` (confirm the
> file was created and the anchor rewritten), then `knap lint` (confirm
> clean).

---

## Step 9 — `SKILL.md` + integration tests + docs

Always last.

**Deliverables:**

- `skill/knap/SKILL.md`, per the Documentation Changes section of the
  design doc — frontmatter plus the lint → fix/rename → lint loop, the six
  diagnostic codes, and `knap index <file>`.
- Any integration tests from Steps 5–8 not yet written land here if they
  were deferred; `cargo test` passes, `cargo clippy -- -D warnings` clean.
- `README.md`: document `--fail-on` and `--since` under "Linter"; document
  that `knap index <path>` now gives a single note's neighborhood when
  `<path>` is a file (full workspace snapshot unchanged for a directory)
  under "Indexer"; add a new "Fixing (`knap fix`)" section; mention
  `skill/knap/SKILL.md` and how to copy it into a vault's `.claude/skills/`.
- `docs/ARCHITECTURE.md`: CLI subcommand table gains a `fix` row (the
  ASCII diagram already says "fix (planned, v0.13)" — drop "(planned, v0.13)"
  now that it's shipped); `index`'s usage string becomes `knap index <path>
[--json]` with a note that a file path scopes to that note (matching
  `lint`'s existing file-or-directory convention); CLI section prose gets a
  sentence on `fix` reusing `compute_create_missing_file_fix`/
  `compute_anchor_fix` the same way `rename-*` reuses the rename `compute_*`
  functions, and on `index`'s file-input case resolving off `cwd` for the
  same reason `rename-heading`/`rename-tag` do.
- `docs/design/components/handlers.md`: document the new `compute_*`/
  `suggest_anchor_fix` functions and the diagnostic `code` field, matching
  the existing description style for `compute_diagnostics`.
- `docs/USER_STORIES.md`/`docs/ROADMAP.md`: already updated as part of
  scoping this release — confirm no drift crept in during implementation.

**Manual read-through checkpoint (SKILL.md):** Open `skill/knap/SKILL.md`
and run every command it references verbatim against a scratch vault;
confirm each one behaves exactly as documented (flags exist, output shape
matches, diagnostic codes match Step 1's constants).

> **Manual checkpoint (full session):** In a scratch vault with a real
> editor connected via `knap lsp`, confirm editor-side diagnostics now show
> codes and editor-side code actions ("Create note", "Change anchor to...")
> are unchanged from pre-release behavior. Separately, in the same vault
> headlessly: `knap lint --json` (inspect `code`/`blocking_count`), `knap
lint --since <ref>`, `knap index <file> --json` (single note), `knap fix
--dry-run` then `knap fix`, `knap lint` again to confirm clean. Confirm
> earlier releases (`rename-*`, plain `lint`/`index <dir>`) are unaffected.

---

## Done — v0.13 complete

| Story  | Feature                                              | Delivered in step |
| ------ | ------------------------------------------------------- | ------------------ |
| US-D11 | Stable `code` field on every diagnostic                  | Step 1              |
| US-D16 | `knap lint --fail-on <severity>`                         | Step 5              |
| US-D12 | `knap lint --since <git-ref>`                            | Step 6              |
| US-D13 | `knap index <file>` — one note's neighborhood            | Step 7              |
| US-D14 | `knap fix [path] [--dry-run]`                            | Step 8              |
| US-D15 | `skill/knap/SKILL.md`                                    | Step 9              |
