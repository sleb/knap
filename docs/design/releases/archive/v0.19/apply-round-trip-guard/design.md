# v0.19 Design — Apply Round-Trip Guard

Covers the stories in the v0.19 release:

| Story  | Feature                                                                                              |
| ------ | ---------------------------------------------------------------------------------------------------- |
| US-D21 | `knap apply` rejects a `repoint-link`/`repoint-anchor` op that would write unparseable markdown      |
| US-D15 | `skill/knap/SKILL.md` (amended) — the apply-from-suggestions loop instructs copying `range` verbatim |

Both stories close out the two "proposed" opportunities from
[Trial 6 of the agentic efficiency benchmark](../../../experiments/agentic-efficiency-benchmark.md#opportunities-for-improvement-surfaced-by-trial-6):
a tool-side guardrail (US-D21, opportunity 1) and a skill prompt fix
(US-D15 amendment, opportunity 2). Opportunity 3 (add a malformed-link check
to the benchmark's own scoring pass) is benchmark-harness work, not a knap
change, and is out of scope here.

---

## Goal

An agent driving `knap apply --json` with a `repoint-link`/`repoint-anchor`
operation can trust that if the command exits `0`, the file it touched still
parses as well-formed Markdown — never silently corrupted the way Trial 6's
`knap-assisted-run3` corrupted 5 of 8 broken-anchor fixes into text with no
closing `)`, invisible to `knap lint` because a markdown parser doesn't
recognize `[text](target` as a link at all once it's broken that badly.

These two stories belong together because they attack the same root cause
from both ends: an agent that hand-recomputes a `range` instead of copying it
from the diagnostic (the behavior Trial 6's transcript traced the corruption
to) is the specific mistake US-D15's amendment tells the agent not to make,
and US-D21 is the backstop for every caller who makes that mistake anyway —
including ones that never read the skill at all. US-D21 is the durable fix
(every caller of `knap apply` is protected, not just ones that follow the
skill); the SKILL.md amendment is a cheap, immediate complement that reduces
how often the guardrail has to fire in the first place.

---

## CLI Changes (`src/cli/apply.rs`)

`knap apply` already stages every operation in a batch against a scratch
copy of the workspace (`copy_tree`/`run`) and only syncs the result back to
the real workspace (`diff_and_sync`) once every operation in the batch has
returned `Ok`. That existing structure is what makes the guardrail
batch-atomic for free: rejecting one operation with `Err` already aborts the
whole batch before any real file is touched, and the scratch tempdir is
discarded on drop. No change to that control flow is needed — the new work
is entirely inside `apply_one`.

New private helper:

```rust
/// Re-parses `content` (a file's text *after* a `repoint-link`/
/// `repoint-anchor` edit was written to it) and confirms a well-formed
/// `MarkdownLink` now sits at `expected_line`, satisfying `matches`.
///
/// This exists because a caller-supplied `range` that's off by even one
/// character can eat the link's closing `)` — the resulting text isn't
/// recognized as a link at all by a markdown parser, so `knap lint` (which
/// only ever inspects links a parse *did* produce) can never flag it. This
/// is the only check standing between a bad `range` and a corrupted file
/// left on disk.
fn validate_link_round_trip(
    path: &Path,
    content: &str,
    expected_line: u32,
    matches: impl Fn(&parser::MarkdownLink) -> bool,
) -> anyhow::Result<()> {
    let note = parser::parse(path, content);
    anyhow::ensure!(
        note
            .md_links
            .iter()
            .any(|l| l.range.start.line == expected_line && matches(l)),
        "no well-formed link found at line {} after the edit — the markdown \
         is likely corrupted (e.g. a missing closing ')')",
        expected_line + 1
    );
    Ok(())
}
```

`apply_one`'s `RepointLink` and `RepointAnchor` arms call it right after
`edit::apply` writes the file, before returning `Ok(AppliedOp { .. })`:

```rust
ChangeOp::RepointLink { file, range, target } => {
    ensure_scoped(root, file)?;
    let file_abs = index::normalize_path(&root.join(file));
    let files_touched =
        edit::apply(&handlers::compute_link_fix(&file_abs, *range, target))?;
    let new_content = fs::read_to_string(&file_abs)?;
    validate_link_round_trip(&file_abs, &new_content, range.start.line, |l| {
        l.target == *target
    })
    .context("repoint-link produced unparseable markdown")?;
    Ok(AppliedOp { /* unchanged */ })
}
ChangeOp::RepointAnchor { file, range, anchor } => {
    ensure_scoped(root, file)?;
    let file_abs = index::normalize_path(&root.join(file));
    let anchor = anchor.strip_prefix('#').unwrap_or(anchor);
    let files_touched =
        edit::apply(&handlers::compute_anchor_fix(&file_abs, *range, anchor))?;
    let new_content = fs::read_to_string(&file_abs)?;
    validate_link_round_trip(&file_abs, &new_content, range.start.line, |l| {
        l.anchor.as_deref() == Some(anchor)
    })
    .context("repoint-anchor produced unparseable markdown")?;
    Ok(AppliedOp { /* unchanged */ })
}
```

`run`'s existing `.with_context(|| op.kind().to_string())` around each
`apply_one` call is upgraded to name the operation's position in the batch,
not just its kind — with N operations of the same kind in one batch (a
realistic shape: several `repoint-anchor`s from the same lint pass), "which
one failed" matters as much as "what kind failed":

```rust
for (i, op) in ops.iter().enumerate() {
    let applied = apply_one(scratch.path(), op)
        .with_context(|| format!("operation {} ({})", i + 1, op.kind()))?;
    operations.push(applied);
}
```

So a bad range now fails the whole batch with, e.g.:

```
Error: operation 4 (repoint-anchor)

Caused by:
    0: repoint-anchor produced unparseable markdown
    1: no well-formed link found at line 12 after the edit — the markdown is likely corrupted (e.g. a missing closing ')')
```

instead of writing `projects/notifications.md` with a truncated link and
exiting `0`.

**Scope: only the two range-based ops.** `RenameFile`, `RenameHeading`, and
`RenameTag` don't take a caller-supplied byte `range` at all — their edits
are computed by `handlers::compute_heading_rename`/`compute_tag_rename`/
`handle_will_rename_files` from knap's own index-driven lookup, the same
code path already covered by `rename_file_at_scopes_to_given_root_not_cwd`
and friends in `src/cli/rename.rs`. `RepointLink`/`RepointAnchor` are the
only two ops where the range comes verbatim from the caller (normally a
diagnostic's `range`, but nothing enforces that) — they're also the only
ops Trial 6's failure mode touched, and the only ones this guardrail needs
to cover.

**Why validate post-write instead of before**: `edit::apply` already reads
the file from disk, splices in the edit, and writes it back in one call
(`apply_text_edits`); threading a "compute without writing" variant through
`edit::apply` for this one caller would duplicate that logic for no
observable benefit, since the write already lands only in the scratch copy
and is discarded unread the moment `apply_one` returns `Err` — nothing about
writing first and validating second reaches the real workspace on failure.

**Edge case — escaped targets.** `compute_link_fix` wraps a `new_target`
containing a space or parenthesis in `<...>` (`escape_link_target`) before
writing it; `matches` must compare against what the parser decodes the
written target back to, not the raw wrapped string. Cover this with a unit
test (`repoint_link` with a spaced target) rather than assuming the decoded
form matches — see [Testing](#testing).

---

## Skill Documentation Changes (`skill/knap/SKILL.md`)

The `lint --suggest` → pick → `apply` round-trip example currently says:

> Both operations use the diagnostic's own `range`, so there's no need to
> re-locate the link/anchor text by hand before repointing it.

This states the _shortcut_ `range` offers but never explicitly forbids the
mistake Trial 6's transcript traced the corruption to: the agent read
`range` correctly off 3 of 8 diagnostics, then for the other 5 re-derived
`start`/`end` by hand from a re-read copy of the file and got them wrong by
a consistent `+1`/`+3`. Replace that sentence with an explicit instruction
and a worked contrast:

```markdown
Both operations use the diagnostic's own `range` — copy it byte-for-byte
into the apply operation. **Never re-derive `range`'s `line`/`character`
values by hand, and never recompute them from a separately re-read copy of
the file** — even a small skew (off by one character, a stray `+1`) can
eat part of the link's syntax (its closing `)`, for instance) and leave
behind text a markdown parser no longer recognizes as a link at all, which
`knap lint` then reports as clean because it never sees a link there to
check. `knap apply` rejects an operation that would do this (see the
"produced unparseable markdown" error below) — but the fix is cheap enough
that you shouldn't rely on the rejection catching it: copy `range` as-is.
```

$ echo '[{"op":"repoint-anchor","file":"a.md","range":{"start":{"line":11,"character":35},"end":{"line":11,"character":43}},"anchor":"dashboard-overview"}]' \
| knap apply --json
Error: operation 1 (repoint-anchor)

Caused by:
0: repoint-anchor produced unparseable markdown
1: no well-formed link found at line 12 after the edit — the markdown is likely corrupted (e.g. a missing closing ')')

```

If you see this error, re-fetch the diagnostic's `range` from a fresh
`knap lint --suggest --json` rather than adjusting the numbers you already
have — an adjusted-but-still-guessed range is exactly how this happens in
the first place.
```

This is documentation-only: no change to `SKILL.md`'s frontmatter, trigger
conditions, or any other section.

---

## Testing

### Unit tests (`src/cli/apply.rs`)

| Test                                                                   | What it verifies                                                                                                                                                                              |
| ---------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `validate_link_round_trip_accepts_well_formed_link`                    | Passes when the expected line holds a `MarkdownLink` matching the predicate                                                                                                                   |
| `validate_link_round_trip_rejects_missing_link_at_line`                | Errors when no link at all sits at `expected_line` (simulates a truncated `[text](target` with no closing `)`)                                                                                |
| `validate_link_round_trip_rejects_link_with_wrong_field`               | Errors when a link exists at the line but `matches` returns `false` (right shape, wrong target/anchor)                                                                                        |
| `apply_one_repoint_link_rejects_range_that_eats_closing_paren`         | `RepointLink` with a `range` extending past the target into the closing `)` returns `Err`, file left untouched by the batch                                                                   |
| `apply_one_repoint_anchor_rejects_range_that_eats_closing_paren`       | `RepointAnchor` with an over-wide `range` returns `Err`, mirrors the above for anchors                                                                                                        |
| `apply_one_repoint_link_accepts_target_needing_angle_bracket_escaping` | A `target` containing a space (wrapped in `<...>` by `compute_link_fix`) still validates — the escaping edge case                                                                             |
| `run_aborts_whole_batch_when_one_repoint_op_is_corrupt`                | A batch with a valid `rename-file` followed by a corrupt `repoint-anchor` leaves the real workspace fully untouched — the earlier op's scratch-copy effect is discarded, not partially synced |
| `run_error_names_the_failing_operation_by_position_and_kind`           | The batch error message contains `"operation 2 (repoint-anchor)"` (or matching index/kind) for a failure at that position                                                                     |

### Integration tests (`tests/cli.rs`, alongside the existing `apply_batch_*`/`apply_*` tests)

| Test                                                         | What it verifies                                                                                                                     |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------ |
| `apply_cli_rejects_corrupt_repoint_anchor_and_exits_nonzero` | Running the `knap apply` binary with a bad-range batch on stdin exits non-zero and leaves the target file's original content on disk |
| `apply_cli_reports_which_operation_failed_in_stderr`         | The process's error output names the failing operation's position and kind                                                           |
