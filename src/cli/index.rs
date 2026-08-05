use std::path::{Path, PathBuf};

use crate::index::{self, ResolvedLink};

/// Text-output path is today's `cmd_index` moved verbatim. `--json` support
/// and `config::for_path` loading (fixing the hardcoded `["md"]` extensions)
/// land in Step 7 — see docs/design/releases/v0.13/plan.md.
pub fn run(path: &Path, json: bool) -> anyhow::Result<()> {
    if json {
        todo!("knap index --json — Step 7")
    }

    let root = PathBuf::from(path);
    let (idx, _) = index::build(&[root], &["md"]);

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
                let anchor_str = link.anchor.as_deref()
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
