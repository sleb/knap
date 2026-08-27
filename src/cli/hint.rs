//! Builds the single `hint` string each `--json`-emitting subcommand's
//! report carries (and text mode prints as a trailing line), from a couple
//! of bools the caller already has in scope. Pure formatting — no new
//! diagnostic, ranking, or index logic lives here.

/// `knap lint`'s hint. `has_repointable` is true if any diagnostic's `code`
/// is `broken-link`/`broken-anchor`; `has_manual` is true if any is one of
/// the four frontmatter codes. Both false (including the clean case, zero
/// diagnostics) returns `None` — nothing to point at.
///
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_lint_none_when_neither_flag_set() {
        assert!(lint(false, false).is_none());
    }

    #[test]
    fn hint_lint_repointable_only() {
        let hint = lint(true, false).expect("expected a hint");
        assert!(hint.contains("apply") || hint.contains("rename-"));
        assert!(!hint.contains("frontmatter"));
    }

    #[test]
    fn hint_lint_manual_only() {
        let hint = lint(false, true).expect("expected a hint");
        assert!(hint.contains("frontmatter"));
        assert!(!hint.contains("apply"));
        assert!(!hint.contains("rename-"));
    }

    #[test]
    fn hint_lint_both_flags_joins_both_clauses() {
        let hint = lint(true, true).expect("expected a hint");
        assert!(hint.contains("apply") || hint.contains("rename-"));
        assert!(hint.contains("frontmatter"));
    }

    #[test]
    fn hint_apply_some_when_not_dry_run() {
        let hint = apply(false).expect("expected a hint");
        assert!(hint.contains("knap lint"));
    }

    #[test]
    fn hint_apply_none_when_dry_run() {
        assert!(apply(true).is_none());
    }

    #[test]
    fn hint_rename_is_always_some() {
        let hint = rename().expect("expected a hint");
        assert!(hint.contains("rename-"));
        assert!(hint.contains("knap lint"));
    }

    #[test]
    fn hint_index_some_when_broken_link() {
        let hint = index(true).expect("expected a hint");
        assert!(hint.contains("knap lint --suggest"));
    }

    #[test]
    fn hint_index_none_when_no_broken_link() {
        assert!(index(false).is_none());
    }
}
