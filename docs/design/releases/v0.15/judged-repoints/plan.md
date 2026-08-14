# v0.15 Implementation Plan

Describes the order in which changes are made, what is tested after each
step, and the checkpoints where the CLI should be manually verified against a
real vault.

The guiding principle: each step produces something testable. No step lays
down untested code for the next step to build on.

---

## Status

| Step                                           | Status | Notes |
| ---------------------------------------------- | ------ | ----- |
| 1 — `repoint-link`/`repoint-anchor` operations | Done   |       |
| 2 — Integration tests                          | Done   |       |
| 3 — Skill doc update                           | Done   |       |

---

## Step 1 — `repoint-link`/`repoint-anchor` operations

Adds the two new `ChangeOp` variants and their `apply_one` dispatch arms.
Both call existing, already-tested builders (`handlers::compute_link_fix`/
`compute_anchor_fix`, unchanged since v0.4/v0.13) and the existing
`edit::apply`, so nothing below this step is new — it's wiring, and it's the
only piece a fixed skill loop actually depends on, so it comes first.

This step uses TDD:

1. Write all unit tests for this step first — add the two `ChangeOp` variants
   and stub `apply_one`'s two new match arms (e.g. `unimplemented!()` bodies)
   so the crate compiles.
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement per the design doc's exact split, then run `cargo clippy -- -D
warnings`.

**Deliverables:**

- `src/cli/apply.rs`: `ChangeOp::RepointLink { file: PathBuf, range: Range, target: String }`
- `src/cli/apply.rs`: `ChangeOp::RepointAnchor { file: PathBuf, range: Range, anchor: String }`
- `src/cli/apply.rs`: `ChangeOp::kind()` gains `"repoint-link"`/`"repoint-anchor"` arms
- `src/cli/apply.rs`: `apply_one` gains the two matching dispatch arms (design doc has the exact bodies), plus `use crate::{edit, handlers};` and `use lsp_types::Range;`

**Unit tests:**

| Test                                                           | What it verifies                                                                              |
| -------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `change_op_deserializes_repoint_link`                          | `{"op":"repoint-link","file":...,"range":...,"target":...}` → `ChangeOp::RepointLink`         |
| `change_op_deserializes_repoint_anchor`                        | `{"op":"repoint-anchor","file":...,"range":...,"anchor":"#slug"}` → `ChangeOp::RepointAnchor` |
| `apply_one_repoint_link_replaces_target_range_with_new_target` | The link text at `range` in `file` becomes `target`; `files_touched == 1`                     |
| `apply_one_repoint_anchor_replaces_range_with_bare_slug`       | Anchor `#old` at `range` becomes `new` (no leading `#` in the written text)                   |
| `apply_one_repoint_anchor_strips_leading_hash_from_input`      | Passing `anchor: "#new"` writes `new`, not `#new` — same result as passing `anchor: "new"`    |
| `apply_one_rejects_repoint_link_file_outside_root`             | `file` resolving outside `root` errors before any edit is attempted                           |
| `apply_one_rejects_repoint_anchor_file_outside_root`           | Same as above, for `RepointAnchor`                                                            |

> **Manual checkpoint:** In a scratch vault with a broken link (e.g.
> `[text](missing.md)` where `real.md` exists), run
> `echo '[{"op":"repoint-link","file":"note.md","range":{"start":{"line":0,"character":7},"end":{"line":0,"character":18}},"target":"real.md"}]' | knap apply --json`
> and confirm the file's link now reads `[text](real.md)`. Repeat for a
> broken anchor with `repoint-anchor`.

---

## Step 2 — Integration tests

End-to-end tests over the full CLI, including a mixed batch — proves
`repoint-link`/`repoint-anchor` compose with the existing `rename-*`/`fix`
operations under the same all-or-nothing guarantee, not just standalone.
Always comes after the unit-level dispatch is solid, so a failure here
isolates to composition, not to `apply_one` itself.

**Deliverables:**

- `tests/cli.rs`: the four tests below, following the existing `apply_*`
  tests' fixture/`knap_with_stdin` conventions
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                                        | What it verifies                                                                                                                                                 |
| ----------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apply_batch_repoints_broken_link_at_diagnostic_range`      | A `repoint-link` entry, given the same `file`/`range` `knap lint --suggest --json` reports for a seeded broken link, resolves it                                 |
| `apply_batch_repoints_broken_anchor_at_diagnostic_range`    | Same, for a seeded broken anchor and a `repoint-anchor` entry                                                                                                    |
| `apply_batch_mixes_rename_file_and_repoint_link_atomically` | A batch with both a `rename-file` and a `repoint-link` entry applies both or neither, matching `apply_all_or_nothing_rolls_back_on_failure`'s existing guarantee |
| `apply_repoint_link_rejects_path_outside_workspace_root`    | Mirrors `apply_rejects_path_outside_workspace_root` for the new operation                                                                                        |

> **Manual checkpoint (full session):** In a scratch vault, seed one broken
> link and one broken anchor. Run `knap lint --suggest --json`, copy each
> diagnostic's `path`/`range` and its first `data.suggestions` entry into a
> `repoint-link`/`repoint-anchor` batch entry by hand, pipe the batch through
> `knap apply --json`, then run `knap lint` again and confirm exit code `0`
> — this is the exact hand round trip the updated skill (Step 3) documents.

---

## Step 3 — Skill doc update

Updates `skill/knap/SKILL.md` to drop `--fix` from the default hand-edit loop
and demonstrate the `repoint-*` round trip, per the design doc's Skill
Changes section. Comes last: it's documentation over CLI behavior that Steps
1–2 already prove works, and there's nothing for a subsequent step to build
on top of.

No unit or integration tests — this is a documentation-only change, verified
by manual read-through and by an actual agent session following the updated
loop.

**Deliverables:**

- `skill/knap/SKILL.md`: step 2 of the hand-edit loop drops `--fix`, becomes
  a read-only `knap lint --suggest --json` call
- `skill/knap/SKILL.md`: step 3's resolution table drops the "already
  applied by `--fix`" carve-out for `broken-link`/`broken-anchor`; both rows
  point at `repoint-link`/`repoint-anchor` in the apply batch
- `skill/knap/SKILL.md`: the `--fix`/`fixes_applied` example section is
  replaced with a `lint --suggest` → pick → `apply` example using
  `repoint-link`
- `skill/knap/SKILL.md`: frontmatter `description` field drops `--fix` from
  "verify every edit with `knap lint --fix --suggest`"

> **Manual checkpoint:** Open `skill/knap/SKILL.md` and read the "Hand-edits"
> loop top to bottom as if seeing it for the first time — confirm no
> remaining reference to `--fix` as the default path, and that the
> `repoint-link` example is copy-pasteable against the vault used in Step
> 2's checkpoint.

---

## Done — v0.15 complete

| Story  | Feature                                                                                                                                                                                           | Delivered in step |
| ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------- |
| US-D19 | `knap apply` gains `repoint-link`/`repoint-anchor` operations — apply an agent-picked target from `lint --suggest`'s candidates at a diagnostic's own range, inside the same all-or-nothing batch | Steps 1–2         |
