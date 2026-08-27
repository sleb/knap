# v0.24 Design — Agent Hints

Covers the stories in the agent-hints release slice:

| Story  | Feature                                                          |
| ------ | ----------------------------------------------------------------- |
| US-D24 | `knap lint` gains a report-level `hint` (text and `--json`)       |
| US-D25 | `knap apply` gains a report-level `hint`; error text enriched     |
| US-D26 | `knap rename-file`/`rename-heading`/`rename-tag` gain `--json` + `hint` |
| US-D27 | `knap index` gains a report-level `hint`                          |

---

## Goal

An agent driving knap headlessly (`skill/knap/SKILL.md`'s lint → fix/rename →
lint loop) sees one plain-language sentence, in both text and `--json`
output, naming its next move — "broken links found, use `apply` or
`rename-*`", "run `lint` to confirm", "chain more renames or verify now" —
instead of having to already know the loop `SKILL.md` documents before the
CLI's own output means anything.

This is a deliberately small mechanism: one `hint: Option<String>` field per
report, built from a couple of `bool`s the caller already has in scope (has
this report got any broken-link/broken-anchor diagnostics? any frontmatter
diagnostics? did this apply run for real or `--dry-run`?). No per-diagnostic
decoration, no fixed vocabulary of `action`/`op`/`target` keys for an agent
to parse — just a string, the same one a human reads in the terminal.

## Non-goals

- **No per-diagnostic `hint`.** An earlier version of this design attached a
  structured `hint` object (`{"action": ..., "op": ..., "target": ...}`) to
  every individual diagnostic in a `lint --json` report, built from a `match`
  over each diagnostic's `code` and its `--suggest` candidates. That's a
  second decision surface to keep in sync with `handlers.rs` and `SKILL.md`
  both, for a payoff (letting an agent skip reading `data.suggestions`
  itself) this release isn't trying to deliver. One aggregate sentence per
  report is enough to point at the next command; which specific link to
  repoint still comes from `data.suggestions`, exactly as it does today.
- **No fixed `action`/`reason` vocabulary.** `hint` is prose, not a value an
  agent branches on programmatically — `SKILL.md` still documents `code` as
  the thing to branch on (unchanged by this release). `hint` is a nudge for
  whichever agent or human is reading the output, not a second protocol.
- **No new diagnostic, ranking, or index logic.** Every hint is pure
  CLI-output shaping over data the relevant handler, `apply`, or index
  method already produced. `src/handlers.rs`'s
  `compute_diagnostics`/`compute_diagnostics_with_suggestions` are
  unchanged — those are shared with the LSP server's
  `textDocument/publishDiagnostics`, and hints are a CLI-only concept that
  must not leak into the editor-facing diagnostic stream.
- **No structured JSON error envelope for `apply` failures.** Today, any
  `apply_one` failure (including the "produced unparseable markdown"
  round-trip rejection) bypasses `--json` output entirely and always prints
  as a plain-text anyhow chain on stderr, regardless of whether `--json` was
  passed. US-D25 folds its hint straight into that existing text instead of
  building a JSON envelope for it; see
  [issue #82](https://github.com/sleb/knap/issues/82) for the longer-term
  idea of removing the underlying `range`-staleness fragility rather than
  just describing around it.
- **`rename-*`'s `hint` is a fixed string, not conditional.** Unlike
  lint/apply/index, there's no data on hand to branch on — a single
  `rename-*` call has no way to know whether the caller is mid-batch or
  done. The hint is the same static sentence every time (see US-D26 below),
  reflecting `SKILL.md`'s existing "chain renames, verify once at the end"
  guidance rather than trying to infer where in that sequence this call is.

---

## CLI Changes

New shared module, `src/cli/hint.rs`, holding the four report-level hint
builders — one per subcommand, each a small function over data its caller
already has, not a generic builder vocabulary:

```rust
//! Builds the single `hint` string each `--json`-emitting subcommand's
//! report carries (and text mode prints as a trailing line), from a couple
//! of bools the caller already has in scope. Pure formatting — no new
//! diagnostic, ranking, or index logic lives here.

use crate::handlers::{
    CODE_BROKEN_ANCHOR, CODE_BROKEN_LINK, CODE_INVALID_FIELD_VALUE,
    CODE_MISSING_FRONTMATTER, CODE_MISSING_REQUIRED_FIELD, CODE_UNKNOWN_FIELD,
};

/// `knap lint`'s hint. `has_repointable` is true if any diagnostic's `code`
/// is `broken-link`/`broken-anchor`; `has_manual` is true if any is one of
/// the four frontmatter codes. Both false (including the clean case, zero
/// diagnostics) returns `None` — nothing to point at.
pub(crate) fn lint(has_repointable: bool, has_manual: bool) -> Option<String> {
    let mut clauses = Vec::new();
    if has_repointable {
        clauses.push(
            "broken-link/broken-anchor diagnostics: rerun with --suggest and \
             `knap apply` a candidate, or use `knap rename-file`/`rename-heading`/\
             `rename-tag` if you're mid-restructure",
        );
    }
    if has_manual {
        clauses.push("frontmatter diagnostics need a manual edit");
    }
    (!clauses.is_empty()).then(|| clauses.join("; ") + ".")
}

/// `knap apply`'s hint. `--dry-run` writes nothing, so there's nothing for a
/// follow-up lint to confirm.
pub(crate) fn apply(dry_run: bool) -> Option<String> {
    (!dry_run).then(|| "run `knap lint` to confirm the workspace is clean.".to_string())
}

/// `knap rename-file`/`rename-heading`/`rename-tag`'s hint — always the same
/// sentence; see the design doc's Non-goals for why this one isn't
/// conditional.
pub(crate) fn rename() -> Option<String> {
    Some(
        "chain more rename-* calls if this restructure isn't done yet; \
         run `knap lint` once at the end to verify."
            .to_string(),
    )
}

/// `knap index`'s hint.
pub(crate) fn index(has_broken_link: bool) -> Option<String> {
    has_broken_link
        .then(|| "broken link(s) found — run `knap lint --suggest` for fix candidates.".to_string())
}
```

`has_repointable`/`has_manual` are computed by each caller with a plain
`.iter().any(...)` over the `code`s it already has — no shared helper beyond
that, since `lint.rs` is the only caller with a `Vec<Diagnostic>` in hand.

**Implementation note:** `CODE_BROKEN_LINK` and friends are `const` (private
to the module) in `src/handlers.rs` today. This release adds `pub(crate)` to
those six declarations so `hint.rs`/`lint.rs` can match on them directly
instead of duplicating the string literals.

Every report gets the same field:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
hint: Option<String>,
```

`skip_serializing_if` keeps the common "nothing to say" case
(`--dry-run`, a clean lint, an index target with no broken links) from
adding a noisy `"hint": null` to output that's otherwise unchanged — an
agent parsing the report checks for the key's presence, same as it already
does for any other optional field. Text mode mirrors this: a `Some(hint)`
prints one extra blank line + `hint: {text}` after the existing summary
line; `None` changes nothing about today's text output.

### `knap lint` (US-D24)

`src/cli/lint.rs`:

```rust
#[derive(Serialize)]
struct LintReport {
    diagnostics: Vec<FileDiagnostics>,
    problem_count: usize,
    file_count: usize,
    blocking_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}
```

In `run`, after `files`/`blocking_count` are computed and before building
`LintReport`:

```rust
let codes: HashSet<&str> = files
    .iter()
    .flat_map(|f| &f.diagnostics)
    .filter_map(|d| match &d.code {
        Some(NumberOrString::String(c)) => Some(c.as_str()),
        _ => None,
    })
    .collect();
let has_repointable = codes.contains(CODE_BROKEN_LINK) || codes.contains(CODE_BROKEN_ANCHOR);
let has_manual = [
    CODE_MISSING_FRONTMATTER,
    CODE_MISSING_REQUIRED_FIELD,
    CODE_INVALID_FIELD_VALUE,
    CODE_UNKNOWN_FIELD,
]
.iter()
.any(|c| codes.contains(c));
let hint = hint::lint(has_repointable, has_manual);
```

Text mode prints it right after today's `problem(s) in N file(s)` /
`--fail-on` lines, before the function's existing `blocking_count > 0` bail
check (so the hint still reaches stderr-adjacent stdout even when `lint`
exits non-zero):

```rust
if let Some(hint) = &hint {
    println!();
    println!("hint: {hint}");
}
```

Example — broken link, no frontmatter problems:

```json
{
  "diagnostics": [ ... ],
  "problem_count": 1, "file_count": 1, "blocking_count": 1,
  "hint": "broken-link/broken-anchor diagnostics: rerun with --suggest and `knap apply` a candidate, or use `knap rename-file`/`rename-heading`/`rename-tag` if you're mid-restructure."
}
```

Example — clean workspace: `hint` key absent entirely (`problem_count: 0`,
no `hint` field).

### `knap apply` (US-D25)

`src/cli/apply.rs`:

```rust
#[derive(Serialize)]
struct ApplyReport {
    dry_run: bool,
    operations: Vec<AppliedOp>,
    files_touched: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}
```

In `run`, right before constructing `report`:

```rust
let hint = hint::apply(dry_run);
```

Text mode prints it after the existing `N operation(s), N file(s) touched`
line, same `if let Some` pattern as lint.

**Error-path text enrichment** (unchanged from the original design). The two
`validate_link_round_trip` calls in `apply_one` fold the re-fetch-`range`
advice into their `.context(...)` message, since that's the one place this
failure actually surfaces (`--json` is bypassed entirely on error, per
Non-goals):

```rust
validate_link_round_trip(&file_abs, &content, range.start.line, |l| {
    index::unescape_link_target(&l.target) == *target
})
.context(
    "repoint-link produced unparseable markdown — re-fetch range from a \
     fresh `knap lint --suggest --json` rather than adjusting it by hand",
)?;
```

(and the equivalent for `repoint-anchor`). No new error type, no
`process::exit` — the anyhow chain still bubbles up through `run` → `main`
exactly as today; only the message text changes.

### `knap rename-file`/`rename-heading`/`rename-tag --json` (US-D26)

None of the three has a `--json` flag today (`src/cli/mod.rs`'s
`Commands::RenameFile`/`RenameHeading`/`RenameTag`, `src/cli/rename.rs`'s
`run_file`/`run_heading`/`run_tag`) — text-only output. Add the flag to all
three `Commands` variants and thread a `json: bool` through to a shared
report type:

```rust
#[derive(Serialize)]
struct RenameReport {
    op: &'static str,       // "rename-file" | "rename-heading" | "rename-tag"
    summary: String,        // same text as today's text-mode line, minus the trailing "(N file(s) touched)"
    files_touched: usize,
    hint: Option<String>,   // always Some(...) — see hint::rename()
}
```

`run_file`/`run_heading`/`run_tag` keep their existing text-mode `println!`
path unchanged when `json` is `false`, plus one new line printing
`hint::rename()`'s text; under `json: true` they build a `RenameReport`
(`hint: hint::rename()`) and print it via `serde_json::to_string_pretty`
instead — same shape `apply::run`'s `json` branch already follows.
`op`/`summary`/`files_touched` reuse exactly what each function already
computes (`rename_file_at`'s `touched` count, the same `old`/`new` or
`old`/`new`-in-`file` values used in today's `println!`) — no new
computation, just a second serialization path plus one static line.

### `knap index --json` (US-D27)

`src/index::{IndexReport, NoteSummary}` are unchanged — the hint wraps them
from `src/cli/index.rs` rather than becoming a field on either type, keeping
"what to do next" out of the Note Index's own report contract:

```rust
#[derive(Serialize)]
struct IndexOutput<T: Serialize> {
    #[serde(flatten)]
    report: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}
```

`#[serde(flatten)]` keeps today's top-level shape intact (`notes`/`tags` for
a directory target, `path`/`title`/`headings`/`links`/`backlinks`/`tags` for
a file target) with `hint` added as a sibling key — purely additive, no
existing field renamed or nested.

```rust
fn has_broken_link(links: &[index::LinkSummary]) -> bool {
    links.iter().any(|l| l.resolved.is_none())
}
```

Used in `run`:

- File target: `hint::index(has_broken_link(&summary.links))`.
- Directory target: `hint::index(report.notes.iter().any(|n| has_broken_link(&n.links)))`.

Text mode gets the same one-line addition: `print_note` prints the hint
right after a note's `referenced by:` block when that note has a broken
link (file-target path); the whole-workspace listing prints it once, after
the loop over notes, when any note in the report has one.

---

## Testing

### Unit tests

| Test                                                             | What it verifies                                                                        |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `hint_lint_none_when_neither_flag_set`                                | `hint::lint(false, false)` is `None`                                                       |
| `hint_lint_repointable_only`                                          | `hint::lint(true, false)` mentions `apply`/`rename-*`, not "frontmatter"                    |
| `hint_lint_manual_only`                                               | `hint::lint(false, true)` mentions "frontmatter", not `apply`/`rename-*`                    |
| `hint_lint_both_flags_joins_both_clauses`                             | `hint::lint(true, true)` contains both clauses                                              |
| `hint_apply_some_when_not_dry_run`                                    | `hint::apply(false)` is `Some` and mentions `knap lint`                                     |
| `hint_apply_none_when_dry_run`                                        | `hint::apply(true)` is `None`                                                              |
| `hint_rename_is_always_some`                                          | `hint::rename()` is `Some` and mentions `rename-*` and `knap lint`                          |
| `hint_index_some_when_broken_link`                                    | `hint::index(true)` is `Some` and mentions `knap lint --suggest`                            |
| `hint_index_none_when_no_broken_link`                                 | `hint::index(false)` is `None`                                                             |
| `lint_report_hint_absent_when_clean`                                  | `LintReport` serialized for zero diagnostics has no `hint` key                              |
| `lint_report_hint_present_for_broken_link`                            | `LintReport` for a workspace with one `broken-link` diagnostic has a `hint` key mentioning `apply` |
| `apply_report_hint_absent_under_dry_run`                              | `ApplyReport` serialized under `--dry-run` has no `hint` key                                |
| `apply_report_hint_present_when_not_dry_run`                          | Same, non-dry-run, has a `hint` key                                                        |
| `apply_one_repoint_link_error_message_names_lint_suggest_rerun`       | The error from the existing `apply_one_repoint_link_rejects_range_that_eats_closing_paren` scenario now contains "re-fetch range from a fresh `knap lint --suggest --json`" |
| `apply_one_repoint_anchor_error_message_names_lint_suggest_rerun`     | Same, for the `repoint-anchor` round-trip rejection                                         |
| `rename_report_json_serializes_op_summary_files_touched_and_hint`     | A `RenameReport` for each of the three ops serializes with the right `op` string and a non-empty `hint` |
| `run_file_json_true_prints_json_not_text`                             | `rename::run_file` with `json: true` (captured stdout) parses as JSON with `op == "rename-file"`, and prints no plain-text line |
| `run_file_json_false_preserves_existing_text_output_plus_hint_line`   | `rename::run_file` with `json: false` still prints today's `"{old} → {new} (N file(s) touched)"` line, plus one new `hint: ...` line |
| `index_output_hint_present_for_file_target_with_broken_link`          | `IndexOutput` for a single file whose `links` includes one `resolved: None` entry has a `hint` key |
| `index_output_hint_absent_for_file_target_all_resolved`               | Same, all links resolved → no `hint` key                                                    |
| `index_output_hint_present_for_directory_target_with_broken_link`     | `IndexOutput` for a directory target with at least one note having a broken link has a `hint` key |
| `index_output_top_level_shape_unchanged_by_flatten`                   | `IndexOutput<NoteSummary>` and `IndexOutput<IndexReport>` still serialize `path`/`notes`/etc. at the top level (via `#[serde(flatten)]`), not nested under a `report` key |

### Integration tests (`tests/cli.rs`)

| Test                                                    | What it verifies                                                                                       |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `lint_json_clean_workspace_has_no_hint`                     | Real `knap lint --json` subprocess on a workspace with no problems has no `hint` key                        |
| `lint_json_broken_link_has_hint`                             | Real `knap lint --json` subprocess on a fixture with a broken link has a `hint` key mentioning `apply`       |
| `lint_text_broken_link_prints_hint_line`                     | Real `knap lint` (no `--json`) subprocess stdout contains a `hint: ` line                                    |
| `apply_json_non_dry_run_has_hint`                            | Real `knap apply --json` subprocess applying a batch has a `hint` key                                        |
| `apply_json_dry_run_has_no_hint`                             | Real `knap apply --dry-run --json` subprocess has no `hint` key                                              |
| `apply_stale_range_error_text_mentions_lint_suggest`         | Real `knap apply` subprocess given a stale `range` fails with stderr containing the re-fetch-range hint text |
| `rename_file_json_flag_emits_valid_json_with_hint`           | Real `knap rename-file --json <old> <new>` subprocess stdout parses as JSON with `op == "rename-file"` and a `hint` key |
| `rename_file_text_prints_hint_line`                          | Real `knap rename-file <old> <new>` (no `--json`) subprocess stdout contains a `hint: ` line                 |
| `index_json_file_target_broken_link_has_hint`                | Real `knap index <file> --json` subprocess against a fixture with one broken link has a `hint` key           |

> **Manual checkpoint:** No editor checkpoint — every change here is CLI
> text/`--json` output shaping, unreachable from an LSP client; covered
> entirely by unit and integration tests.
