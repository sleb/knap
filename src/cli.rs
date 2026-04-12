use std::path::{Path, PathBuf};

use crate::index::{self, ResolvedLink};
use crate::parser;

pub fn cmd_parse(args: &[String]) -> anyhow::Result<()> {
    let path = args.first().ok_or_else(|| anyhow::anyhow!("usage: knap parse <file>"))?;
    let path = Path::new(path);
    let content = std::fs::read_to_string(path)?;
    let note = parser::parse(path, &content);

    println!("path:  {}", note.path.display());
    println!("stem:  {}", note.stem);

    if note.wiki_links.is_empty() {
        println!("links: none");
    } else {
        println!("links: {}", note.wiki_links.len());
        for link in &note.wiki_links {
            let r = &link.range;
            let ir = &link.inner_range;
            println!(
                "  [[{}]]  {}:{}\u{2013}{}:{}  (inner: {}:{}\u{2013}{}:{})",
                link.stem,
                r.start.line,
                r.start.character,
                r.end.line,
                r.end.character,
                ir.start.line,
                ir.start.character,
                ir.end.line,
                ir.end.character,
            );
        }
    }

    Ok(())
}

pub fn cmd_index(args: &[String]) -> anyhow::Result<()> {
    let dir = args.first().ok_or_else(|| anyhow::anyhow!("usage: knap index <dir>"))?;
    let root = PathBuf::from(dir);

    let (idx, _) = index::build(&[root], &["md"]);

    let mut notes: Vec<_> = idx.all_notes().collect();
    notes.sort_by(|a, b| a.path.cmp(&b.path));

    println!("{} note(s) indexed", notes.len());

    for note in notes {
        println!();
        println!("{}  (stem: {})", note.path.display(), note.stem);

        if note.wiki_links.is_empty() {
            println!("  links: none");
        } else {
            for link in &note.wiki_links {
                let status = match idx.resolve(&link.stem) {
                    ResolvedLink::Found(p) => format!("→ {}", p.display()),
                    ResolvedLink::Ambiguous(_) => "ambiguous".to_string(),
                    ResolvedLink::Broken => "broken".to_string(),
                };
                println!("  [[{}]]  {}", link.stem, status);
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
