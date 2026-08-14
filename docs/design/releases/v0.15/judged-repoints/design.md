# v0.15 Design — Judged Repoints

Covers the stories in the v0.15 release:

| Story  | Feature                                                                                                                                                                                           |
| ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| US-D19 | `knap apply` gains `repoint-link`/`repoint-anchor` operations — apply an agent-picked target from `lint --suggest`'s candidates at a diagnostic's own range, inside the same all-or-nothing batch |

---

## Goal

An agent that ran `knap lint --suggest --json` and picked the right candidate
for a `broken-link`/`broken-anchor` diagnostic can now include that pick as
one more entry in the same `knap apply` batch as any `rename-*`/`fix`
operations, instead of stepping outside `apply` for a raw hand `Edit` call.
`file` and `range` are exactly the diagnostic's own `path`/`range` fields, so
the agent never re-locates the edit — it copies two fields off the
diagnostic it already has and adds the target it picked.

This closes the gap the [agentic efficiency
benchmark](../../../experiments/agentic-efficiency-benchmark.md)'s Trial 3
surfaced: `lint --fix`'s auto-apply step ranks repoint candidates by raw edit
distance with no semantic signal, and reports what it applied as a fully
resolved diagnostic rather than something to double-check — it picked a
plausible-but-wrong link target in 1 of 4 knap-assisted runs, and fell back
to creating a spurious stub in roughly half the runs, both only caught by an
agent reading the diff afterward. [Opportunity for improvement
1](../../../experiments/agentic-efficiency-benchmark.md#1-drop---fix-from-the-skills-default-loop--the-false-positive-risk-outweighs-the-tool-call-savings)
proposes dropping `--fix` from the skill's default hand-edit loop and
requiring a hand decision from `data.suggestions` for every `broken-link`/
`broken-anchor` diagnostic — the same posture the skill already takes for the
four frontmatter codes, which never auto-fix. That proposal's own tracked
trade-off is that it gives back part of Trial 3's tool-call/token win, since
the unambiguous cases move from one `--fix` call back to a `lint --suggest`
call plus a separate hand `Edit` per diagnostic. `repoint-link`/
`repoint-anchor` recover that collapse: every `broken-link`/`broken-anchor`
diagnostic, ambiguous or not, becomes one more entry in the same `knap apply`
batch as the task's `rename-*` calls — one `apply` call still applies
everything, but every repoint is now a decision the agent made and can see
in the batch it sent, not a hidden auto-pick it has to audit after the fact.

No new LSP capability, no parser/index change. `compute_link_fix`/
`compute_anchor_fix` (`src/handlers.rs`) already build the exact
`WorkspaceEdit` this needs — they're what `knap fix`'s unambiguous-only path
already calls internally. This release's work is exposing them as two new
`ChangeOp` variants and updating the skill's documented loop to use them.

---

## CLI Changes

### `repoint-link`/`repoint-anchor` operations (`src/cli/apply.rs`)

Two new `ChangeOp` variants, deserializing the diagnostic's own `path`/
`range` fields verbatim (renamed to `file`/`range` to match the other
operations' field naming) plus the one field the agent contributes — its
picked target:

```rust
enum ChangeOp {
    RenameFile { old: PathBuf, new: PathBuf },
    RenameHeading { file: PathBuf, old: String, new: String },
    RenameTag { old: String, new: String },
    Fix { #[serde(default = "default_fix_path")] path: PathBuf },
    RepointLink {
        file: PathBuf,
        range: Range,
        target: String,
    },
    RepointAnchor {
        file: PathBuf,
        range: Range,
        anchor: String,
    },
}
```

`range: lsp_types::Range` derives `Deserialize` already (it's what every
diagnostic's own `range` field serializes as), so a `broken-link`/
`broken-anchor` diagnostic's `range` — or a `broken-link` suggestion's
`target` — drops straight into the batch entry with no reshaping. `target`
matches a `broken-link` suggestion's `target` field exactly (a path relative
to the linking note, e.g. `"notes/mission.md"`). `anchor` accepts a
`broken-anchor` suggestion's `target` field as-is (`"#slug"`, the leading `#`
included, matching how the skill already tells agents to read it) as well as
a bare slug (`"slug"`) — `apply_one` strips a leading `#` before use, so
copying the suggestion's `target` field verbatim works for both `repoint-
link` and `repoint-anchor` without the agent needing to know which fix code
expects which shape.

`apply_one` dispatches both the same way `RenameFile`/`RenameHeading`
already do — scope-check the path, resolve it against the batch's scratch
root, then call the existing `compute_*_fix` builder and apply the resulting
edit directly (no merge step needed: both builders return a `changes`-shaped
`WorkspaceEdit`, never `document_changes`, so there's nothing to combine):

```rust
ChangeOp::RepointLink { file, range, target } => {
    ensure_scoped(root, file)?;
    let file_abs = index::normalize_path(&root.join(file));
    let files_touched = edit::apply(&handlers::compute_link_fix(&file_abs, *range, target))?;
    Ok(AppliedOp {
        op: op.kind(),
        summary: format!("{}: repoint → '{target}'", file.display()),
        files_touched,
    })
}
ChangeOp::RepointAnchor { file, range, anchor } => {
    ensure_scoped(root, file)?;
    let file_abs = index::normalize_path(&root.join(file));
    let anchor = anchor.strip_prefix('#').unwrap_or(anchor);
    let files_touched = edit::apply(&handlers::compute_anchor_fix(&file_abs, *range, anchor))?;
    Ok(AppliedOp {
        op: op.kind(),
        summary: format!("{}: anchor → '#{anchor}'", file.display()),
        files_touched,
    })
}
```

`ChangeOp::kind()` gains the two matching arms (`"repoint-link"`,
`"repoint-anchor"`), and the module gains `use crate::{edit, handlers};` and
`use lsp_types::Range;`. Both variants reuse `ensure_scoped` the same way
`RenameFile`/`Fix` already do — a `file` that resolves outside the scratch
root (an absolute path escaping the workspace) is rejected before any edit
is attempted, same guarantee the existing operations already give.

`compute_link_fix`/`compute_anchor_fix` don't validate that `target`/
`anchor` actually resolves to something real in the vault — same posture
`rename-*` already takes toward its own arguments. The agent supplied the
target from either a `lint --suggest` candidate or its own judgment; `knap
apply` applies it and a following `knap lint --json` (the loop's mandatory
last step, unchanged) is what catches a bad pick, same as it would for a
hand `Edit` outside the batch.

No change to `Fix`, `RenameFile`, `RenameHeading`, or `RenameTag` — the
scratch-copy/`diff_and_sync`/all-or-nothing machinery from v0.14 is reused
as-is.

---

## Skill Changes

### `skill/knap/SKILL.md`: drop `--fix` from the default hand-edit loop

Per [Opportunity 1](../../../experiments/agentic-efficiency-benchmark.md#1-drop---fix-from-the-skills-default-loop--the-false-positive-risk-outweighs-the-tool-call-savings),
step 2 of the "Hand-edits" loop changes from:

```
2. `knap lint --fix --suggest --json`. One call does three things: apply
   every unambiguous fix `knap fix` would make, then report what's left —
   ...
```

to:

```
2. `knap lint --suggest --json` (read-only — no `--fix`). Add `--since
   <git-ref>` to narrow the report to files changed since a commit.
```

Step 3's resolution table drops the "already applied by the `--fix` pass
above" carve-out for `broken-link`/`broken-anchor` — every diagnostic of
either code now gets a hand decision, unambiguous or not:

| `code`          | Meaning                                         | Resolution                                                                       |
| --------------- | ----------------------------------------------- | -------------------------------------------------------------------------------- |
| `broken-link`   | Link target file doesn't exist                  | Pick from `data.suggestions` (or override) → `repoint-link` in the apply batch   |
| `broken-anchor` | `#slug` doesn't match any heading in the target | Pick from `data.suggestions` (or override) → `repoint-anchor` in the apply batch |

The four frontmatter codes are unchanged — they already required a hand
decision and never went through `--fix`. The loop's closing instruction
("chain `rename-*` calls with no lint in between, verify once at the end")
extends the same way: a task's `repoint-link`/`repoint-anchor` picks ride in
the same `knap apply` batch as any `rename-*` calls already in flight, and
the loop still finishes with the one mandatory `knap lint --json` — unchanged
— as the check that actually backs a "no broken links left" claim.

The `--fix`/`fixes_applied` example section is replaced with a `repoint-*`
example showing a full `lint --suggest` → pick → `apply` round trip, so the
skill demonstrates the intended replacement shape rather than just describing
it in prose. The frontmatter `description` field's "verify every edit with
`knap lint --fix --suggest`" line drops `--fix` to match.

`knap fix` and `knap apply`'s `fix` operation are unchanged and still exist —
this only changes what the skill's _default_ loop reaches for first. An
agent that already knows it wants the unambiguous-only auto-apply behavior
can still ask for it explicitly; the skill just stops recommending it by
default, per Opportunity 1's proposed change.

---

## Testing

### Unit tests (`src/cli/apply.rs`)

| Test                                                           | What it verifies                                                                              |
| -------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `change_op_deserializes_repoint_link`                          | `{"op":"repoint-link","file":...,"range":...,"target":...}` → `ChangeOp::RepointLink`         |
| `change_op_deserializes_repoint_anchor`                        | `{"op":"repoint-anchor","file":...,"range":...,"anchor":"#slug"}` → `ChangeOp::RepointAnchor` |
| `apply_one_repoint_link_replaces_target_range_with_new_target` | The link text at `range` in `file` becomes `target`; `files_touched == 1`                     |
| `apply_one_repoint_anchor_replaces_range_with_bare_slug`       | Anchor `#old` at `range` becomes `new` (no leading `#` in the written text)                   |
| `apply_one_repoint_anchor_strips_leading_hash_from_input`      | Passing `anchor: "#new"` writes `new`, not `#new` — same result as passing `anchor: "new"`    |
| `apply_one_rejects_repoint_link_file_outside_root`             | `file` resolving outside `root` errors before any edit is attempted                           |
| `apply_one_rejects_repoint_anchor_file_outside_root`           | Same as above, for `RepointAnchor`                                                            |

### Integration tests (`tests/cli.rs`)

| Test                                                        | What it verifies                                                                                                                                                   |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `apply_batch_repoints_broken_link_at_diagnostic_range`      | A `repoint-link` entry, given the same `file`/`range` `knap lint --suggest --json` reports for a seeded broken link, resolves it                                   |
| `apply_batch_repoints_broken_anchor_at_diagnostic_range`    | Same, for a seeded broken anchor and a `repoint-anchor` entry                                                                                                      |
| `apply_batch_mixes_rename_file_and_repoint_link_atomically` | A batch with both a `rename-file` and a `repoint-link` entry applies both or neither, matching the existing `apply_all_or_nothing_rolls_back_on_failure` guarantee |
| `apply_repoint_link_rejects_path_outside_workspace_root`    | Mirrors `apply_rejects_path_outside_workspace_root` for the new operation                                                                                          |
