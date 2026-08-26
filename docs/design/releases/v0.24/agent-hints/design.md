# v0.24 Design — Agent Hints

Covers the stories in the agent-hints release slice:

| Story  | Feature                                                          |
| ------ | ----------------------------------------------------------------- |
| US-D24 | `knap lint --json` gains a report-level and per-diagnostic `hint` |
| US-D25 | `knap apply --json` gains a report-level `hint`; error text enriched |
| US-D26 | `knap rename-file`/`rename-heading`/`rename-tag` gain `--json`    |
| US-D27 | `knap index --json` gains a report-level `hint`                   |

---

## Goal

An agent driving knap headlessly (`skill/knap/SKILL.md`'s lint → fix/rename →
lint loop) can act on a fixed `action` field the CLI emits alongside its
existing `--json` output, instead of re-deriving "what do I do with this
diagnostic/report" from `code`/`data.suggestions`/`text_mismatch` by hand
every time. The decision rules this replaces — which `code` maps to which
resolution, when a `repoint-*` needs a fresh `range`, when `text_mismatch`
means stop and look by hand — live only in `SKILL.md` today, have to be kept
in sync with the CLI by hand, and get spent as context on every trigger
regardless of whether the current result needs them. Moving the mechanical
part of that logic into the CLI's own output makes it versioned and tested
with the command itself, and lets `SKILL.md` shrink to pointing at
`hints`/`hint` rather than re-deriving it (setup for #76's restructure,
which follows this release).

These four stories are one release slice because they're the same mechanism
— a `hints`/`hint` field built entirely from data each subcommand has
already computed — applied to every `--json`-emitting subcommand `SKILL.md`
documents a decision rule for.

## Non-goals

- **No new diagnostic, ranking, or index logic.** Every hint is pure
  CLI-output shaping over a `Vec<Diagnostic>`/`ApplyReport`/`IndexReport`
  the relevant handler or index method already produced.
  `src/handlers.rs`'s `compute_diagnostics`/`compute_diagnostics_with_suggestions`
  are unchanged — those are shared with the LSP server's
  `textDocument/publishDiagnostics`, and hints are an agent-CLI-only concept
  that must not leak into the editor-facing diagnostic stream.
- **No structured JSON error envelope for `apply` failures.** Today, any
  `apply_one` failure (including the "produced unparseable markdown"
  round-trip rejection) bypasses `--json` output entirely and always prints
  as a plain-text anyhow chain on stderr, regardless of whether `--json` was
  passed — there's no existing JSON-error contract to extend. US-D25 folds
  its hint straight into that existing text instead of building one; see
  [issue #82](https://github.com/sleb/knap/issues/82) for the longer-term
  idea of removing the underlying `range`-staleness fragility rather than
  just describing around it.
- **`rename-*`'s new `hints` field carries no content this release.**
  `SKILL.md` already tells agents to chain `rename-*` calls with no lint in
  between and verify once at the end of the batch — a per-call "go lint now"
  hint would contradict that. The right guidance here is a cross-call
  pattern that stays owned by `SKILL.md` prose, not a single-call hint. The
  field is added now (empty) purely so `rename-*`'s `--json` shape is
  future-proof and consistent with every other subcommand.

---

## CLI Changes

_No config, parser, note index, or handler changes._ New shared module,
`src/cli/hints.rs`, holding small `serde_json::Value` builders reused by
`lint`/`apply`/`rename-*`/`index` so each hint's `action`/`reason` vocabulary
is defined once, not duplicated per call site:

```rust
//! Small `serde_json::Value` builders for the `hints`/`data.hint` fields
//! `--json` output attaches, naming an agent's next action. Pure formatting
//! over data the caller already computed — no decision logic lives here
//! beyond picking which builder applies.

pub(crate) fn none_clean() -> serde_json::Value {
    serde_json::json!({ "action": "none", "reason": "clean" })
}

pub(crate) fn lint(reason: &str) -> serde_json::Value {
    serde_json::json!({ "action": "lint", "reason": reason })
}

pub(crate) fn manual(reason: &str) -> serde_json::Value {
    serde_json::json!({ "action": "manual", "reason": reason })
}

pub(crate) fn disambiguate(reason: &str) -> serde_json::Value {
    serde_json::json!({ "action": "disambiguate", "reason": reason })
}

pub(crate) fn apply_repoint_link(target: &str) -> serde_json::Value {
    serde_json::json!({ "action": "apply", "op": "repoint-link", "target": target })
}

pub(crate) fn apply_repoint_anchor(anchor: &str) -> serde_json::Value {
    serde_json::json!({ "action": "apply", "op": "repoint-anchor", "anchor": anchor })
}
```

Two shapes recur across every subcommand, both documented here once instead
of per-story below:

- **Report-level `hints`** is always a `Vec<serde_json::Value>` (0 or more
  items) — a report-wide "what's next," independent of any single
  diagnostic/operation.
- **Per-diagnostic `hint`** (lint only) is a single object, not an array —
  a diagnostic has exactly one next action.

### `knap lint --json` (US-D24)

`src/cli/lint.rs`:

```rust
#[derive(Serialize)]
struct LintReport {
    diagnostics: Vec<FileDiagnostics>,
    problem_count: usize,
    file_count: usize,
    blocking_count: usize,
    hints: Vec<serde_json::Value>,
}
```

`hints` is `[hints::none_clean()]` when `problem_count == 0`, else `[]` — the
actionable content for a non-clean report lives per-diagnostic instead.

Per-diagnostic `data.hint` is attached by a new post-processing pass,
`attach_hint(d: &mut Diagnostic, suggest_n: usize)`, called once per
diagnostic in `run` after `compute_diagnostics_with_suggestions` returns —
after the existing `files` vector is built, before it's wrapped into
`LintReport`. It never touches `handlers.rs`; it only reads/extends the
`Diagnostic`s the handler already returned:

```rust
/// Attaches a `hint` to `d.data` (creating `data` if the diagnostic has
/// none yet) naming the next action for this specific diagnostic. Reads
/// only `d.code` and whatever `compute_diagnostics_with_suggestions`
/// already put in `d.data` — no new ranking or lookup.
fn attach_hint(d: &mut Diagnostic, suggest_n: usize) {
    let Some(NumberOrString::String(code)) = &d.code else {
        return;
    };
    let hint = match code.as_str() {
        CODE_BROKEN_LINK | CODE_BROKEN_ANCHOR => {
            match &d.data {
                Some(data) if data["text_mismatch"] == true => {
                    hints::disambiguate("text_mismatch")
                }
                Some(data) => {
                    let target = data["suggestions"][0]["target"].as_str().unwrap_or_default();
                    if code == CODE_BROKEN_LINK {
                        hints::apply_repoint_link(target)
                    } else {
                        hints::apply_repoint_anchor(target)
                    }
                }
                None if suggest_n == 0 => hints::lint("rerun_with_suggest"),
                None => hints::disambiguate("no_candidates"),
            }
        }
        CODE_MISSING_FRONTMATTER
        | CODE_MISSING_REQUIRED_FIELD
        | CODE_INVALID_FIELD_VALUE
        | CODE_UNKNOWN_FIELD => hints::manual(code),
        _ => return, // unrecognized code (forward compat) — no hint
    };
    d.data.get_or_insert_with(|| serde_json::json!({}))["hint"] = hint;
}
```

The `None if suggest_n == 0` / `None` split matters: `d.data` is only ever
`Some` when `compute_diagnostics_with_suggestions` found at least one
candidate (it early-returns `diagnostics` unmodified for `top_n == 0`, and
only sets `d.data` `if !suggestions.is_empty()`). So `d.data == None` is
ambiguous on its own between "`--suggest` wasn't passed" and "`--suggest`
was passed but found zero candidates for this diagnostic" — `suggest_n`
(already in scope in `run`) disambiguates the two, giving `rerun_with_suggest`
only in the first case and `disambiguate("no_candidates")` in the second.

`CODE_BROKEN_LINK`/`CODE_BROKEN_ANCHOR`/the four frontmatter `CODE_*`
constants stay `const` in `src/handlers.rs`, unchanged — `lint.rs` needs
`pub(crate)` visibility on them (already the case, since `compute_diagnostics`
sets them via `NumberOrString::String(CODE_BROKEN_LINK.to_string())` today)
or its own copy; reuse the existing constants, don't duplicate the strings.

Example — clean workspace:

```json
{ "diagnostics": [], "problem_count": 0, "file_count": 0, "blocking_count": 0,
  "hints": [{ "action": "none", "reason": "clean" }] }
```

Example — unambiguous `--suggest` candidate:

```json
{ "code": "broken-link", "message": "Link target not found: 'docs/missing.md'",
  "data": {
    "suggestions": [{ "target": "docs/mission.md", "distance": 2, "text_distance": 5 }],
    "hint": { "action": "apply", "op": "repoint-link", "target": "docs/mission.md" }
  } }
```

Example — `text_mismatch` present:

```json
{ "data": { "suggestions": [...], "text_mismatch": true,
    "hint": { "action": "disambiguate", "reason": "text_mismatch" } } }
```

### `knap apply --json` (US-D25)

`src/cli/apply.rs`:

```rust
#[derive(Serialize)]
struct ApplyReport {
    dry_run: bool,
    operations: Vec<AppliedOp>,
    files_touched: usize,
    hints: Vec<serde_json::Value>,
}
```

In `run`, right before constructing `report`:

```rust
let hints = if dry_run { vec![] } else { vec![hints::lint("confirm_clean")] };
```

`--dry-run` never writes, so there's nothing for a follow-up `lint` to
confirm.

**Error-path text enrichment.** `apply_one`'s two `validate_link_round_trip`
calls change their `.context(...)` message to fold in the re-fetch-`range`
hint, so the advice reaches the agent in the one place this failure actually
surfaces (US-D25's second half):

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
exactly as today; only the message text changes. See Non-goals above for
why this stays text instead of a JSON envelope.

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
    hints: Vec<serde_json::Value>,
}
```

`hints` is always `[]` this release (see Non-goals). `run_file`/
`run_heading`/`run_tag` keep their existing text-mode `println!` path
unchanged when `json` is `false`; under `json: true` they build a
`RenameReport` and print it via `serde_json::to_string_pretty` instead —
same shape `apply::run`'s `json` branch already follows for a single
report. `op`/`summary`/`files_touched` reuse exactly what each function
already computes (`rename_file_at`'s `touched` count, the same `old`/`new`
or `old`/`new`-in-`file` values used in today's `println!`) — no new
computation, just a second serialization path for data already in hand.

### `knap index --json` (US-D27)

`src/index::{IndexReport, NoteSummary}` are unchanged — hints wrap them from
`src/cli/index.rs` rather than becoming a field on either type, keeping
"what an agent should do next" out of the Note Index's own report contract:

```rust
#[derive(Serialize)]
struct IndexOutput<T: Serialize> {
    #[serde(flatten)]
    report: T,
    hints: Vec<serde_json::Value>,
}
```

`#[serde(flatten)]` keeps today's top-level shape intact (`notes`/`tags` for
a directory target, `path`/`title`/`headings`/`links`/`backlinks`/`tags` for
a file target) with `hints` added as a sibling key — purely additive, no
existing field renamed or nested.

```rust
fn has_broken_link(links: &[index::LinkSummary]) -> bool {
    links.iter().any(|l| l.resolved.is_none())
}
```

Used in `run`:

- File target: `hints` is `[hints::lint("broken_links_present")]` if
  `has_broken_link(&summary.links)`, else `[]`.
- Directory target: `hints` is `[hints::lint("broken_links_present")]` if any
  `note.links` in `report.notes` has a broken link, else `[]`.

---

## Testing

### Unit tests

| Test                                                                 | What it verifies                                                                        |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `hints_none_clean_has_action_none_reason_clean`                        | `hints::none_clean()` serializes to `{"action":"none","reason":"clean"}`                    |
| `hints_lint_carries_given_reason`                                      | `hints::lint("confirm_clean")` has `action == "lint"`, `reason == "confirm_clean"`          |
| `hints_manual_carries_given_reason`                                    | `hints::manual("missing-frontmatter")` has `action == "manual"`, `reason == "missing-frontmatter"` |
| `hints_disambiguate_carries_given_reason`                              | `hints::disambiguate("text_mismatch")` has `action == "disambiguate"`, `reason == "text_mismatch"` |
| `hints_apply_repoint_link_carries_target`                              | `hints::apply_repoint_link("a.md")` has `action == "apply"`, `op == "repoint-link"`, `target == "a.md"` |
| `hints_apply_repoint_anchor_carries_anchor`                            | `hints::apply_repoint_anchor("#slug")` has `op == "repoint-anchor"`, `anchor == "#slug"`     |
| `lint_report_hints_is_none_clean_when_zero_diagnostics`                | `knap lint`'s `LintReport.hints` on an empty vault is `[{"action":"none","reason":"clean"}]` |
| `lint_report_hints_is_empty_when_diagnostics_present`                  | `LintReport.hints` is `[]` when `problem_count > 0`                                          |
| `attach_hint_broken_link_no_suggest_hints_rerun_with_suggest`          | `attach_hint` with `suggest_n == 0` on a `broken-link` diagnostic (no `data`) sets `data.hint == {"action":"lint","reason":"rerun_with_suggest"}` |
| `attach_hint_broken_link_suggest_zero_candidates_hints_disambiguate`   | `attach_hint` with `suggest_n > 0` but `data == None` sets `data.hint == {"action":"disambiguate","reason":"no_candidates"}` |
| `attach_hint_broken_link_unambiguous_candidate_hints_apply`            | `attach_hint` on a `broken-link` diagnostic with one ranked `suggestions[0]` and no `text_mismatch` sets `data.hint` to `apply_repoint_link` with that target |
| `attach_hint_broken_anchor_unambiguous_candidate_hints_apply`          | Same as above for `broken-anchor`, using `apply_repoint_anchor`                              |
| `attach_hint_text_mismatch_hints_disambiguate`                         | `attach_hint` on a diagnostic whose `data.text_mismatch == true` sets `data.hint == {"action":"disambiguate","reason":"text_mismatch"}`, regardless of `suggestions` content |
| `attach_hint_frontmatter_codes_hint_manual_with_code_as_reason`        | Each of the four frontmatter `CODE_*` values produces `data.hint == {"action":"manual","reason":"<that code>"}` |
| `attach_hint_unrecognized_code_leaves_data_untouched`                  | A diagnostic with an unrecognized `code` (forward-compat case) has its `data` unchanged by `attach_hint` |
| `apply_report_hints_confirm_clean_when_not_dry_run`                    | `ApplyReport.hints` is `[{"action":"lint","reason":"confirm_clean"}]` after a non-dry-run batch |
| `apply_report_hints_empty_under_dry_run`                               | `ApplyReport.hints` is `[]` under `--dry-run`                                                |
| `apply_one_repoint_link_error_message_names_lint_suggest_rerun`        | The error from `apply_one_repoint_link_rejects_range_that_eats_closing_paren`'s existing scenario now contains "re-fetch range from a fresh `knap lint --suggest --json`" |
| `apply_one_repoint_anchor_error_message_names_lint_suggest_rerun`      | Same, for the `repoint-anchor` round-trip rejection                                          |
| `rename_report_json_serializes_op_summary_files_touched_empty_hints`   | A `RenameReport` for each of the three ops serializes with the right `op` string and `hints: []` |
| `run_file_json_true_prints_json_not_text`                              | `rename::run_file` with `json: true` (captured stdout) parses as JSON with `op == "rename-file"`, and prints no plain-text line |
| `run_file_json_false_preserves_existing_text_output`                   | `rename::run_file` with `json: false` still prints today's `"{old} → {new} (N file(s) touched)"` line, byte-for-byte unchanged |
| `index_output_hints_broken_links_present_for_file_target`              | `IndexOutput` for a single file whose `links` includes one `resolved: None` entry has `hints == [{"action":"lint","reason":"broken_links_present"}]` |
| `index_output_hints_empty_for_file_target_all_resolved`                | Same, all links resolved → `hints == []`                                                     |
| `index_output_hints_broken_links_present_for_directory_target`         | `IndexOutput` for a directory target with at least one note having a broken link has the same hint |
| `index_output_top_level_shape_unchanged_by_flatten`                    | `IndexOutput<NoteSummary>` and `IndexOutput<IndexReport>` still serialize `path`/`notes`/etc. at the top level (via `#[serde(flatten)]`), not nested under a `report` key |

### Integration tests (`tests/cli.rs`)

| Test                                                    | What it verifies                                                                                       |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `lint_json_clean_workspace_has_none_clean_hint`             | Real `knap lint --json` subprocess on a workspace with no problems has the `none`/`clean` report-level hint  |
| `lint_json_suggest_broken_link_has_apply_hint`               | Real `knap lint --json --suggest` subprocess on a fixture with one unambiguous broken link has `data.hint.action == "apply"` on that diagnostic |
| `apply_json_non_dry_run_has_confirm_clean_hint`              | Real `knap apply --json` subprocess applying a batch has `hints == [{"action":"lint","reason":"confirm_clean"}]` |
| `apply_json_dry_run_has_empty_hints`                         | Real `knap apply --dry-run --json` subprocess has `hints == []`                                              |
| `apply_stale_range_error_text_mentions_lint_suggest`         | Real `knap apply` subprocess given a stale `range` fails with stderr containing the re-fetch-range hint text |
| `rename_file_json_flag_emits_valid_json`                     | Real `knap rename-file --json <old> <new>` subprocess stdout parses as JSON with `op == "rename-file"`       |
| `index_json_file_target_broken_link_has_lint_hint`           | Real `knap index <file> --json` subprocess against a fixture with one broken link has the `broken_links_present` hint |

> **Manual checkpoint:** No editor checkpoint — every change here is CLI
> `--json`/error-text output shaping, unreachable from an LSP client;
> covered entirely by unit and integration tests.
