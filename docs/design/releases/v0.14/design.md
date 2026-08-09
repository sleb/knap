# v0.14 Design — Batch Apply

Covers the stories in the v0.14 release:

| Story  | Feature                                                                                                                                                                            |
| ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| US-D18 | `knap apply --json` — apply a JSON array of change operations (rename-file, rename-heading, rename-tag, fix) from stdin, sequentially and all-or-nothing, with `--dry-run` support |

---

## Goal

An agent that's just run `knap lint --suggest --json` and picked the right fix
for each finding can now apply the whole batch in one call, instead of
shelling out to `rename-file`/`rename-heading`/`rename-tag`/`fix` once per
change. `knap apply --json` reads a JSON array describing the batch from
stdin, applies each entry in order, and guarantees the workspace ends up
either fully changed or completely untouched — never partway through a
multi-step refactor because operation 4 of 7 hit a conflict.

No new LSP capability, no parser/index/handler change — every one of the four
operation kinds already has a working, tested implementation
(`rename-file`/`rename-heading`/`rename-tag` from v0.12, `fix` from v0.13).
This release's actual work is sequencing them safely: making each operation's
core logic reusable with an explicit workspace root instead of always the
process's own working directory, and adding the scratch-copy machinery that
makes "all-or-nothing" true instead of aspirational.

---

## CLI Changes

### Root-parameterized rename core (`src/cli/rename.rs`)

`run_file`/`run_heading`/`run_tag` each resolve their own scope by calling
`config::for_path` against `std::env::current_dir()` — fine for a single
standalone invocation, but `knap apply` needs to run the same computation
against a scratch copy of the workspace instead of the real one, once per
operation in the batch. Each function is split into a thin CLI wrapper
(unchanged behavior, unchanged printed output) and a `pub(crate)` core that
takes the workspace root explicitly:

```rust
pub fn run_file(old: &Path, new: &Path) -> anyhow::Result<()> {
    let cwd = absolute(Path::new("."))?;
    let touched = rename_file_at(&cwd, old, new)?;
    println!("{} → {} ({touched} file(s) touched)", old.display(), new.display());
    Ok(())
}

/// Core of `rename-file`, scoped to `root` instead of always the process's
/// actual cwd. `old`/`new` are resolved against `root` when relative
/// (`Path::join` leaves an already-absolute argument unchanged). Shared with
/// `knap apply`, which calls this once per `rename-file` entry in a batch,
/// with `root` pointing at that batch's scratch copy of the workspace.
pub(crate) fn rename_file_at(root: &Path, old: &Path, new: &Path) -> anyhow::Result<usize> {
    let old_abs = index::normalize_path(&root.join(old));
    let new_abs = index::normalize_path(&root.join(new));
    anyhow::ensure!(old_abs.exists(), "{}: no such file", old.display());
    anyhow::ensure!(!new_abs.exists(), "{}: already exists", new.display());

    let old_uri = path_to_uri(&old_abs);
    let new_uri = path_to_uri(&new_abs);

    let config = config::for_path(root, None)?;
    let extensions: Vec<&str> = config.extensions.iter().map(String::as_str).collect();
    let (idx, _) = index::build(&config.index_roots, &extensions);

    let params = RenameFilesParams {
        files: vec![FileRename {
            old_uri: old_uri.as_str().to_string(),
            new_uri: new_uri.as_str().to_string(),
        }],
    };
    let link_edit = handlers::handle_will_rename_files(params, &idx);
    let wrapped = wrap_as_document_changes(link_edit, old_uri, new_uri);
    edit::apply(&wrapped)
}
```

`run_heading`/`rename_heading_at` and `run_tag`/`rename_tag_at` split the
same way — `rename_heading_at(root, file, old, new)` resolves `file` against
`root` instead of `absolute(file)`; `rename_tag_at(root, old, new)` takes no
path argument and just swaps `config::for_path(&cwd, ..)` for
`config::for_path(root, ..)`. All three wrappers keep printing exactly what
they print today; this is a pure extraction; `cargo test`'s existing
`rename_file_*`/`rename_heading_*`/`rename_tag_*` integration tests must stay
green unmodified.

### Shared fix target-setup (`src/cli/fix.rs`)

`fix::run` builds `(idx, config, targets)` from a path before calling
`plan_fixes` — the exact same setup `knap apply`'s `fix` operation needs.
`plan_fixes`/`apply` are already `pub(crate)` and already root-independent
(they take an already-built `NoteIndex`/`Config`, never touch
`std::env::current_dir()`), so only the setup needs extracting:

```rust
/// Builds the index and fix targets for `path_abs` (already absolutized): a
/// file path scopes to just that note, a directory scopes to every indexed
/// note — the setup `fix::run` and `knap apply`'s `fix` operation both need
/// before calling `plan_fixes`.
pub(crate) fn targets_for(path_abs: &Path) -> anyhow::Result<(NoteIndex, Config, Vec<PathBuf>)> {
    let config = config::for_path(path_abs, None)?;
    let extensions: Vec<&str> = config.extensions.iter().map(String::as_str).collect();
    let (idx, _) = index::build(&config.index_roots, &extensions);
    let targets: Vec<PathBuf> = if path_abs.is_file() {
        vec![path_abs.to_path_buf()]
    } else {
        idx.all_notes().map(|n| n.path.clone()).collect()
    };
    Ok((idx, config, targets))
}
```

`fix::run` becomes `let (idx, config, targets) = targets_for(&absolute(path)?)?;`
followed by its existing `plan_fixes`/apply-or-preview logic, unchanged.

### `knap apply --dry-run --json` (new subcommand, `src/cli/apply.rs`)

```rust
/// Apply a batch of write operations (rename-file, rename-heading,
/// rename-tag, fix) from a JSON array on stdin, sequentially and
/// all-or-nothing.
Apply {
    /// Preview the batch's effect without touching the real workspace.
    #[arg(long)]
    dry_run: bool,
    /// Emit a machine-readable JSON summary instead of text.
    #[arg(long)]
    json: bool,
},
```

`--json` picks the output shape, matching `lint`/`index`'s existing
convention — stdin is always parsed as JSON, regardless of the flag. There is
no other input format to select.

**Wire format.** Each array entry is tagged by `op`, one variant per existing
mutating subcommand, with the same field names as that subcommand's
arguments:

```rust
#[derive(serde::Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
enum ChangeOp {
    RenameFile { old: PathBuf, new: PathBuf },
    RenameHeading { file: PathBuf, old: String, new: String },
    RenameTag { old: String, new: String },
    Fix {
        #[serde(default = "default_fix_path")]
        path: PathBuf,
    },
}

fn default_fix_path() -> PathBuf {
    PathBuf::from(".")
}

impl ChangeOp {
    /// The `op` tag's wire value, for per-operation error context.
    fn kind(&self) -> &'static str {
        match self {
            ChangeOp::RenameFile { .. } => "rename-file",
            ChangeOp::RenameHeading { .. } => "rename-heading",
            ChangeOp::RenameTag { .. } => "rename-tag",
            ChangeOp::Fix { .. } => "fix",
        }
    }
}
```

An unrecognized `op` value, or a variant missing a required field, is a
`serde_json` deserialization error surfaced via `anyhow::Context` — no custom
validation needed. An empty array is valid: zero operations, zero files
touched, exit 0.

**All-or-nothing via a scratch copy.** Each operation's `_at`/`plan_fixes`
computation needs the _previous_ operation's effect already on disk to
compute correctly (e.g. `rename-heading` on a file `rename-file` just moved),
so operations can't be validated independently and then committed — they
have to actually run in order. To make that safe, the whole batch runs
against a temporary copy of the workspace, and the real workspace is touched
only once, at the very end, and only if every operation succeeded:

```rust
/// `knap apply [--dry-run] [--json]`: reads a JSON array of `ChangeOp` from
/// stdin, applies each one in order against a scratch copy of the workspace,
/// and only then — and only if every operation succeeded — syncs the result
/// back onto the real workspace. If any operation fails, the scratch copy is
/// discarded and the real workspace is never touched.
pub fn run(dry_run: bool, json: bool) -> anyhow::Result<()> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("reading operations from stdin")?;
    let ops: Vec<ChangeOp> = serde_json::from_str(&input).context("parsing operations JSON")?;

    let cwd = absolute(Path::new("."))?;
    let config = config::for_path(&cwd, None)?;
    let real_root = config.index_roots[0].clone();

    let staging = tempfile::tempdir()?;
    copy_tree(&real_root, staging.path())?;

    let mut applied = Vec::with_capacity(ops.len());
    for (i, op) in ops.iter().enumerate() {
        let result = apply_one(staging.path(), op).with_context(|| {
            format!(
                "operation {} ({}) failed — workspace left untouched",
                i + 1,
                op.kind()
            )
        })?;
        applied.push(result);
    }

    let files_touched = diff_and_sync(staging.path(), &real_root, !dry_run)?;

    if json {
        let report = ApplyReport { dry_run, operations: applied, files_touched };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        let verb = if dry_run { "would apply" } else { "applied" };
        for op in &applied {
            println!("{verb} {}: {} ({} file(s) touched)", op.op, op.summary, op.files_touched);
        }
        println!(
            "{} operation(s), {files_touched} file(s) {}",
            ops.len(),
            if dry_run { "would be touched" } else { "touched" },
        );
    }

    Ok(())
}
```

`copy_tree` and `diff_and_sync` both walk in terms of `index::should_skip_dir`
— the same hidden/`node_modules`/`target` exclusion the index already applies
— so the scratch copy never includes `.git`, and mirroring back never
deletes it either:

```rust
/// Recursively copies every file under `src` into `dst` (which must already
/// exist), skipping directories `index::should_skip_dir` would skip during
/// indexing. The batch never needs to see `.git`/`node_modules`/`target`,
/// and skipping them keeps the scratch copy fast even in a vault that's also
/// a large git worktree.
fn copy_tree(src: &Path, dst: &Path) -> anyhow::Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let name = entry.file_name();
            if index::should_skip_dir(&name.to_string_lossy()) {
                continue;
            }
            let dst_dir = dst.join(&name);
            fs::create_dir(&dst_dir)?;
            copy_tree(&entry.path(), &dst_dir)?;
        } else if entry.file_type()?.is_file() {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Compares `src` (the batch's mutated scratch copy) against `dst` (the real
/// workspace root) and returns how many files differ — created, changed, or
/// (left behind by a rename inside the batch) present in `dst` but not
/// `src`. When `commit` is `false` this is read-only: the count is exactly
/// what a real run would touch, but nothing is written — this is what makes
/// `--dry-run` accurate instead of a guess. When `commit` is `true`, also
/// performs the sync: copies every created/changed file from `src` to `dst`,
/// then removes every file `dst` has that `src` no longer does.
fn diff_and_sync(src: &Path, dst: &Path, commit: bool) -> anyhow::Result<usize> {
    let src_files = relative_files(src)?;
    let dst_files = relative_files(dst)?;

    let mut changed = 0;
    for rel in &src_files {
        let (from, to) = (src.join(rel), dst.join(rel));
        if fs::read(&from)? != fs::read(&to).unwrap_or_default() {
            changed += 1;
            if commit {
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&from, &to)?;
            }
        }
    }
    for rel in dst_files.difference(&src_files) {
        changed += 1;
        if commit {
            fs::remove_file(dst.join(rel))?;
        }
    }
    Ok(changed)
}

/// Every file under `root`, as a path relative to `root`, skipping
/// directories `index::should_skip_dir` would skip.
fn relative_files(root: &Path) -> anyhow::Result<HashSet<PathBuf>> { .. }
```

**Dispatch, and a path-escape guard.** `apply_one` maps each `ChangeOp` to
its `_at`/`targets_for` call and a human-readable summary:

```rust
#[derive(serde::Serialize)]
struct AppliedOp {
    op: &'static str,
    summary: String,
    files_touched: usize,
}

fn apply_one(root: &Path, op: &ChangeOp) -> anyhow::Result<AppliedOp> {
    match op {
        ChangeOp::RenameFile { old, new } => {
            ensure_scoped(root, old)?;
            ensure_scoped(root, new)?;
            let files_touched = rename::rename_file_at(root, old, new)?;
            Ok(AppliedOp {
                op: op.kind(),
                summary: format!("{} → {}", old.display(), new.display()),
                files_touched,
            })
        }
        ChangeOp::RenameHeading { file, old, new } => {
            ensure_scoped(root, file)?;
            let files_touched = rename::rename_heading_at(root, file, old, new)?;
            Ok(AppliedOp {
                op: op.kind(),
                summary: format!("{old:?} → {new:?} in {}", file.display()),
                files_touched,
            })
        }
        ChangeOp::RenameTag { old, new } => {
            let files_touched = rename::rename_tag_at(root, old, new)?;
            Ok(AppliedOp { op: op.kind(), summary: format!("#{old} → #{new}"), files_touched })
        }
        ChangeOp::Fix { path } => {
            ensure_scoped(root, path)?;
            let path_abs = index::normalize_path(&root.join(path));
            let (idx, config, targets) = fix::targets_for(&path_abs)?;
            let fixes = fix::plan_fixes(&idx, &config, &targets);
            let files_touched = fix::apply(&fixes)?;
            let summary = if fixes.is_empty() {
                "no safe fixes found".to_string()
            } else {
                fixes.iter().map(|f| f.description.clone()).collect::<Vec<_>>().join("; ")
            };
            Ok(AppliedOp { op: op.kind(), summary, files_touched })
        }
    }
}

/// Resolves `path` against `root` and rejects anything that lands outside
/// it. Without this, an absolute path in the batch JSON — `Path::join`
/// discards `root` when the joined path is already absolute — would resolve
/// against the real filesystem instead of the scratch copy, silently
/// breaking the all-or-nothing guarantee: that one operation would mutate
/// real files even though every other operation in the same batch only ever
/// touches staging.
fn ensure_scoped(root: &Path, path: &Path) -> anyhow::Result<()> {
    let resolved = index::normalize_path(&root.join(path));
    anyhow::ensure!(
        resolved.starts_with(root),
        "{}: resolves outside the workspace root",
        path.display()
    );
    Ok(())
}
```

`RenameTag` has no path field, so it's the one variant `ensure_scoped` never
applies to.

**Output.** Text mode prints one line per operation (`applied rename-file:
sub/old.md → new.md (2 file(s) touched)`, or `would apply …` under
`--dry-run`) followed by a one-line total. `--json` emits:

```rust
#[derive(serde::Serialize)]
struct ApplyReport {
    dry_run: bool,
    operations: Vec<AppliedOp>,
    files_touched: usize,
}
```

**Dependency change.** `tempfile` moves from `[dev-dependencies]` to
`[dependencies]` in `Cargo.toml` — every other use is test-only, but
`cli::apply::run` needs `tempfile::tempdir()` at runtime to build the batch's
scratch copy.

**Edge cases:**

- Operation _N_ fails → error surfaces with `operation N (<kind>) failed`
  context; `diff_and_sync` is never called, so the real workspace is
  byte-for-byte unchanged, regardless of how many earlier operations in the
  batch already ran against the scratch copy.
- An operation's path resolves outside the workspace root (absolute path
  escaping the batch's intended scope) → rejected by `ensure_scoped` before
  the operation runs, same all-or-nothing guarantee as any other failure.
- Empty array (`[]`) → zero operations, `diff_and_sync` finds no
  differences, exit 0 — a no-op batch is success, not an error.
- Malformed JSON on stdin → `serde_json` error surfaced via `anyhow::Context`,
  non-zero exit, nothing touched (the scratch copy is never even created,
  since parsing happens first).
- A later operation in the batch targets a file an earlier operation in the
  _same_ batch renamed away — this just works, since each operation reads
  the scratch copy's current state, not a snapshot taken at batch start.

---

## Testing

### Unit tests

| Test (file)                                                                    | What it verifies                                                                                                          |
| ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| `rename_file_at_scopes_to_given_root_not_cwd` (`src/cli/rename.rs`)            | `rename_file_at` succeeds against a tempdir root while the process's actual cwd is unrelated                              |
| `rename_file_at_new_path_exists_errors` (`src/cli/rename.rs`)                  | Existence check operates against `root`-joined paths, not stale `cwd`-joined ones                                         |
| `rename_heading_at_scopes_to_given_root_not_cwd` (`src/cli/rename.rs`)         | `rename_heading_at` rewrites text and anchors correctly with `root` ≠ cwd                                                 |
| `rename_tag_at_scopes_to_given_root_not_cwd` (`src/cli/rename.rs`)             | `rename_tag_at` rewrites frontmatter tags correctly with `root` ≠ cwd                                                     |
| `targets_for_file_path_returns_single_target` (`src/cli/fix.rs`)               | A file path → `targets` is exactly `[path_abs]`                                                                           |
| `targets_for_directory_path_returns_all_notes` (`src/cli/fix.rs`)              | A directory path → `targets` is every note `idx.all_notes()` returns                                                      |
| `change_op_deserializes_rename_file` (`src/cli/apply.rs`)                      | `{"op":"rename-file","old":"a.md","new":"b.md"}` → `ChangeOp::RenameFile`                                                 |
| `change_op_deserializes_rename_heading` (`src/cli/apply.rs`)                   | `{"op":"rename-heading",...}` → `ChangeOp::RenameHeading`                                                                 |
| `change_op_deserializes_rename_tag` (`src/cli/apply.rs`)                       | `{"op":"rename-tag",...}` → `ChangeOp::RenameTag`                                                                         |
| `change_op_deserializes_fix_default_path` (`src/cli/apply.rs`)                 | `{"op":"fix"}` (no `path`) → `ChangeOp::Fix { path: "." }`                                                                |
| `change_op_unknown_op_errors` (`src/cli/apply.rs`)                             | `{"op":"delete-everything"}` → deserialization error, not a panic                                                         |
| `copy_tree_copies_files_and_skips_hidden_dirs` (`src/cli/apply.rs`)            | A tree with a `.git` subdirectory → files copied, `.git` absent from the copy                                             |
| `diff_and_sync_counts_and_copies_new_file` (`src/cli/apply.rs`)                | A file only in `src` → counted, and (when `commit`) created in `dst`                                                      |
| `diff_and_sync_counts_and_copies_changed_file` (`src/cli/apply.rs`)            | Differing content on a shared path → counted, and (when `commit`) `dst` matches `src` afterward                           |
| `diff_and_sync_removes_file_absent_from_src` (`src/cli/apply.rs`)              | A file only in `dst` (left behind by a rename) → counted, and (when `commit`) removed                                     |
| `diff_and_sync_dry_run_counts_without_writing` (`src/cli/apply.rs`)            | `commit: false` → same count as the `commit: true` case, but `dst` is byte-for-byte unchanged afterward                   |
| `ensure_scoped_accepts_relative_path_under_root` (`src/cli/apply.rs`)          | A plain relative path resolves under `root` without error                                                                 |
| `ensure_scoped_rejects_path_outside_root` (`src/cli/apply.rs`)                 | An absolute path outside `root` → `Err`                                                                                   |
| `apply_one_rename_file_reports_summary_and_touched_count` (`src/cli/apply.rs`) | `ChangeOp::RenameFile` against a tempdir root → correct `summary`/`files_touched`                                         |
| `apply_one_fix_reports_no_safe_fixes_found_when_clean` (`src/cli/apply.rs`)    | `ChangeOp::Fix` against a vault with no broken links/anchors → `summary` is `"no safe fixes found"`, `files_touched` is 0 |
| `apply_one_rejects_rename_file_old_outside_root` (`src/cli/apply.rs`)          | `ChangeOp::RenameFile { old: <absolute path outside root>, .. }` → `Err`, `ensure_scoped`'s rejection surfaced            |

Existing `rename::wrap_as_document_changes` and `fix::plan_fixes`/`fix::apply`
tests must stay green unmodified — the `_at`/`targets_for` extractions are
pure refactors, not behavior changes.

### Integration tests (`tests/cli.rs`)

| Test                                                     | What it verifies                                                                                                                                                                               |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apply_runs_rename_file_then_rename_heading_in_sequence` | A batch renaming a file, then renaming a heading _in that renamed file_ — proves later operations see earlier operations' effects                                                              |
| `apply_mixed_batch_rename_tag_and_fix`                   | A batch with one `rename-tag` and one `fix` op → both effects present on disk afterward                                                                                                        |
| `apply_all_or_nothing_rolls_back_on_failure`             | A batch whose second operation fails (e.g. `rename-file` to an already-existing path) → non-zero exit, real workspace byte-for-byte unchanged, including the first operation's would-be effect |
| `apply_dry_run_reports_plan_without_touching_disk`       | `--dry-run` on a multi-op batch → stdout lists every planned operation with `would apply`, fixture directory unchanged                                                                         |
| `apply_json_output_shape`                                | `--json` output parses as `ApplyReport`; `operations` has one entry per input op, `files_touched` matches the non-JSON total                                                                   |
| `apply_invalid_json_input_errors`                        | Malformed JSON on stdin → non-zero exit, fixture directory unchanged                                                                                                                           |
| `apply_rejects_path_outside_workspace_root`              | An operation with an absolute path outside the fixture root → non-zero exit, no file created at that outside path, fixture directory unchanged                                                 |
| `apply_empty_batch_is_a_noop`                            | `[]` on stdin → exit 0, `0 operation(s)` in output, fixture directory unchanged                                                                                                                |
