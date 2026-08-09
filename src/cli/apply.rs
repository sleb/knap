use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::index;

/// A single batch entry `knap apply` reads from stdin, one variant per
/// existing mutating subcommand (`rename-file`/`rename-heading`/
/// `rename-tag`/`fix`), with the same field names as that subcommand's
/// arguments. Deserialized standalone here; execution (`apply_one`/`run`)
/// lands in a later step.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
#[allow(dead_code)] // wired up in Step 5 (apply_one/run)
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
}

/// The `#[serde(default = ...)]` target for `Fix.path`: `knap fix`'s own
/// default of "the current directory" (see `src/cli/fix.rs`), spelled out as
/// `"."` since `ChangeOp` has no access to the process's actual cwd — it's
/// resolved against the batch's scratch root, not the process, once
/// `apply_one` runs it.
#[allow(dead_code)] // wired up in Step 5 (apply_one/run)
fn default_fix_path() -> PathBuf {
    PathBuf::from(".")
}

impl ChangeOp {
    /// The `op` tag's wire value, for per-operation error context.
    #[allow(dead_code)] // wired up in Step 5 (apply_one/run)
    fn kind(&self) -> &'static str {
        match self {
            ChangeOp::RenameFile { .. } => "rename-file",
            ChangeOp::RenameHeading { .. } => "rename-heading",
            ChangeOp::RenameTag { .. } => "rename-tag",
            ChangeOp::Fix { .. } => "fix",
        }
    }
}

/// Recursively copies every file under `src` into `dst` (which must already
/// exist), skipping directories `index::should_skip_dir` would skip during
/// indexing. The batch never needs to see `.git`/`node_modules`/`target`,
/// and skipping them keeps the scratch copy fast even in a vault that's also
/// a large git worktree.
#[allow(dead_code)] // wired up in Step 5 (apply::run)
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

/// Every file under `root`, as a path relative to `root`, skipping
/// directories `index::should_skip_dir` would skip.
#[allow(dead_code)] // wired up in Step 5 (apply::run)
fn relative_files(root: &Path) -> anyhow::Result<HashSet<PathBuf>> {
    fn walk(dir: &Path, root: &Path, out: &mut HashSet<PathBuf>) -> anyhow::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let name = entry.file_name();
                if index::should_skip_dir(&name.to_string_lossy()) {
                    continue;
                }
                walk(&entry.path(), root, out)?;
            } else if entry.file_type()?.is_file() {
                out.insert(entry.path().strip_prefix(root)?.to_path_buf());
            }
        }
        Ok(())
    }

    let mut out = HashSet::new();
    walk(root, root, &mut out)?;
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
#[allow(dead_code)] // wired up in Step 5 (apply::run)
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

/// Resolves `path` against `root` and rejects anything that lands outside
/// it. Without this, an absolute path in the batch JSON — `Path::join`
/// discards `root` when the joined path is already absolute — would resolve
/// against the real filesystem instead of the scratch copy, silently
/// breaking the all-or-nothing guarantee: that one operation would mutate
/// real files even though every other operation in the same batch only ever
/// touches staging.
#[allow(dead_code)] // wired up in Step 5 (apply::run)
fn ensure_scoped(root: &Path, path: &Path) -> anyhow::Result<()> {
    let resolved = index::normalize_path(&root.join(path));
    anyhow::ensure!(
        resolved.starts_with(root),
        "{}: resolves outside the workspace root",
        path.display()
    );
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

        copy_tree(src.path(), dst.path()).unwrap();

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

        let count = diff_and_sync(src.path(), dst.path(), true).unwrap();

        assert_eq!(count, 1);
        assert_eq!(
            fs::read_to_string(dst.path().join("new.md")).unwrap(),
            "new content"
        );
    }

    #[test]
    fn diff_and_sync_counts_and_copies_changed_file() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        fs::write(src.path().join("a.md"), "new").unwrap();
        fs::write(dst.path().join("a.md"), "old").unwrap();

        let count = diff_and_sync(src.path(), dst.path(), true).unwrap();

        assert_eq!(count, 1);
        assert_eq!(fs::read_to_string(dst.path().join("a.md")).unwrap(), "new");
    }

    #[test]
    fn diff_and_sync_removes_file_absent_from_src() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        fs::write(dst.path().join("stale.md"), "leftover").unwrap();

        let count = diff_and_sync(src.path(), dst.path(), true).unwrap();

        assert_eq!(count, 1);
        assert!(!dst.path().join("stale.md").exists());
    }

    #[test]
    fn diff_and_sync_dry_run_counts_without_writing() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        fs::write(src.path().join("new.md"), "new content").unwrap();
        fs::write(dst.path().join("stale.md"), "leftover").unwrap();

        let count = diff_and_sync(src.path(), dst.path(), false).unwrap();

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
}
