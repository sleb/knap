use std::path::Path;

use crate::config;
use crate::index::{self, IndexReport, NoteIndex, ResolvedLink};
use crate::parser::Note;

/// `config::for_path` → `index::build`, same as `lint`. A directory `path`
/// keeps today's full-workspace behavior (`--json` serializes
/// [`NoteIndex::report`]); a file `path` scopes to that one note's
/// neighborhood instead of the whole workspace snapshot.
///
/// A file argument is indexed off `cwd`, not `path.parent()` —
/// `config::for_path` would otherwise treat the file's own directory as the
/// whole index root, silently dropping backlinks from anywhere else in the
/// vault (the same bug `rename-heading`/`rename-tag` fixed in v0.12).
pub fn run(path: &Path, json: bool) -> anyhow::Result<()> {
    let config = if path.is_file() {
        config::for_path(Path::new("."), None, &[])?
    } else {
        config::for_path(path, None, &[])?
    };
    let extensions: Vec<&str> = config.extensions.iter().map(String::as_str).collect();
    let (idx, _) = index::build(&config.index_roots, &extensions);

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
            println!("{}", serde_json::to_string_pretty(&summary)?);
        } else {
            print_note(note, &idx);
        }
        return Ok(());
    }

    if json {
        let report: IndexReport = idx.report();
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    let mut notes: Vec<_> = idx.all_notes().collect();
    notes.sort_by(|a, b| a.path.cmp(&b.path));

    println!("{} note(s) indexed", notes.len());

    for note in notes {
        println!();
        print_note(note, &idx);
    }

    Ok(())
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
