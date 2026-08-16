use std::collections::HashSet;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use lsp_types::Range;

use crate::cli::{fix, rename};
use crate::config::PathFilter;
use crate::{config, edit, handlers, index};

/// A single batch entry `knap apply` reads from stdin, one variant per
/// existing mutating subcommand (`rename-file`/`rename-heading`/
/// `rename-tag`/`fix`), with the same field names as that subcommand's
/// arguments. Deserialized standalone here; execution (`apply_one`/`run`)
/// lands in a later step.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
enum ChangeOp {
    RenameFile {
        old: PathBuf,
        new: PathBuf,
    },
    RenameHeading {
        file: PathBuf,
        old: String,
        new: String,
    },
    RenameTag {
        old: String,
        new: String,
    },
    Fix {
        #[serde(default = "default_fix_path")]
        path: PathBuf,
    },
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

/// The `#[serde(default = ...)]` target for `Fix.path`: `knap fix`'s own
/// default of "the current directory" (see `src/cli/fix.rs`), spelled out as
/// `"."` since `ChangeOp` has no access to the process's actual cwd — it's
/// resolved against the batch's scratch root, not the process, once
/// `apply_one` runs it.
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
            ChangeOp::RepointLink { .. } => "repoint-link",
            ChangeOp::RepointAnchor { .. } => "repoint-anchor",
        }
    }
}

/// Recursively copies every file under `src` into `dst` (which must already
/// exist), skipping anything `filter` excludes — the same `PathFilter` the
/// crawl and every live-index handler consult, so the scratch copy the batch
/// operates on has the same shape as the index (`.git`/`node_modules`/
/// `target` pruned, plus whatever `knap.toml`'s `exclude` adds).
fn copy_tree(src: &Path, dst: &Path, filter: &PathFilter) -> anyhow::Result<()> {
    fn walk(src: &Path, dst: &Path, root: &Path, filter: &PathFilter) -> anyhow::Result<()> {
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry.file_type()?.is_dir() {
                let name = entry.file_name();
                if filter.should_skip_dir(root, &entry_path, &name.to_string_lossy()) {
                    continue;
                }
                let dst_dir = dst.join(&name);
                fs::create_dir(&dst_dir)?;
                walk(&entry_path, &dst_dir, root, filter)?;
            } else if entry.file_type()?.is_file() && filter.should_index(root, &entry_path) {
                fs::copy(&entry_path, dst.join(entry.file_name()))?;
            }
        }
        Ok(())
    }

    walk(src, dst, src, filter)
}

/// Every file under `root` that `filter` admits, as a path relative to
/// `root` — the same `PathFilter` `copy_tree` uses, so a file excluded from
/// the scratch copy is also excluded here rather than looking "deleted" to
/// `diff_and_sync`.
fn relative_files(root: &Path, filter: &PathFilter) -> anyhow::Result<HashSet<PathBuf>> {
    fn walk(
        dir: &Path,
        root: &Path,
        filter: &PathFilter,
        out: &mut HashSet<PathBuf>,
    ) -> anyhow::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry.file_type()?.is_dir() {
                let name = entry.file_name();
                if filter.should_skip_dir(root, &entry_path, &name.to_string_lossy()) {
                    continue;
                }
                walk(&entry_path, root, filter, out)?;
            } else if entry.file_type()?.is_file() && filter.should_index(root, &entry_path) {
                out.insert(entry_path.strip_prefix(root)?.to_path_buf());
            }
        }
        Ok(())
    }

    let mut out = HashSet::new();
    walk(root, root, filter, &mut out)?;
    Ok(out)
}

/// Compares `src` (the batch's mutated scratch copy) against `dst` (the real
/// workspace root) and returns how many files differ — created, changed, or
/// (left behind by a rename inside the batch) present in `dst` but not
/// `src`. When `commit` is `false` this is read-only: the count is exactly
/// what a real run would touch, but nothing is written — this is what makes
/// `--dry-run` accurate instead of a guess. When `commit` is `true`, also
/// performs the sync: copies every created/changed file from `src` to `dst`,
/// then removes every file `dst` has that `src` no longer does.
fn diff_and_sync(
    src: &Path,
    dst: &Path,
    commit: bool,
    filter: &PathFilter,
) -> anyhow::Result<usize> {
    let src_files = relative_files(src, filter)?;
    let dst_files = relative_files(dst, filter)?;

    let mut changed = 0;
    for rel in &src_files {
        let (from, to) = (src.join(rel), dst.join(rel));
        // `to` missing entirely must count as "differs" regardless of
        // `from`'s content — comparing against `unwrap_or_default()` would
        // treat a brand-new *empty* file (e.g. `fix`'s stub creation) as
        // identical to a `to` that doesn't exist at all, since both read as
        // `[]`, and silently drop it from the sync.
        let differs = match fs::read(&to) {
            Ok(existing) => existing != fs::read(&from)?,
            Err(_) => true,
        };
        if differs {
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

/// The result of dispatching one `ChangeOp` — enough to report a
/// human-readable line (`summary`) and to sum `files_touched` across the
/// whole batch. `op` is `ChangeOp::kind()`'s wire value, so `--json` output
/// names the operation the same way the input did.
#[derive(Serialize)]
struct AppliedOp {
    op: &'static str,
    summary: String,
    files_touched: usize,
}

/// Dispatches one `ChangeOp` to the matching `_at`/`targets_for` call,
/// scoped to `root` (the batch's scratch copy, not the process's actual
/// cwd). Every path field is checked with `ensure_scoped` before use, since
/// `root.join` on an already-absolute path discards `root` and would
/// otherwise let one operation escape the scratch copy and mutate the real
/// filesystem directly.
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
            Ok(AppliedOp {
                op: op.kind(),
                summary: format!("#{old} → #{new}"),
                files_touched,
            })
        }
        ChangeOp::Fix { path } => {
            ensure_scoped(root, path)?;
            let path_abs = index::normalize_path(&root.join(path));
            let (idx, config, targets) = fix::targets_for(&path_abs)?;
            let fixes = fix::plan_fixes(&idx, &config, &targets);
            if fixes.is_empty() {
                return Ok(AppliedOp {
                    op: op.kind(),
                    summary: "no safe fixes found".to_string(),
                    files_touched: 0,
                });
            }
            let files_touched = fix::apply(&fixes)?;
            Ok(AppliedOp {
                op: op.kind(),
                summary: format!("applied {} fix(es) in {files_touched} file(s)", fixes.len()),
                files_touched,
            })
        }
        ChangeOp::RepointLink {
            file,
            range,
            target,
        } => {
            ensure_scoped(root, file)?;
            let file_abs = index::normalize_path(&root.join(file));
            let files_touched =
                edit::apply(&handlers::compute_link_fix(&file_abs, *range, target))?;
            Ok(AppliedOp {
                op: op.kind(),
                summary: format!("{}: repoint → '{target}'", file.display()),
                files_touched,
            })
        }
        ChangeOp::RepointAnchor {
            file,
            range,
            anchor,
        } => {
            ensure_scoped(root, file)?;
            let file_abs = index::normalize_path(&root.join(file));
            let anchor = anchor.strip_prefix('#').unwrap_or(anchor);
            let files_touched =
                edit::apply(&handlers::compute_anchor_fix(&file_abs, *range, anchor))?;
            Ok(AppliedOp {
                op: op.kind(),
                summary: format!("{}: anchor → '#{anchor}'", file.display()),
                files_touched,
            })
        }
    }
}

/// Output shape for `knap apply --json`, and the source of the plain-text
/// summary otherwise: how many operations ran (or, under `--dry-run`, would
/// run) and how many files changed in total.
#[derive(Serialize)]
struct ApplyReport {
    dry_run: bool,
    operations: Vec<AppliedOp>,
    files_touched: usize,
}

/// `knap apply [--dry-run] [--json]`: reads a JSON array of `ChangeOp`s from
/// stdin, applies them in order against a scratch copy of the current
/// directory, then syncs the result back to the real workspace — unless
/// `dry_run`, in which case `diff_and_sync` still counts what *would* change
/// but writes nothing.
///
/// The all-or-nothing guarantee falls out of ordering, not a special case:
/// nothing here touches the real workspace until every operation in the
/// batch has already succeeded against the scratch copy. If any `apply_one`
/// call fails, `run` returns before `diff_and_sync` is ever called, and the
/// scratch tempdir is deleted when it drops — the real workspace was never
/// touched.
pub fn run(dry_run: bool, json: bool) -> anyhow::Result<()> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("reading batch from stdin")?;
    let ops: Vec<ChangeOp> = serde_json::from_str(&input)
        .context("invalid batch: expected a JSON array of change operations")?;

    let root = index::normalize_path(&std::env::current_dir()?);
    let config = config::for_path(&root, None, &[]).context("loading knap.toml")?;
    let scratch = tempfile::tempdir()?;
    copy_tree(&root, scratch.path(), &config.path_filter)?;

    let mut operations = Vec::with_capacity(ops.len());
    for op in &ops {
        let applied = apply_one(scratch.path(), op).with_context(|| op.kind().to_string())?;
        operations.push(applied);
    }

    let files_touched = diff_and_sync(scratch.path(), &root, !dry_run, &config.path_filter)?;
    let report = ApplyReport {
        dry_run,
        operations,
        files_touched,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        let verb = if dry_run { "would apply" } else { "applied" };
        for applied in &report.operations {
            println!("{verb} {}: {}", applied.op, applied.summary);
        }
        let touch_verb = if dry_run { "would touch" } else { "touched" };
        println!(
            "{} operation(s), {} file(s) {touch_verb}",
            report.operations.len(),
            files_touched
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_op_deserializes_rename_file() {
        let op: ChangeOp =
            serde_json::from_str(r#"{"op":"rename-file","old":"a.md","new":"b.md"}"#).unwrap();
        assert_eq!(
            op,
            ChangeOp::RenameFile {
                old: PathBuf::from("a.md"),
                new: PathBuf::from("b.md"),
            }
        );
    }

    #[test]
    fn change_op_deserializes_rename_heading() {
        let op: ChangeOp = serde_json::from_str(
            r#"{"op":"rename-heading","file":"a.md","old":"Old Section","new":"New Section"}"#,
        )
        .unwrap();
        assert_eq!(
            op,
            ChangeOp::RenameHeading {
                file: PathBuf::from("a.md"),
                old: "Old Section".to_string(),
                new: "New Section".to_string(),
            }
        );
    }

    #[test]
    fn change_op_deserializes_rename_tag() {
        let op: ChangeOp =
            serde_json::from_str(r#"{"op":"rename-tag","old":"wip","new":"draft"}"#).unwrap();
        assert_eq!(
            op,
            ChangeOp::RenameTag {
                old: "wip".to_string(),
                new: "draft".to_string(),
            }
        );
    }

    #[test]
    fn change_op_deserializes_fix_default_path() {
        let op: ChangeOp = serde_json::from_str(r#"{"op":"fix"}"#).unwrap();
        assert_eq!(
            op,
            ChangeOp::Fix {
                path: PathBuf::from(".")
            }
        );
    }

    #[test]
    fn change_op_deserializes_repoint_link() {
        let op: ChangeOp = serde_json::from_str(
            r#"{"op":"repoint-link","file":"a.md","range":{"start":{"line":0,"character":7},"end":{"line":0,"character":13}},"target":"real.md"}"#,
        )
        .unwrap();
        assert_eq!(
            op,
            ChangeOp::RepointLink {
                file: PathBuf::from("a.md"),
                range: Range {
                    start: lsp_types::Position {
                        line: 0,
                        character: 7,
                    },
                    end: lsp_types::Position {
                        line: 0,
                        character: 13,
                    },
                },
                target: "real.md".to_string(),
            }
        );
    }

    #[test]
    fn change_op_deserializes_repoint_anchor() {
        let op: ChangeOp = serde_json::from_str(
            r##"{"op":"repoint-anchor","file":"a.md","range":{"start":{"line":0,"character":8},"end":{"line":0,"character":11}},"anchor":"#slug"}"##,
        )
        .unwrap();
        assert_eq!(
            op,
            ChangeOp::RepointAnchor {
                file: PathBuf::from("a.md"),
                range: Range {
                    start: lsp_types::Position {
                        line: 0,
                        character: 8,
                    },
                    end: lsp_types::Position {
                        line: 0,
                        character: 11,
                    },
                },
                anchor: "#slug".to_string(),
            }
        );
    }

    #[test]
    fn change_op_unknown_op_errors() {
        let result: Result<ChangeOp, serde_json::Error> =
            serde_json::from_str(r#"{"op":"delete-everything"}"#);
        assert!(result.is_err(), "expected a deserialization error");
    }

    #[test]
    fn copy_tree_copies_files_and_skips_hidden_dirs() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        fs::write(src.path().join("a.md"), "hello").unwrap();
        fs::create_dir(src.path().join("sub")).unwrap();
        fs::write(src.path().join("sub/b.md"), "world").unwrap();
        fs::create_dir(src.path().join(".git")).unwrap();
        fs::write(src.path().join(".git/HEAD"), "ref: refs/heads/main").unwrap();

        copy_tree(src.path(), dst.path(), &PathFilter::default()).unwrap();

        assert_eq!(
            fs::read_to_string(dst.path().join("a.md")).unwrap(),
            "hello"
        );
        assert_eq!(
            fs::read_to_string(dst.path().join("sub/b.md")).unwrap(),
            "world"
        );
        assert!(!dst.path().join(".git").exists());
    }

    #[test]
    fn diff_and_sync_counts_and_copies_new_file() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        fs::write(src.path().join("new.md"), "new content").unwrap();

        let count = diff_and_sync(src.path(), dst.path(), true, &PathFilter::default()).unwrap();

        assert_eq!(count, 1);
        assert_eq!(
            fs::read_to_string(dst.path().join("new.md")).unwrap(),
            "new content"
        );
    }

    #[test]
    fn diff_and_sync_counts_and_copies_new_empty_file() {
        // Regression test: a brand-new *empty* file (e.g. `fix`'s stub
        // creation) must not be mistaken for "unchanged" just because an
        // absent `dst` file also reads as empty.
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        fs::write(src.path().join("stub.md"), "").unwrap();

        let count = diff_and_sync(src.path(), dst.path(), true, &PathFilter::default()).unwrap();

        assert_eq!(count, 1);
        assert!(dst.path().join("stub.md").exists());
    }

    #[test]
    fn diff_and_sync_counts_and_copies_changed_file() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        fs::write(src.path().join("a.md"), "new").unwrap();
        fs::write(dst.path().join("a.md"), "old").unwrap();

        let count = diff_and_sync(src.path(), dst.path(), true, &PathFilter::default()).unwrap();

        assert_eq!(count, 1);
        assert_eq!(fs::read_to_string(dst.path().join("a.md")).unwrap(), "new");
    }

    #[test]
    fn diff_and_sync_removes_file_absent_from_src() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        fs::write(dst.path().join("stale.md"), "leftover").unwrap();

        let count = diff_and_sync(src.path(), dst.path(), true, &PathFilter::default()).unwrap();

        assert_eq!(count, 1);
        assert!(!dst.path().join("stale.md").exists());
    }

    #[test]
    fn diff_and_sync_dry_run_counts_without_writing() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        fs::write(src.path().join("new.md"), "new content").unwrap();
        fs::write(dst.path().join("stale.md"), "leftover").unwrap();

        let count = diff_and_sync(src.path(), dst.path(), false, &PathFilter::default()).unwrap();

        assert_eq!(count, 2);
        assert!(!dst.path().join("new.md").exists());
        assert!(dst.path().join("stale.md").exists());
    }

    #[test]
    fn ensure_scoped_accepts_relative_path_under_root() {
        let root = tempfile::tempdir().unwrap();
        assert!(ensure_scoped(root.path(), Path::new("sub/file.md")).is_ok());
    }

    #[test]
    fn ensure_scoped_rejects_path_outside_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_path = outside.path().join("file.md");
        assert!(ensure_scoped(root.path(), &outside_path).is_err());
    }

    #[test]
    fn apply_one_rename_file_reports_summary_and_touched_count() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("old.md"), "old\n").unwrap();

        let op = ChangeOp::RenameFile {
            old: PathBuf::from("old.md"),
            new: PathBuf::from("new.md"),
        };

        let applied = apply_one(root, &op).unwrap();

        assert_eq!(applied.op, "rename-file");
        assert!(
            applied.summary.contains("old.md") && applied.summary.contains("new.md"),
            "unexpected summary: {}",
            applied.summary
        );
        assert_eq!(applied.files_touched, 1);
        assert!(root.join("new.md").exists());
        assert!(!root.join("old.md").exists());
    }

    #[test]
    fn apply_one_fix_reports_no_safe_fixes_found_when_clean() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.md"), "# A\n\nNothing broken here.\n").unwrap();

        let op = ChangeOp::Fix {
            path: PathBuf::from("."),
        };

        let applied = apply_one(root, &op).unwrap();

        assert_eq!(applied.op, "fix");
        assert_eq!(applied.summary, "no safe fixes found");
        assert_eq!(applied.files_touched, 0);
    }

    #[test]
    fn apply_one_rejects_rename_file_old_outside_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_old = outside.path().join("old.md");
        fs::write(&outside_old, "old\n").unwrap();

        let op = ChangeOp::RenameFile {
            old: outside_old,
            new: PathBuf::from("new.md"),
        };

        assert!(apply_one(root.path(), &op).is_err());
    }

    #[test]
    fn apply_one_repoint_link_replaces_target_range_with_new_target() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.md"), "[text](old.md)\n").unwrap();

        let op = ChangeOp::RepointLink {
            file: PathBuf::from("a.md"),
            range: Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 7,
                },
                end: lsp_types::Position {
                    line: 0,
                    character: 13,
                },
            },
            target: "real.md".to_string(),
        };

        let applied = apply_one(root, &op).unwrap();

        assert_eq!(applied.op, "repoint-link");
        assert_eq!(applied.files_touched, 1);
        assert_eq!(
            fs::read_to_string(root.join("a.md")).unwrap(),
            "[text](real.md)\n"
        );
    }

    #[test]
    fn apply_one_repoint_anchor_replaces_range_with_bare_slug() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.md"), "[text](#old)\n").unwrap();

        let op = ChangeOp::RepointAnchor {
            file: PathBuf::from("a.md"),
            range: Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 8,
                },
                end: lsp_types::Position {
                    line: 0,
                    character: 11,
                },
            },
            anchor: "new".to_string(),
        };

        let applied = apply_one(root, &op).unwrap();

        assert_eq!(applied.op, "repoint-anchor");
        assert_eq!(applied.files_touched, 1);
        assert_eq!(
            fs::read_to_string(root.join("a.md")).unwrap(),
            "[text](#new)\n"
        );
    }

    #[test]
    fn apply_one_repoint_anchor_strips_leading_hash_from_input() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.md"), "[text](#old)\n").unwrap();

        let op = ChangeOp::RepointAnchor {
            file: PathBuf::from("a.md"),
            range: Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 8,
                },
                end: lsp_types::Position {
                    line: 0,
                    character: 11,
                },
            },
            anchor: "#new".to_string(),
        };

        apply_one(root, &op).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("a.md")).unwrap(),
            "[text](#new)\n"
        );
    }

    #[test]
    fn apply_one_rejects_repoint_link_file_outside_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("a.md");
        fs::write(&outside_file, "[text](old.md)\n").unwrap();

        let op = ChangeOp::RepointLink {
            file: outside_file,
            range: Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 7,
                },
                end: lsp_types::Position {
                    line: 0,
                    character: 13,
                },
            },
            target: "real.md".to_string(),
        };

        assert!(apply_one(root.path(), &op).is_err());
    }

    #[test]
    fn apply_one_rejects_repoint_anchor_file_outside_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("a.md");
        fs::write(&outside_file, "[text](#old)\n").unwrap();

        let op = ChangeOp::RepointAnchor {
            file: outside_file,
            range: Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 8,
                },
                end: lsp_types::Position {
                    line: 0,
                    character: 11,
                },
            },
            anchor: "new".to_string(),
        };

        assert!(apply_one(root.path(), &op).is_err());
    }
}
