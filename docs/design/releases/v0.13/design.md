# v0.13 Design — Agent Ergonomics

Covers the stories in the v0.13 release:

| Story  | Feature                                                                         |
| ------ | -------------------------------------------------------------------------------- |
| US-D11 | Stable `code` field on every `compute_diagnostics` diagnostic                    |
| US-D16 | `knap lint --fail-on <severity>` — only fail on diagnostics at or above a threshold |
| US-D12 | `knap lint --since <git-ref>` — scope linting to files changed since a ref       |
| US-D13 | `knap index <file>` — one note's neighborhood, not the full index               |
| US-D14 | `knap fix [path] [--dry-run]` — headless quick-fix apply for safe code actions   |
| US-D15 | `skill/knap/SKILL.md` — shippable skill documenting the agent lint/fix/rename loop |

---

## Goal

v0.12 gave agents write access to knap's refactors (`rename-*`). This release
removes the friction that shows up once an agent is actually looping on
`lint`/`index`/`rename-*`/`fix` as its edit-verify cycle: diagnostics that can
only be triaged by matching message prose, a full-workspace lint on every
check, no way to see one note's neighborhood without paging through the whole
index snapshot, and no headless equivalent of the two safe code actions an
editor session already offers. None of these are new capabilities so much as
existing capabilities made cheap and machine-addressable — nothing in this
release adds a new LSP capability; every change is either CLI-surface or
internal to already-shared `handlers::` logic.

Two corrections to the roadmap's candidate list, found while scoping:

- The roadmap's diagnostic-code list omits the "value not in the allowed
  list" diagnostic (`compute_diagnostics`' schema branch). It gets a code
  too (`invalid-field-value`) — an agent branching on `code` needs every
  diagnostic covered, not five out of six.
- `--fail-on <severity>` (pulled in from the Backlog) is only useful once
  diagnostics can actually differ in severity from each other. Today every
  diagnostic `compute_diagnostics` emits is `DiagnosticSeverity::WARNING` —
  this release does **not** change that (reassigning severities is a
  separate, higher-risk decision affecting live editor diagnostics, not an
  "agent ergonomics" change). `--fail-on` ships as a mechanical threshold
  against whatever severities exist; today only `--fail-on warning` (the
  default, preserving current behavior exactly) and below have any effect,
  and `--fail-on error` always passes until some future diagnostic is
  promoted to `ERROR`. This is called out in `README.md` rather than solved
  by inventing a severity scheme this release doesn't otherwise need.

---

## Handler Changes

### `compute_diagnostics` — stable `code` per diagnostic

Every `Diagnostic` literal `compute_diagnostics` builds (`src/handlers.rs`)
gains a `code`, using six module-level constants next to the existing
`DIAG_SOURCE`:

```rust
const CODE_BROKEN_LINK: &str = "broken-link";
const CODE_BROKEN_ANCHOR: &str = "broken-anchor";
const CODE_MISSING_FRONTMATTER: &str = "missing-frontmatter";
const CODE_MISSING_REQUIRED_FIELD: &str = "missing-required-field";
const CODE_INVALID_FIELD_VALUE: &str = "invalid-field-value";
const CODE_UNKNOWN_FIELD: &str = "unknown-field";
```

Mapping (six call sites in `compute_diagnostics` today, all currently ending
`..Default::default()` with no `code`):

| Diagnostic                                                     | Code                        |
| ---------------------------------------------------------------- | ---------------------------- |
| `ResolvedLink::Broken` — link target not found                   | `CODE_BROKEN_LINK`           |
| Anchor doesn't match any heading in the resolved target           | `CODE_BROKEN_ANCHOR`         |
| Required field missing, note has **no** frontmatter block at all | `CODE_MISSING_FRONTMATTER`   |
| Required field missing, frontmatter block **exists**              | `CODE_MISSING_REQUIRED_FIELD` |
| Value not in an allowed list                                       | `CODE_INVALID_FIELD_VALUE`   |
| Frontmatter key not recognized by the schema                     | `CODE_UNKNOWN_FIELD`         |

Each becomes `code: Some(NumberOrString::String(CODE_XXX.to_string())), ..`
alongside the existing `range`/`severity`/`message`/`source` fields — no
other field changes, no behavior change to which diagnostics fire or their
text. `NumberOrString` is added to the existing `use lsp_types::{...}` import
list at the top of `src/handlers.rs`.

This flows through both existing consumers of `compute_diagnostics` for
free: `knap lint --json`'s `FileDiagnostics.diagnostics` (already
`Vec<lsp_types::Diagnostic>`, so `code` serializes with no `LintReport`
change) and real `textDocument/publishDiagnostics` notifications — an
editor's Problems panel gains the same stable code, which is a side effect
of sharing one function, not separate work.

### `compute_create_missing_file_fix` (new, extracted from `handle_code_actions`)

```rust
/// Build the "create missing file" fix for a single broken link: an
/// `Op(ResourceOp::Create)` for the new note, plus — when the link's raw
/// target needed `<...>` escaping — a `TextEdit` rewriting it in place.
/// Extracted from `handle_code_actions`'s `ResolvedLink::Broken` arm so
/// `knap fix` computes the exact same edit the interactive "Create note"
/// quick fix does, instead of a second implementation that can drift.
pub(crate) fn compute_create_missing_file_fix(
    link: &parser::MarkdownLink,
    source: &Path,
    config: &crate::config::Config,
) -> WorkspaceEdit
```

Body is today's `ResolvedLink::Broken` arm verbatim (`new_note_path`, the
`Op(ResourceOp::Create)`, the conditional escape `TextEdit`), just returning
the `WorkspaceEdit` instead of pushing a `CodeAction` inline.
`handle_code_actions`'s Broken arm shrinks to calling this and wrapping the
result in a `CodeAction { title: "Create note".to_string(), .. }` — no
behavior change for the LSP path.

### `compute_anchor_fix` (new, extracted from `handle_code_actions`)

```rust
/// Build the `WorkspaceEdit` that retargets a broken anchor link's
/// `anchor_range` to `new_anchor` (a GFM slug). Extracted from
/// `handle_code_actions`'s per-heading "Change anchor to..." arm so both the
/// interactive code action (one call per candidate heading) and `knap fix`
/// (one call, for the single best-guess heading) build the identical edit.
pub(crate) fn compute_anchor_fix(source: &Path, anchor_range: Range, new_anchor: &str) -> WorkspaceEdit
```

Body is today's single-`TextEdit`-in-a-`changes`-map construction, unchanged.
`handle_code_actions`'s loop over `target_note`'s headings is otherwise
unchanged — it still offers every heading as its own action, because an
interactive session should let the writer pick; only `knap fix` needs a
single unambiguous answer, which is what the next two functions compute.

### `suggest_anchor_fix` (new)

```rust
/// The single best-guess replacement heading for `broken_slug` in
/// `target_note`, or `None` when there's no safe unambiguous choice: the
/// target has no headings, or two or more headings tie for the closest GFM
/// slug (by edit distance). Used only by `knap fix` — the interactive code
/// action keeps listing every heading, since a human can pick.
pub(crate) fn suggest_anchor_fix<'a>(
    broken_slug: &str,
    target_note: &'a parser::Note,
) -> Option<&'a parser::Heading> {
    let mut best: Option<(usize, &parser::Heading)> = None;
    let mut tied = false;
    for heading in &target_note.headings {
        let dist = edit_distance(broken_slug, &slug(&heading.text));
        match &best {
            None => best = Some((dist, heading)),
            Some((best_dist, _)) if dist < *best_dist => {
                best = Some((dist, heading));
                tied = false;
            }
            Some((best_dist, _)) if dist == *best_dist => tied = true,
            _ => {}
        }
    }
    if tied { None } else { best.map(|(_, h)| h) }
}
```

No distance threshold — even a distant "closest" heading is still reported
(and, under `--dry-run`, previewable) rather than silently skipped; the
agent's own next `knap lint` call is the safety net that catches a bad
guess, which is the whole premise of this release's edit-verify loop.

### `edit_distance` (new, private helper)

```rust
/// Levenshtein edit distance, byte-wise. GFM slugs are already lowercase
/// ASCII alphanumerics and hyphens, so byte-wise is equivalent to
/// char-wise here and avoids a `Vec<char>` allocation.
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}
```

---

## Note Index Changes

### `NoteIndex::note_report` (new, extracted from `report`)

```rust
/// The `NoteSummary` for a single note — the same shape `report()` builds
/// per note, extracted so `knap index <file>` can fetch just one without
/// building (and discarding) summaries for every other note.
pub fn note_report(&self, path: &Path) -> Option<NoteSummary> {
    self.get_note(path).map(|note| self.note_summary(note))
}
```

`report()`'s existing per-note closure body moves into a new private
`fn note_summary(&self, note: &Note) -> NoteSummary`; `report()` becomes
`self.all_notes().collect::<Vec<_>>()` (sorted, as today)
`.into_iter().map(|n| self.note_summary(n)).collect()`. Pure refactor — same
output for `report()`, `note_report()` is the only new surface.

`NoteSummary`, `HeadingSummary`, and `LinkSummary` gain
`#[cfg_attr(test, derive(PartialEq, Debug))]` so a unit test can assert
`note_report(path)` equals the matching entry from `report().notes` without
hand-comparing fields (they have no `PartialEq`/`Debug` today since nothing
needed to compare them; this is additive and test-only).

---

## CLI Changes

### `knap lint --fail-on <severity>`

```rust
#[derive(clap::ValueEnum, Clone, Copy)]
#[value(rename_all = "lower")]
enum FailOn {
    Error,
    Warning,
    Info,
    Hint,
}
```

Added to the `Lint` variant in `src/cli/mod.rs`:
`#[arg(long, value_enum, default_value = "warning")] fail_on: FailOn` — the
default reproduces today's exact behavior, since every current diagnostic is
`WARNING` and today's bail condition is "any diagnostic at all."

`src/cli/lint.rs`:

```rust
fn severity_rank(severity: Option<DiagnosticSeverity>) -> i32 {
    match severity {
        Some(DiagnosticSeverity::ERROR) => 1,
        Some(DiagnosticSeverity::INFORMATION) => 3,
        Some(DiagnosticSeverity::HINT) => 4,
        _ => 2, // WARNING, and None — matches severity_label's existing default
    }
}

impl FailOn {
    fn rank(self) -> i32 {
        match self {
            FailOn::Error => 1,
            FailOn::Warning => 2,
            FailOn::Info => 3,
            FailOn::Hint => 4,
        }
    }
}
```

`LintReport` gains a field: `blocking_count: usize` — the count of
diagnostics with `severity_rank(d.severity) <= fail_on.rank()`, alongside
the existing `problem_count` (total diagnostics found, unfiltered — every
diagnostic is still printed/serialized regardless of `--fail-on`; the flag
only changes which ones count toward failing). `run`'s bail condition
switches from `problem_count > 0` to `blocking_count > 0`. Text-mode output
gains one line only when `blocking_count != problem_count`, e.g. `3
problem(s) in 2 file(s), 1 at or above --fail-on threshold`, to avoid
changing the default text output shape.

### `knap lint --since <git-ref>`

Added to the `Lint` variant: `#[arg(long)] since: Option<String>`.

`src/cli/lint.rs`:

```rust
/// Union of tracked changes (`git diff --name-only <git_ref>`) and untracked
/// new files (`git ls-files --others --exclude-standard`), each resolved to
/// a canonicalized absolute path so it can be matched against `NoteIndex`
/// paths regardless of the CLI's own relative/absolute working directory.
fn changed_paths_since(root: &Path, git_ref: &str) -> anyhow::Result<HashSet<PathBuf>> {
    let repo_root = git_output(root, &["rev-parse", "--show-toplevel"])
        .context("--since requires a git repository")?;
    let repo_root = PathBuf::from(repo_root.trim());

    let mut changed = HashSet::new();
    for rel in git_output(&repo_root, &["diff", "--name-only", git_ref])?.lines() {
        changed.insert(repo_root.join(rel));
    }
    for rel in git_output(&repo_root, &["ls-files", "--others", "--exclude-standard"])?.lines() {
        changed.insert(repo_root.join(rel));
    }
    Ok(changed.into_iter().map(|p| p.canonicalize().unwrap_or(p)).collect())
}

fn git_output(cwd: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .context("failed to run git")?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
```

`run` calls `changed_paths_since(&config.index_roots[0], since)?` when
`since` is `Some`, then retains only targets whose own canonicalized path is
in the returned set:
`targets.retain(|t| t.canonicalize().map(|c| changed.contains(&c)).unwrap_or(false))`.
Applied after the existing single-file-vs-directory target selection, so a
single-file `path` argument combined with `--since` degenerates to "lint this
file only if it's actually changed" — no special case needed.

No new dependency: shells out to the user's own `git`, matching the
project's dependency-light style (no `git2` crate, as CLI commands already
shell out to nothing today, but a git worktree is a reasonable thing to
assume for a workspace an agent is iterating in).

Edge cases:

- Not a git worktree → `git rev-parse --show-toplevel` fails →
  `anyhow::bail!` surfaces git's own stderr, non-zero exit.
- Invalid `<git-ref>` → `git diff` fails the same way.
- Deleted files appear in `git diff --name-only` but no longer exist on
  disk, so they're never in `idx` to begin with — no special-casing needed,
  the `retain` naturally excludes them.
- No changes since `<git-ref>` → empty target list → `0 problem(s) in 0
file(s)`, exit 0 (not an error).

### `knap index <path>` — a file argument scopes to one note

No new flag. `Index`'s existing `path: PathBuf` already accepts a file or a
directory today, but a file argument currently does something surprising
and undocumented: `config::for_path` treats the file's *parent directory*
as the index root, so `knap index some/note.md` silently prints the whole
parent directory's index, not just that note. This release gives file input
a real, documented meaning: print just that one note's neighborhood.

Doing this correctly needs one more fix, not just output-narrowing. This
release's `NoteSummary` includes `backlinks` — unlike `lint`, which only
ever needs a target file's own outgoing links, `index` on a single file
still needs the *whole vault* indexed to find who links to it. Indexing off
`file.parent()` (today's behavior) would silently drop backlinks from
anywhere else in the vault — the same bug `rename-heading`/`rename-tag`
already hit and fixed in v0.12 by resolving config off `cwd` instead of the
target file (see `docs/ARCHITECTURE.md`'s CLI section). `knap index` adopts
the identical fix:

```rust
pub fn run(path: &Path, json: bool) -> anyhow::Result<()> {
    let config = if path.is_file() {
        config::for_path(Path::new("."), None)? // whole vault, not just this file's directory
    } else {
        config::for_path(path, None)? // unchanged: today's directory behavior
    };
    let extensions: Vec<&str> = config.extensions.iter().map(String::as_str).collect();
    let (idx, _) = index::build(&config.index_roots, &extensions);

    if path.is_file() {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let note = idx
            .all_notes()
            .find(|n| n.path.canonicalize().map(|p| p == canon).unwrap_or(false))
            .ok_or_else(|| anyhow::anyhow!("no indexed note at {}", path.display()))?;
        if json {
            let summary = idx.note_report(&note.path).expect("just found in idx.all_notes()");
            println!("{}", serde_json::to_string_pretty(&summary)?);
        } else {
            print_note(note, &idx);
        }
        return Ok(());
    }

    // directory: unchanged full-workspace behavior below, using the same
    // print_note for text mode
}
```

Same assumption `rename-heading`/`rename-tag` already document: the CLI is
invoked from within the vault (or its root), so `cwd` reaches every note a
backlink could come from. Text mode's existing per-note print loop is
extracted into `fn print_note(note: &Note, idx: &NoteIndex)`, used by both
the new single-file branch and the existing all-notes loop, so the two paths
render a note identically.

`--json` on a file path serializes a bare `NoteSummary` (via `note_report`),
not wrapped in an `IndexReport` — the payload is one note's worth, not the
workspace tag map too.

### `knap fix [path] [--dry-run]` (new subcommand)

New module `src/cli/fix.rs`, new `Commands::Fix { path: PathBuf, dry_run:
bool }` in `src/cli/mod.rs` (`path` defaults to `.`, matching `Lint`).

```rust
/// `config::for_path` → `index::build`, mirroring `lint`. For every link in
/// every target note, computes the fix `compute_create_missing_file_fix` or
/// `compute_anchor_fix`+`suggest_anchor_fix` would make — skipping anything
/// ambiguous — then either prints the plan (`--dry-run`) or merges every
/// fix's edit into one `WorkspaceEdit` and hands it to `edit::apply`.
pub fn run(path: &Path, dry_run: bool) -> anyhow::Result<()>
```

```rust
struct PlannedFix {
    edit: WorkspaceEdit,
    description: String, // e.g. "create notes/new-note.md" or
                          // "notes/a.md: anchor '#old' → '#my-section'"
}
```

Per target note, per `link` in `note.md_links` (skipping `link.target.is_empty()`,
same as `handle_code_actions`):

- `ResolvedLink::Broken` → `PlannedFix` from `compute_create_missing_file_fix`.
- `ResolvedLink::Found(target_path)` with an anchor that doesn't match any
  heading in `idx.get_note(&target_path)` → `suggest_anchor_fix(anchor,
target_note)`; `Some(heading)` → `PlannedFix` from `compute_anchor_fix`
  targeting `slug(&heading.text)`; `None` → skip (left for an interactive
  code action, or a future `knap fix --interactive`, not this release).

No fixes found → print `no safe fixes found` and return `Ok(())` (not an
error — a clean vault is success, same posture as `rename-*`'s "nothing to
update" case in v0.12).

`--dry-run` → print `would <description>` per `PlannedFix`, apply nothing.

Otherwise → merge every `PlannedFix.edit` into one
`Vec<DocumentChangeOperation>` (each `compute_anchor_fix` result is
`changes`-shaped — one `(uri, Vec<TextEdit>)` — and gets wrapped into a
`DocumentChangeOperation::Edit(TextDocumentEdit { .. })`, the same
conversion `rename-file` already does in `src/cli/rename.rs`; each
`compute_create_missing_file_fix` result is already `document_changes`-shaped
and its ops are appended directly), call
`edit::apply(&WorkspaceEdit { document_changes: Some(DocumentChanges::Operations(merged)), ..Default::default() })`,
then print `applied N fix(es) in M file(s)` plus each fix's `description`
line.

No `--since` integration this release — `fix` always scans the same target
set `lint` would without `--since` (single file, or every indexed note).
Combining the two is a reasonable future extension, not built now, same as
how v0.12 left `edit::apply`'s `Delete` arm as an explicit not-yet-implemented
error rather than guessing at unneeded scope.

---

## Documentation Changes

### `skill/knap/SKILL.md` (new)

A ready-to-copy Claude Code skill, shipped in the repo at `skill/knap/` so a
vault owner can `cp -r skill/knap ~/.claude/skills/` (or into a project's
`.claude/skills/`) to teach a coding agent knap's conventions directly,
instead of the agent inferring them from `--help` text. Not consumed by this
repo's own development — that's `AGENTS.md`'s job; this is for agents
working in a vault that merely has `knap` installed.

Content (frontmatter `name`/`description` per the Claude Code skill format,
then a markdown body covering):

- When to reach for it: any task that edits Markdown notes in a vault with a
  `knap.toml` or an obvious note structure.
- The loop this release exists for: edit → `knap lint --json [--since <ref>]`
  → branch on each diagnostic's `code` (the six values from US-D11, listed
  in a table) → `knap fix` for the two codes it can resolve
  (`broken-link`, `broken-anchor`) or `knap rename-*` for a deliberate
  restructure → `knap lint` again to confirm clean.
- One example `--json` payload showing `code`, and one showing
  `blocking_count` vs. `problem_count`.
- `knap index <file> --json` for inspecting a just-edited note's neighborhood
  without paging the full workspace snapshot.
- A pointer to `README.md` for the full command reference — the skill stays
  a workflow guide, not a duplicate of `--help`.

No unit or integration test — verified by a manual read-through checkpoint
in the plan (Step 9), confirming every command and flag it references
actually exists in this release.

---

## Testing

### Unit tests

| Test (file)                                                          | What it verifies                                                                                     |
| ----------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `diagnostic_broken_link_has_broken_link_code` (`src/handlers.rs`)       | Broken-link diagnostic's `code` is `Some(NumberOrString::String("broken-link".into()))`              |
| `diagnostic_broken_anchor_has_broken_anchor_code` (`src/handlers.rs`)   | Broken-anchor diagnostic's `code` is `"broken-anchor"`                                                |
| `diagnostic_missing_frontmatter_has_missing_frontmatter_code` (`src/handlers.rs`) | Required field missing on a note with **no** frontmatter block → `code` is `"missing-frontmatter"` |
| `diagnostic_missing_required_field_has_missing_required_field_code` (`src/handlers.rs`) | Required field missing on a note **with** a frontmatter block → `code` is `"missing-required-field"` |
| `diagnostic_invalid_value_has_invalid_field_value_code` (`src/handlers.rs`) | Value not in the allowed list → `code` is `"invalid-field-value"`                                    |
| `diagnostic_unknown_key_has_unknown_field_code` (`src/handlers.rs`)     | Unrecognized frontmatter key → `code` is `"unknown-field"`                                            |
| `edit_distance_identical_strings_is_zero` (`src/handlers.rs`)          | `edit_distance("x", "x") == 0`                                                                       |
| `edit_distance_counts_substitutions` (`src/handlers.rs`)               | `edit_distance("abc", "abd") == 1`                                                                    |
| `suggest_anchor_fix_picks_unique_closest` (`src/handlers.rs`)          | A target with headings at distinct distances returns the closest one                                 |
| `suggest_anchor_fix_none_on_tied_distance` (`src/handlers.rs`)         | Two headings equally close to the broken slug → `None`                                                |
| `suggest_anchor_fix_none_when_no_headings` (`src/handlers.rs`)         | Target note has zero headings → `None`                                                                |
| `compute_create_missing_file_fix_matches_prior_action_shape` (`src/handlers.rs`) | Output matches the `document_changes` shape `handle_code_actions` built inline before extraction (regression net) |
| `compute_anchor_fix_replaces_anchor_range` (`src/handlers.rs`)         | Single `TextEdit` at `anchor_range` with `new_anchor` as `new_text`                                    |
| `note_report_matches_report_entry` (`src/index/mod.rs`)                | `idx.note_report(path)` equals the corresponding entry in `idx.report().notes`                       |
| `note_report_none_for_unindexed_path` (`src/index/mod.rs`)             | A path not in the index → `None`, not a panic                                                        |
| `severity_rank_orders_error_above_warning_above_info_above_hint` (`src/cli/lint.rs`) | `severity_rank` values are strictly increasing across `ERROR, WARNING/None, INFORMATION, HINT`   |
| `fail_on_default_matches_todays_behavior` (`src/cli/lint.rs`)          | `FailOn::Warning.rank()` (the default) admits every diagnostic `compute_diagnostics` emits today (all `WARNING`) |

Existing `handle_code_actions` and `compute_diagnostics` tests must stay
green unmodified — the extractions and the `code` addition are additive, not
behavior changes to which diagnostics or actions fire.

### Integration tests (`tests/cli.rs`, extending v0.11/v0.12's suite)

| Test                                             | What it verifies                                                                                       |
| ------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `lint_json_diagnostics_include_stable_code`       | `knap lint tests/fixtures/lint_basic --json` → both diagnostics carry `code` (`broken-link`, `broken-anchor`) |
| `lint_fail_on_error_passes_when_only_warnings_present` | `--fail-on error` on `lint_basic` (WARNING-only diagnostics) → exit 0                                |
| `lint_fail_on_warning_matches_default_behavior`   | `--fail-on warning` (and the flag omitted) both exit 1 on `lint_basic`, identical to today               |
| `lint_since_scopes_to_git_changed_files`          | git-initialized fixture, two notes, a commit, then a broken link introduced in only one; `--since <that commit>` reports only the touched file |
| `lint_since_includes_untracked_new_files`         | A new, uncommitted note with a broken link is included by `--since <ref>`                                |
| `lint_since_outside_git_repo_errors`              | Non-zero exit, clear stderr, when `--since` is used outside a git worktree                               |
| `index_file_path_prints_single_note_neighborhood` | `knap index <file> --json` → parses as one `NoteSummary` object, not an `IndexReport` envelope             |
| `index_file_path_includes_backlinks_from_outside_its_directory` | A note in a subdirectory, linked from a note in a sibling directory; `knap index <the nested file>` (run from the vault root) still reports the backlink — proves the `cwd`-rooted fix, not `file.parent()` |
| `index_unindexed_file_path_errors`                | Non-zero exit for a file path the index doesn't have                                                      |
| `fix_creates_missing_file`                        | Broken-link fixture → `knap fix` creates the stub file; a subsequent `knap lint` is clean                 |
| `fix_replaces_unambiguous_broken_anchor`           | Fixture with a broken anchor and one clearly-closest heading → rewritten; subsequent `knap lint` is clean |
| `fix_skips_ambiguous_anchor`                       | Fixture with a broken anchor and two equally-close headings → left alone; still flagged by `knap lint`    |
| `fix_dry_run_makes_no_changes`                     | `--dry-run` prints the plan; the fixture directory is byte-for-byte unchanged                             |
