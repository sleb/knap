use std::path::Path;

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
