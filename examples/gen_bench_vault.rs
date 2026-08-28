//! Generates a synthetic Markdown vault for the agentic-editing benchmark
//! described in `docs/design/experiments/agentic-efficiency-benchmark.md`.
//!
//! Lives under `examples/` rather than `src/bin/` so it never ships as part
//! of the `knap` binary or adds a dependency to it — it's a dev-only tool,
//! run with `cargo run --example gen_bench_vault`.
//!
//! Deterministic: the same `--seed` always produces byte-identical output
//! (a `rand::rngs::StdRng` seeded from `--seed`), so a generated vault can
//! be committed and reused across benchmark runs without checking in the
//! generator's output by hand.
//!
//! ```text
//! cargo run --example gen_bench_vault -- --out /tmp/bench-vault --seed 1
//! ```

use std::fs;
use std::path::PathBuf;

use clap::Parser;
use knap::slug;
use rand::rngs::StdRng;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::{RngExt, SeedableRng};
use serde::Serialize;

#[derive(Parser)]
struct Args {
    /// Directory to write the generated vault into (cleared first if it exists).
    #[arg(long, default_value = "bench-vault")]
    out: PathBuf,

    /// RNG seed — same seed, same vault, byte for byte.
    #[arg(long, default_value_t = 1)]
    seed: u64,

    /// Number of notes to generate.
    #[arg(long, default_value_t = 50)]
    notes: usize,

    /// How many of the generated links to break (broken-link defects).
    #[arg(long, default_value_t = 5)]
    broken_links: usize,

    /// How many of the generated anchor links to break (broken-anchor defects).
    #[arg(long, default_value_t = 3)]
    broken_anchors: usize,
}

const DIRS: &[&str] = &["topics", "reference", "projects", "notes"];
const TAGS: &[&str] = &["draft", "published", "reference", "idea", "archived"];

const WORDS: &[&str] = &[
    "auth",
    "cache",
    "config",
    "index",
    "parser",
    "server",
    "client",
    "queue",
    "schema",
    "router",
    "session",
    "token",
    "vault",
    "graph",
    "pipeline",
    "worker",
    "handler",
    "resolver",
    "template",
    "registry",
    "gateway",
    "monitor",
    "backup",
    "search",
    "sync",
    "migration",
    "storage",
    "runtime",
    "scheduler",
    "logging",
    "metrics",
    "dashboard",
    "workflow",
    "release",
    "onboarding",
    "billing",
    "notifications",
    "permissions",
    "audit",
    "deployment",
    "testing",
    "checklist",
    "retrospective",
    "roadmap",
    "architecture",
    "glossary",
    "faq",
    "changelog",
    "style-guide",
    "playbook",
    "incident",
];

struct Heading {
    text: String,
    slug: String,
}

struct Note {
    dir: &'static str,
    slug_name: String,
    title: String,
    headings: Vec<Heading>,
    tags: Vec<&'static str>,
    tag_style: usize, // 0 = bare scalar, 1 = inline list, 2 = block list
}

impl Note {
    fn rel_path(&self) -> String {
        format!("{}/{}.md", self.dir, self.slug_name)
    }
}

/// A link planted in a note's body, tracked separately from the rendered
/// Markdown so the defect-seeding pass can find-and-replace it precisely.
struct PlantedLink {
    note_index: usize,
    target_index: usize,
    anchor: Option<usize>, // index into target's headings, if this is an anchor link
}

fn main() {
    let args = Args::parse();
    let mut rng = StdRng::seed_from_u64(args.seed);

    if args.out.exists() {
        fs::remove_dir_all(&args.out).expect("clear output dir");
    }
    for dir in DIRS {
        fs::create_dir_all(args.out.join(dir)).expect("create subdir");
    }

    // ─── Note identities ────────────────────────────────────────────────
    let mut notes = Vec::with_capacity(args.notes);
    let mut used_names = std::collections::HashSet::new();
    for i in 0..args.notes {
        let dir = DIRS[i % DIRS.len()];
        let mut name = WORDS.choose(&mut rng).unwrap().to_string();
        while !used_names.insert(format!("{dir}/{name}")) {
            name = format!(
                "{}-{}",
                WORDS.choose(&mut rng).unwrap(),
                rng.random_range(0..1000)
            );
        }
        let title = titlecase(&name);
        let heading_count = 2 + rng.random_range(0..3); // 2..=4
        let headings = (0..heading_count)
            .map(|h| {
                let text = format!(
                    "{} {}",
                    titlecase(WORDS.choose(&mut rng).unwrap()),
                    section_word(h)
                );
                Heading {
                    slug: slug(&text),
                    text,
                }
            })
            .collect();
        let tag_count = 1 + rng.random_range(0..2); // 1..=2
        let mut tags = vec![];
        while tags.len() < tag_count {
            let t = *TAGS.choose(&mut rng).unwrap();
            if !tags.contains(&t) {
                tags.push(t);
            }
        }
        notes.push(Note {
            dir,
            slug_name: name,
            title,
            headings,
            tags,
            tag_style: i % 3,
        });
    }

    // ~30% of notes are "hubs" that other notes preferentially link to —
    // this is what gives rename/retag a real blast radius to exercise,
    // instead of a uniformly flat link graph.
    let hub_count = (args.notes / 3).max(1);
    let hubs: Vec<usize> = (0..hub_count).collect();

    // ─── Link graph ─────────────────────────────────────────────────────
    let mut planted = Vec::new();
    for i in 0..notes.len() {
        let link_count = 2 + rng.random_range(0..4); // 2..=5
        for _ in 0..link_count {
            let target = if rng.random_ratio(3, 5) && !hubs.is_empty() {
                *hubs.choose(&mut rng).unwrap()
            } else {
                rng.random_range(0..notes.len())
            };
            if target == i {
                continue;
            }
            let anchor = if rng.random_ratio(1, 4) && !notes[target].headings.is_empty() {
                Some(rng.random_range(0..notes[target].headings.len()))
            } else {
                None
            };
            planted.push(PlantedLink {
                note_index: i,
                target_index: target,
                anchor,
            });
        }
    }

    // ─── Seed defects (before rendering, so bodies embed the broken form) ──
    // Pick distinct planted links to break; report exactly what was broken
    // so the benchmark's "fix" tasks have a known-correct answer.
    let mut defect_indices: Vec<usize> = (0..planted.len()).collect();
    defect_indices.shuffle(&mut rng);
    let mut manifest = Manifest::default();

    let broken_link_targets: Vec<usize> = defect_indices
        .iter()
        .copied()
        .filter(|&idx| planted[idx].anchor.is_none())
        .take(args.broken_links)
        .collect();
    let broken_anchor_targets: Vec<usize> = defect_indices
        .iter()
        .copied()
        .filter(|&idx| planted[idx].anchor.is_some() && !broken_link_targets.contains(&idx))
        .take(args.broken_anchors)
        .collect();

    // ─── Render bodies ──────────────────────────────────────────────────
    // Seeded-defect targets must be *unresolvable by string-stripping* — a
    // fixed, recognizable marker (a literal `-removed`/`-stale` suffix) lets
    // an agent "fix" the defect by pattern-matching the marker away instead
    // of actually resolving the broken reference against real candidates in
    // the vault. Mangling by deleting/duplicating a random character instead
    // produces a target that doesn't exist but carries no recognizable
    // shape, so recovering the correct one requires genuine matching
    // (ranking, text, or directory signal). Collision-check against every
    // real slug/heading-slug in the vault so a mangled target can't
    // accidentally land on another real note or heading by chance.
    let all_slug_names: std::collections::HashSet<String> =
        notes.iter().map(|n| n.slug_name.clone()).collect();
    let all_heading_slugs: std::collections::HashSet<String> = notes
        .iter()
        .flat_map(|n| n.headings.iter().map(|h| h.slug.clone()))
        .collect();

    // Group planted links by source note so each note's body can weave them
    // into sentences in one pass.
    let mut by_source: Vec<Vec<usize>> = vec![Vec::new(); notes.len()];
    for (idx, link) in planted.iter().enumerate() {
        by_source[link.note_index].push(idx);
    }

    for (i, note) in notes.iter().enumerate() {
        let mut body = String::new();
        body.push_str("---\n");
        body.push_str(&format!("title: {}\n", note.title));
        match note.tag_style {
            0 => body.push_str(&format!("tags: {}\n", note.tags[0])),
            1 => body.push_str(&format!(
                "tags: [{}]\n",
                note.tags
                    .iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            _ => {
                body.push_str("tags:\n");
                for t in &note.tags {
                    body.push_str(&format!("  - {t}\n"));
                }
            }
        }
        body.push_str("---\n\n");
        body.push_str(&format!("# {}\n\n", note.title));
        body.push_str(&format!(
            "Notes on {}, covering the pieces relevant to day-to-day work.\n\n",
            note.title.to_lowercase()
        ));

        for link_idx in &by_source[i] {
            let link = &planted[*link_idx];
            let target = &notes[link.target_index];
            let mut href = path_from(note.dir, &target.rel_path());
            let mut anchor_slug = None;
            if let Some(h) = link.anchor {
                let mut s = target.headings[h].slug.clone();
                if broken_anchor_targets.contains(link_idx) {
                    manifest.broken_anchors.push(DefectRecord {
                        file: note.rel_path(),
                        original_target: format!("{href}#{s}"),
                    });
                    s = mangle_word(&s, &all_heading_slugs, &mut rng);
                }
                anchor_slug = Some(s);
            }
            if broken_link_targets.contains(link_idx) {
                manifest.broken_links.push(DefectRecord {
                    file: note.rel_path(),
                    original_target: href.clone(),
                });
                let mangled_stem = mangle_word(&target.slug_name, &all_slug_names, &mut rng);
                href = href.replacen(&target.slug_name, &mangled_stem, 1);
            }
            let dest = match &anchor_slug {
                Some(s) => format!("{href}#{s}"),
                None => href,
            };
            body.push_str(&format!(
                "See [{}]({}) for related context.\n\n",
                target.title, dest
            ));
        }

        for h in &note.headings {
            body.push_str(&format!("## {}\n\n", h.text));
            body.push_str(&format!(
                "Details on {} live here.\n\n",
                h.text.to_lowercase()
            ));
        }

        fs::write(args.out.join(note.rel_path()), body).expect("write note");
    }

    manifest.note_count = notes.len();
    manifest.planted_link_count = planted.len();
    let manifest_path = args.out.join("BENCH_MANIFEST.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .expect("write manifest");

    println!(
        "generated {} notes, {} links ({} broken-link, {} broken-anchor defects) in {}",
        manifest.note_count,
        manifest.planted_link_count,
        manifest.broken_links.len(),
        manifest.broken_anchors.len(),
        args.out.display()
    );
    println!("answer key: {}", manifest_path.display());
}

#[derive(Default, Serialize)]
struct Manifest {
    note_count: usize,
    planted_link_count: usize,
    broken_links: Vec<DefectRecord>,
    broken_anchors: Vec<DefectRecord>,
}

#[derive(Serialize)]
struct DefectRecord {
    file: String,
    original_target: String,
}

fn titlecase(s: &str) -> String {
    s.split(['-', '_', ' '])
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn section_word(i: usize) -> &'static str {
    ["Overview", "Details", "Notes", "Background", "Appendix"][i % 5]
}

/// Mangles `s` into a same-shaped but non-existent string by deleting (or,
/// for very short strings, duplicating) a random character — never by
/// appending a fixed, recognizable marker. Retries against `existing` so the
/// result can't accidentally collide with a real slug/heading-slug already
/// in the vault.
fn mangle_word(s: &str, existing: &std::collections::HashSet<String>, rng: &mut StdRng) -> String {
    for _ in 0..8 {
        let mut chars: Vec<char> = s.chars().collect();
        if chars.is_empty() {
            break;
        }
        let idx = rng.random_range(0..chars.len());
        if chars.len() > 3 {
            chars.remove(idx);
        } else {
            chars.insert(idx, chars[idx]);
        }
        let candidate: String = chars.into_iter().collect();
        if candidate != s && !existing.contains(&candidate) {
            return candidate;
        }
    }
    // Astronomically unlikely fallback: still non-mechanical (not a fixed
    // suffix), just not collision-checked.
    format!("{s}{}", rng.random_range(0..10))
}

/// Relative path from a note in `from_dir` to another note's vault-root
/// path. All note directories are siblings one level below the vault root,
/// so climbing out (`../`) and back into the target's own directory always
/// resolves correctly, same-directory links included (`../topics/y.md` from
/// `topics/x.md` normalizes right back to `topics/y.md`).
fn path_from(_from_dir: &str, to_rel: &str) -> String {
    format!("../{to_rel}")
}
