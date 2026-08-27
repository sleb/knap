use std::path::Path;

use serde::Serialize;

use crate::cli::hint;
use crate::config;
use crate::index::{self, IndexReport, LinkSummary, NoteIndex, ResolvedLink};
use crate::parser::Note;

/// Wraps a `--json` report value with the optional `hint` line every
/// `--json`-emitting subcommand's report carries. `#[serde(flatten)]` keeps
/// `T`'s own fields at the top level instead of nesting them under a
/// `report` key.
#[derive(Serialize)]
struct IndexOutput<T: Serialize> {
    #[serde(flatten)]
    report: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}

/// `true` if any link in `links` failed to resolve (`resolved: None`).
fn has_broken_link(links: &[LinkSummary]) -> bool {
    links.iter().any(|link| link.resolved.is_none())
}

/// `config::for_path` → `index::build`, same as `lint`. A directory `path`
/// keeps today's full-workspace behavior (`--json` serializes
/// [`NoteIndex::report`]); a file `path` scopes to that one note's
/// neighborhood instead of the whole workspace snapshot.
///
/// A file argument is indexed off `cwd`, not `path.parent()` —
/// `config::for_path` would otherwise treat the file's own directory as the
/// whole index root, silently dropping backlinks from anywhere else in the
/// vault (the same bug `rename-heading`/`rename-tag` fixed in v0.12).
pub fn run(
    path: &Path,
    json: bool,
    exclude: &[String],
    ignore_link_target: &[String],
) -> anyhow::Result<()> {
    let config = if path.is_file() {
        config::for_path(Path::new("."), None, exclude, ignore_link_target)?
    } else {
        config::for_path(path, None, exclude, ignore_link_target)?
    };
    let (idx, _) = index::build(&config.index_roots, &config.path_filter)?;

    if path.is_file() {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let note = idx
            .all_notes()
            .find(|n| n.path.canonicalize().map(|p| p == canon).unwrap_or(false))
            .ok_or_else(|| anyhow::anyhow!("no indexed note at {}", path.display()))?;

        if json {
            let summary = idx
                .note_report(&note.path)
                .expect("just found in idx.all_notes()");
            let hint = hint::index(has_broken_link(&summary.links));
            let output = IndexOutput {
                report: summary,
                hint,
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            print_note(note, &idx);
            let hint = hint::index(note_has_broken_link(note, &idx));
            if let Some(hint) = hint {
                println!("hint: {hint}");
            }
        }
        return Ok(());
    }

    if json {
        let report: IndexReport = idx.report();
        let hint = hint::index(report.notes.iter().any(|n| has_broken_link(&n.links)));
        let output = IndexOutput { report, hint };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    let mut notes: Vec<_> = idx.all_notes().collect();
    notes.sort_by(|a, b| a.path.cmp(&b.path));

    println!("{} note(s) indexed", notes.len());

    let mut any_broken_link = false;
    for note in &notes {
        println!();
        print_note(note, &idx);
        any_broken_link |= note_has_broken_link(note, &idx);
    }

    if let Some(hint) = hint::index(any_broken_link) {
        println!("hint: {hint}");
    }

    Ok(())
}

/// `true` if any of `note`'s outgoing Markdown links fail to resolve in
/// `idx`. Mirrors [`has_broken_link`], but works from a live [`Note`]/
/// [`NoteIndex`] pair instead of an already-built [`LinkSummary`] list, for
/// text-mode rendering where no `LinkSummary`s are constructed.
fn note_has_broken_link(note: &Note, idx: &NoteIndex) -> bool {
    note.md_links
        .iter()
        .any(|link| matches!(idx.resolve(&note.path, &link.target), ResolvedLink::Broken))
}

/// Text-mode rendering of a single note's neighborhood — the path, its
/// outgoing links (resolved/broken), and who links back to it. Shared by
/// the whole-workspace listing (once per note) and `knap index <file>`
/// (once, for just that note).
fn print_note(note: &Note, idx: &NoteIndex) {
    println!("{}", note.path.display());

    if note.md_links.is_empty() {
        println!("  links: none");
    } else {
        for link in &note.md_links {
            let status = match idx.resolve(&note.path, &link.target) {
                ResolvedLink::Found(p) => format!("→ {}", p.display()),
                ResolvedLink::Broken => "broken".to_string(),
            };
            let anchor_str = link
                .anchor
                .as_deref()
                .map(|a| format!("#{a}"))
                .unwrap_or_default();
            println!("  [{}]{}  {}", link.target, anchor_str, status);
        }
    }

    let incoming = idx.links_to(&note.path);
    if !incoming.is_empty() {
        println!("  referenced by:");
        for l in incoming {
            println!("    {}", l.source_path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::index::NoteSummary;

    fn link(resolved: bool) -> LinkSummary {
        LinkSummary {
            target: "other.md".to_string(),
            anchor: None,
            resolved: resolved.then(|| PathBuf::from("other.md")),
        }
    }

    fn note_summary(links: Vec<LinkSummary>) -> NoteSummary {
        NoteSummary {
            path: PathBuf::from("note.md"),
            title: None,
            tags: Vec::new(),
            headings: Vec::new(),
            links,
            backlinks: Vec::new(),
        }
    }

    #[test]
    fn has_broken_link_true_when_any_link_unresolved() {
        assert!(has_broken_link(&[link(true), link(false)]));
    }

    #[test]
    fn has_broken_link_false_when_all_resolved() {
        assert!(!has_broken_link(&[link(true), link(true)]));
    }

    #[test]
    fn index_output_hint_present_for_file_target_with_broken_link() {
        let note = note_summary(vec![link(false)]);
        let output = IndexOutput {
            hint: hint::index(has_broken_link(&note.links)),
            report: note,
        };
        let value = serde_json::to_value(&output).expect("serializes");
        assert!(value.get("hint").is_some());
    }

    #[test]
    fn index_output_hint_absent_for_file_target_all_resolved() {
        let note = note_summary(vec![link(true)]);
        let output = IndexOutput {
            hint: hint::index(has_broken_link(&note.links)),
            report: note,
        };
        let value = serde_json::to_value(&output).expect("serializes");
        assert!(value.get("hint").is_none());
    }

    #[test]
    fn index_output_hint_present_for_directory_target_with_broken_link() {
        let report = IndexReport {
            notes: vec![
                note_summary(vec![link(true)]),
                note_summary(vec![link(false)]),
            ],
            tags: std::collections::BTreeMap::new(),
        };
        let has_broken = report.notes.iter().any(|n| has_broken_link(&n.links));
        let output = IndexOutput {
            hint: hint::index(has_broken),
            report,
        };
        let value = serde_json::to_value(&output).expect("serializes");
        assert!(value.get("hint").is_some());
    }

    #[test]
    fn index_output_top_level_shape_unchanged_by_flatten() {
        let note = note_summary(vec![link(true)]);
        let note_output = IndexOutput {
            hint: None,
            report: note,
        };
        let note_value = serde_json::to_value(&note_output).expect("serializes");
        assert!(note_value.get("report").is_none());
        assert!(note_value.get("path").is_some());

        let report = IndexReport {
            notes: Vec::new(),
            tags: std::collections::BTreeMap::new(),
        };
        let report_output = IndexOutput { hint: None, report };
        let report_value = serde_json::to_value(&report_output).expect("serializes");
        assert!(report_value.get("report").is_none());
        assert!(report_value.get("notes").is_some());
    }
}
