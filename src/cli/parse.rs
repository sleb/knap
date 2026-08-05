use std::path::Path;

use crate::parser;

pub fn run(path: &Path) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(path)?;
    let note = parser::parse(path, &content);

    println!("path:  {}", note.path.display());

    match &note.frontmatter {
        None => println!("title: (no frontmatter)"),
        Some(fm) => {
            match &fm.title {
                None => println!("title: (none)"),
                Some(t) => println!("title: {t}"),
            }
            if !fm.tags.is_empty() {
                let names: Vec<&str> = fm.tags.iter().map(|t| t.name.as_str()).collect();
                println!("tags:  [{}]", names.join(", "));
            }
        }
    }

    if note.headings.is_empty() {
        println!("headings: none");
    } else {
        println!("headings: {}", note.headings.len());
        for h in &note.headings {
            let r = &h.range;
            let tr = &h.text_range;
            println!(
                "  h{}  \"{}\"  {}:{}\u{2013}{}:{}  (text: {}:{}\u{2013}{}:{})",
                h.level,
                h.text,
                r.start.line,
                r.start.character,
                r.end.line,
                r.end.character,
                tr.start.line,
                tr.start.character,
                tr.end.line,
                tr.end.character,
            );
        }
    }

    if note.md_links.is_empty() {
        println!("md_links: none");
    } else {
        println!("md_links: {}", note.md_links.len());
        for link in &note.md_links {
            let r = &link.range;
            let tr = &link.target_range;
            let kind = if link.is_image { "image" } else { "link" };
            let anchor_str = match (&link.anchor, &link.anchor_range) {
                (Some(a), Some(ar)) => format!(
                    "  #{a}  (anchor: {}:{}\u{2013}{}:{})",
                    ar.start.line, ar.start.character, ar.end.line, ar.end.character
                ),
                (Some(a), None) => format!("  #{a}"),
                _ => String::new(),
            };
            println!(
                "  [{kind}]  \"{}\"  →  {}  (target: {}:{}\u{2013}{}:{})  range: {}:{}\u{2013}{}:{}{}",
                link.text,
                link.target,
                tr.start.line,
                tr.start.character,
                tr.end.line,
                tr.end.character,
                r.start.line,
                r.start.character,
                r.end.line,
                r.end.character,
                anchor_str,
            );
        }
    }

    Ok(())
}
