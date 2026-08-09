use std::path::PathBuf;

use serde::Deserialize;

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
}
