use std::path::Path;

use crate::config;
use crate::index::{self, IndexReport, ResolvedLink};

/// `config::for_path` → `index::build`, same as `lint` — this is the fix for
/// the hardcoded `extensions: &["md"]` bug. Text output unchanged from the
/// pre-Step-7 format; `--json` serializes `NoteIndex::report()`.
pub fn run(path: &Path, json: bool) -> anyhow::Result<()> {
    let config = config::for_path(path, None)?;
    let extensions: Vec<&str> = config.extensions.iter().map(String::as_str).collect();
    let (idx, _) = index::build(&config.index_roots, &extensions);

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

    Ok(())
}
