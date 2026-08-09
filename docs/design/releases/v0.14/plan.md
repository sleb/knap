# v0.14 Implementation Plan

Describes the order in which changes are made, what is tested after each
step, and the checkpoints where the CLI should be manually verified against a
real vault.

The guiding principle: each step produces something testable. No step lays
down untested code for the next step to build on.

---

## Status

| Step                                             | Status | Notes |
| ------------------------------------------------ | ------ | ----- |
| 1 — Root-parameterized rename core               | Done   |       |
| 2 — Shared fix target-setup                      | Done   |       |
| 3 — Batch operation model (`ChangeOp`)           | Done   |       |
| 4 — Scratch workspace machinery                  | Done   |       |
| 5 — `apply_one` dispatch and `knap apply` wiring | Done   |       |
| 6 — Integration tests                            | Todo   |       |

---

## Step 1 — Root-parameterized rename core

`knap apply` needs to run `rename-file`/`rename-heading`/`rename-tag`'s
computation against a scratch copy of the workspace, once per matching
operation in a batch — not against the process's actual `cwd`, which is what
these three do today. This has to land before anything else touches them, and
it changes nothing observable yet, so it comes first.

This step uses TDD:

1. Write all unit tests for this step first — stub `rename_file_at`,
   `rename_heading_at`, `rename_tag_at` (e.g. `todo!()` bodies) so the crate
   compiles.
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement by extracting each function's existing body out of
   `run_file`/`run_heading`/`run_tag` (see design doc for the exact split),
   then run `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/cli/rename.rs`: `pub(crate) fn rename_file_at(root: &Path, old: &Path, new: &Path) -> anyhow::Result<usize>`, extracted from `run_file`
- `src/cli/rename.rs`: `pub(crate) fn rename_heading_at(root: &Path, file: &Path, old: &str, new: &str) -> anyhow::Result<usize>`, extracted from `run_heading`
- `src/cli/rename.rs`: `pub(crate) fn rename_tag_at(root: &Path, old: &str, new: &str) -> anyhow::Result<usize>`, extracted from `run_tag`
- `run_file`/`run_heading`/`run_tag` become thin wrappers: compute `cwd`, call the matching `_at` function, print the existing message

**Unit tests:**

| Test                                             | What it verifies                                                            |
| ------------------------------------------------ | --------------------------------------------------------------------------- |
| `rename_file_at_scopes_to_given_root_not_cwd`    | Succeeds against a tempdir root while the process's actual cwd is unrelated |
| `rename_file_at_new_path_exists_errors`          | Existence check operates against `root`-joined paths                        |
| `rename_heading_at_scopes_to_given_root_not_cwd` | Rewrites heading text and anchor links correctly with `root` ≠ cwd          |
| `rename_tag_at_scopes_to_given_root_not_cwd`     | Rewrites frontmatter tags correctly with `root` ≠ cwd                       |

> **Manual checkpoint:** none — pure refactor, no observable change yet.
> Verified by `cargo test`: every existing `rename_file_*`/`rename_heading_*`/
> `rename_tag_*` integration test in `tests/cli.rs` must still pass
> unmodified.

---

## Step 2 — Shared fix target-setup

Extracts the `(idx, config, targets)` setup `fix::run` already does into a
function `knap apply`'s `fix` operation can call directly, without
duplicating `fix::run`'s file-vs-directory branch. `plan_fixes`/`apply`
(the fix-selection and merge-and-write logic) are already `pub(crate)` and
already take an explicit `idx`/`config` — only the setup needs extracting.

This step uses TDD:

1. Write the unit tests below first, with `targets_for` stubbed to compile.
2. Run `cargo test` and confirm they **fail**.
3. Implement, then run `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/cli/fix.rs`: `pub(crate) fn targets_for(path_abs: &Path) -> anyhow::Result<(NoteIndex, Config, Vec<PathBuf>)>`
- `fix::run` calls `targets_for` instead of inlining the setup

**Unit tests:**

| Test                                           | What it verifies                                        |
| ---------------------------------------------- | ------------------------------------------------------- |
| `targets_for_file_path_returns_single_target`  | A file path → `targets` is exactly `[path_abs]`         |
| `targets_for_directory_path_returns_all_notes` | A directory path → `targets` is every note in the index |

> **Manual checkpoint:** none — pure refactor. Verified by `cargo test`:
> every existing `fix_*` integration test must still pass unmodified.

---

## Step 3 — Batch operation model (`ChangeOp`)

Defines the wire format `knap apply` reads from stdin, independent of the
execution machinery in Step 4 — this can be fully tested by deserializing
JSON strings, no filesystem involved.

This step uses TDD:

1. Write all unit tests for this step first, with `ChangeOp` and
   `default_fix_path` stubbed to compile (an empty enum body won't compile,
   so start from the real enum shape but leave `apply_one`/`run`
   unimplemented — this step only needs `ChangeOp` to exist and derive
   `Deserialize`).
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement `ChangeOp`'s `#[serde(tag = "op", ...)]` shape and `kind()`
   until they pass, then run `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/cli/apply.rs` (new file): `enum ChangeOp` with `RenameFile`/`RenameHeading`/`RenameTag`/`Fix` variants, `#[serde(tag = "op", rename_all = "kebab-case")]`
- `fn default_fix_path() -> PathBuf` (the `#[serde(default = ...)]` target for `Fix.path`)
- `impl ChangeOp { fn kind(&self) -> &'static str }`
- `mod apply;` added to `src/cli/mod.rs` (no `Commands::Apply` variant yet — that's Step 5)

**Unit tests:**

| Test                                      | What it verifies                                                          |
| ----------------------------------------- | ------------------------------------------------------------------------- |
| `change_op_deserializes_rename_file`      | `{"op":"rename-file","old":"a.md","new":"b.md"}` → `ChangeOp::RenameFile` |
| `change_op_deserializes_rename_heading`   | `{"op":"rename-heading",...}` → `ChangeOp::RenameHeading`                 |
| `change_op_deserializes_rename_tag`       | `{"op":"rename-tag",...}` → `ChangeOp::RenameTag`                         |
| `change_op_deserializes_fix_default_path` | `{"op":"fix"}` (no `path`) → `ChangeOp::Fix { path: "." }`                |
| `change_op_unknown_op_errors`             | `{"op":"delete-everything"}` → deserialization error, not a panic         |

> **Manual checkpoint:** none — no CLI surface yet. Verified purely by
> `cargo test`.

---

## Step 4 — Scratch workspace machinery

The all-or-nothing guarantee: build a scratch copy of the workspace, apply
every operation against it, and only sync back to the real workspace if
everything succeeded. This step builds and tests that machinery in isolation
— against plain tempdirs, no `ChangeOp`/CLI involved — before Step 5 wires it
to real operations.

This step uses TDD:

1. Write all unit tests for this step first, with `copy_tree`,
   `diff_and_sync`, `relative_files`, and `ensure_scoped` stubbed
   (`todo!()`) so the crate compiles.
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement until they pass, then run `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/cli/apply.rs`: `fn copy_tree(src: &Path, dst: &Path) -> anyhow::Result<()>`
- `src/cli/apply.rs`: `fn relative_files(root: &Path) -> anyhow::Result<HashSet<PathBuf>>`
- `src/cli/apply.rs`: `fn diff_and_sync(src: &Path, dst: &Path, commit: bool) -> anyhow::Result<usize>`
- `src/cli/apply.rs`: `fn ensure_scoped(root: &Path, path: &Path) -> anyhow::Result<()>`
- `Cargo.toml`: `tempfile` moves from `[dev-dependencies]` to `[dependencies]`

**Unit tests:**

| Test                                             | What it verifies                                                                                |
| ------------------------------------------------ | ----------------------------------------------------------------------------------------------- |
| `copy_tree_copies_files_and_skips_hidden_dirs`   | A tree with a `.git` subdirectory → files copied, `.git` absent from the copy                   |
| `diff_and_sync_counts_and_copies_new_file`       | A file only in `src` → counted, and (when `commit`) created in `dst`                            |
| `diff_and_sync_counts_and_copies_changed_file`   | Differing content on a shared path → counted, and (when `commit`) `dst` matches `src` afterward |
| `diff_and_sync_removes_file_absent_from_src`     | A file only in `dst` → counted, and (when `commit`) removed                                     |
| `diff_and_sync_dry_run_counts_without_writing`   | `commit: false` → same count as `commit: true`, but `dst` unchanged afterward                   |
| `ensure_scoped_accepts_relative_path_under_root` | A plain relative path resolves under `root` without error                                       |
| `ensure_scoped_rejects_path_outside_root`        | An absolute path outside `root` → `Err`                                                         |

> **Manual checkpoint:** none — no CLI surface yet. Verified purely by
> `cargo test`.

---

## Step 5 — `apply_one` dispatch and `knap apply` wiring

Wires Steps 1–4 together into the actual `knap apply` subcommand: `apply_one`
dispatches a `ChangeOp` to the matching `_at`/`targets_for` call from the
scratch root, and `run` orchestrates the read-stdin → copy → apply-in-order →
sync-or-discard → report sequence.

This step uses TDD for `apply_one`'s dispatch (independently testable against
a tempdir, without going through stdin); `run`'s stdin/output plumbing has no
useful unit-test seam of its own and is covered end-to-end in Step 6.

1. Write `apply_one`'s unit tests first, stubbing `apply_one` to compile.
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement `apply_one` and `run`, wire `Commands::Apply` into
   `src/cli/mod.rs`, then run `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/cli/apply.rs`: `struct AppliedOp { op: &'static str, summary: String, files_touched: usize }` (`#[derive(serde::Serialize)]`)
- `src/cli/apply.rs`: `fn apply_one(root: &Path, op: &ChangeOp) -> anyhow::Result<AppliedOp>`
- `src/cli/apply.rs`: `struct ApplyReport { dry_run: bool, operations: Vec<AppliedOp>, files_touched: usize }`
- `src/cli/apply.rs`: `pub fn run(dry_run: bool, json: bool) -> anyhow::Result<()>`
- `src/cli/mod.rs`: `Commands::Apply { dry_run: bool, json: bool }` variant and dispatch arm

**Unit tests:**

| Test                                                      | What it verifies                                                                                                          |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `apply_one_rename_file_reports_summary_and_touched_count` | `ChangeOp::RenameFile` against a tempdir root → correct `summary`/`files_touched`                                         |
| `apply_one_fix_reports_no_safe_fixes_found_when_clean`    | `ChangeOp::Fix` against a vault with no broken links/anchors → `summary` is `"no safe fixes found"`, `files_touched` is 0 |
| `apply_one_rejects_rename_file_old_outside_root`          | `ChangeOp::RenameFile { old: <absolute path outside root>, .. }` → `Err`, `ensure_scoped`'s rejection surfaced            |

> **Manual checkpoint:** In a scratch vault, run
> `echo '[{"op":"rename-tag","old":"wip","new":"draft"}]' | knap apply`;
> confirm the tag is rewritten and the summary line prints. Run the same
> command again with `--dry-run` on a fresh copy; confirm the summary line
> prints `would apply` and the vault is unchanged.

---

## Step 6 — Integration tests

End-to-end tests over the full CLI process, covering sequencing,
all-or-nothing rollback, `--dry-run`, and `--json`. Always the last step —
everything it exercises was already unit-tested in isolation in Steps 1–5.

**Deliverables:**

- `tests/cli.rs`: a stdin-piping helper alongside the existing `knap()`/`copy_fixture()` helpers
- `tests/fixtures/apply_batch/` (new fixture): a note with a heading and a cross-file link, set up so one file-rename and one heading-rename in the same batch compose
- All tests below added to `tests/cli.rs`
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                                     | What it verifies                                                                                                                               |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `apply_runs_rename_file_then_rename_heading_in_sequence` | A batch renaming a file, then renaming a heading _in that renamed file_ — proves later operations see earlier operations' effects              |
| `apply_mixed_batch_rename_tag_and_fix`                   | A batch with one `rename-tag` and one `fix` op → both effects present on disk afterward                                                        |
| `apply_all_or_nothing_rolls_back_on_failure`             | A batch whose second operation fails (`rename-file` to an already-existing path) → non-zero exit, real workspace byte-for-byte unchanged       |
| `apply_dry_run_reports_plan_without_touching_disk`       | `--dry-run` on a multi-op batch → stdout lists every planned operation with `would apply`, fixture directory unchanged                         |
| `apply_json_output_shape`                                | `--json` output parses as the `ApplyReport` shape; `operations` has one entry per input op, `files_touched` matches the non-JSON total         |
| `apply_invalid_json_input_errors`                        | Malformed JSON on stdin → non-zero exit, fixture directory unchanged                                                                           |
| `apply_rejects_path_outside_workspace_root`              | An operation with an absolute path outside the fixture root → non-zero exit, no file created at that outside path, fixture directory unchanged |
| `apply_empty_batch_is_a_noop`                            | `[]` on stdin → exit 0, `0 operation(s)` in output, fixture directory unchanged                                                                |

> **Manual checkpoint (full session):** In a scratch vault with a broken
> link, an ambiguous-free broken anchor, and a tag used in two files, pipe a
> batch combining `rename-tag` and `fix` into `knap apply --json`; confirm
> the JSON summary's `operations` array has two entries and `files_touched`
> matches what `knap lint` reports clean afterward. Separately, construct a
> batch whose second operation is guaranteed to fail (e.g. a `rename-file` to
> a path that already exists) and confirm the vault is untouched afterward —
> `git status` (in a vault under git) shows no changes at all.

---

## Done — v0.14 complete

| Story  | Feature                                                                               | Delivered in step               |
| ------ | ------------------------------------------------------------------------------------- | ------------------------------- |
| US-D18 | `knap apply --json` — batch-apply change operations, all-or-nothing, with `--dry-run` | Step 6 (built across Steps 1–5) |
