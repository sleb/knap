use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::parser::{self, Note, WikiLink};

#[cfg(test)]
mod tests;

#[derive(Default)]
pub struct NoteIndex {
    /// Primary store: absolute path → parsed note.
    by_path: HashMap<PathBuf, Note>,

    /// Stem → all paths that have that stem.
    /// Length 0: broken. Length 1: resolved. Length >1: ambiguous.
    by_stem: HashMap<String, Vec<PathBuf>>,

    /// Reverse index: target path → all wiki-links pointing to it.
    /// Only contains links that resolved successfully at index time.
    links_to: HashMap<PathBuf, Vec<LocatedLink>>,
}

/// A wiki-link together with the file it lives in.
pub struct LocatedLink {
    pub source_path: PathBuf,
    pub wiki_link: WikiLink,
}

pub enum ResolvedLink {
    Found(PathBuf),
    Ambiguous(Vec<PathBuf>),
    Broken,
}

/// Paths whose diagnostic state may have changed after a mutation.
type AffectedPaths = HashSet<PathBuf>;

/// Returned by every index mutation; tells the caller which files need
/// their diagnostics republished.
pub struct IndexDelta {
    pub affected_paths: AffectedPaths,
}

impl NoteIndex {
    /// Stem lookup with no side effects.
    pub fn resolve(&self, stem: &str) -> ResolvedLink {
        match self.by_stem.get(stem).map(|v| v.as_slice()) {
            Some([path]) => ResolvedLink::Found(path.clone()),
            Some(paths) => ResolvedLink::Ambiguous(paths.to_vec()),
            _ => ResolvedLink::Broken,
        }
    }

    /// Add or replace a note. Calling `index()` on an already-known path
    /// replaces it cleanly.
    pub fn index(&mut self, note: Note) -> IndexDelta {
        // 1. Remove the old version if present, collecting affected paths.
        let mut affected = if self.by_path.contains_key(&note.path) {
            self.remove_internal(&note.path)
        } else {
            AffectedPaths::default()
        };

        // 2. Add to by_stem.
        self.by_stem.entry(note.stem.clone()).or_default().push(note.path.clone());

        // 3. Resolve each wiki-link and populate links_to.
        for link in &note.wiki_links {
            if let ResolvedLink::Found(target) = self.resolve(&link.stem) {
                self.links_to.entry(target.clone()).or_default().push(LocatedLink {
                    source_path: note.path.clone(),
                    wiki_link: link.clone(),
                });
                affected.insert(target);
            }
        }

        // 4. Adding this note may resolve previously broken links in other notes.
        affected.extend(self.recheck_links_to(&note.stem));

        // 5. Store the note.
        affected.insert(note.path.clone());
        self.by_path.insert(note.path.clone(), note);

        IndexDelta { affected_paths: affected }
    }

    /// When a new note is indexed, links in other notes that previously pointed
    /// at its stem (and were broken) now resolve. Find and record them.
    fn recheck_links_to(&mut self, new_stem: &str) -> AffectedPaths {
        let mut affected = AffectedPaths::default();
        let new_path = match self.resolve(new_stem) {
            ResolvedLink::Found(p) => p,
            _ => return affected, // still ambiguous or broken after add
        };

        // Split borrow: iterate by_path immutably while mutating links_to.
        let by_path = &self.by_path;
        let links_to = &mut self.links_to;

        for note in by_path.values() {
            for link in &note.wiki_links {
                if link.stem == new_stem {
                    let already_tracked = links_to
                        .get(&new_path)
                        .map(|ls| ls.iter().any(|l| l.source_path == note.path))
                        .unwrap_or(false);

                    if !already_tracked {
                        links_to.entry(new_path.clone()).or_default().push(LocatedLink {
                            source_path: note.path.clone(),
                            wiki_link: link.clone(),
                        });
                        affected.insert(note.path.clone());
                    }
                }
            }
        }
        affected
    }

    /// Remove a note from the index.
    pub fn remove(&mut self, path: &Path) -> IndexDelta {
        let affected = self.remove_internal(path);
        IndexDelta { affected_paths: affected }
    }

    fn remove_internal(&mut self, path: &Path) -> AffectedPaths {
        let mut affected = AffectedPaths::default();

        let Some(note) = self.by_path.remove(path) else {
            return affected;
        };

        // Remove from by_stem.
        if let Some(paths) = self.by_stem.get_mut(&note.stem) {
            paths.retain(|p| p != path);
            if paths.is_empty() {
                self.by_stem.remove(&note.stem);
            }
        }

        // Files that link TO this note now have broken links — republish their diagnostics.
        // Remove the entry for this path from the reverse index at the same time.
        if let Some(incoming) = self.links_to.remove(path) {
            for l in &incoming {
                affected.insert(l.source_path.clone());
            }
        }

        // Remove all links_to entries sourced FROM this file.
        for links in self.links_to.values_mut() {
            links.retain(|l| l.source_path != path);
        }
        self.links_to.retain(|_, v| !v.is_empty());

        // Files this note linked to may also have changed diagnostics
        // (e.g. ambiguous stem resolves now that one candidate is gone).
        for link in &note.wiki_links {
            if let ResolvedLink::Found(target) = self.resolve(&link.stem) {
                affected.insert(target);
            }
        }

        affected.insert(path.to_path_buf());
        affected
    }

    pub fn get_note(&self, path: &Path) -> Option<&Note> {
        self.by_path.get(path)
    }

    pub fn all_notes(&self) -> impl Iterator<Item = &Note> {
        self.by_path.values()
    }

    /// All links from other notes that point to `path`.
    pub fn links_to(&self, path: &Path) -> &[LocatedLink] {
        self.links_to.get(path).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// Build an initial index by crawling `roots` for files with the given extensions.
pub fn build(roots: &[PathBuf], extensions: &[&str]) -> (NoteIndex, IndexDelta) {
    let mut index = NoteIndex::default();
    let mut all_affected = HashSet::new();

    for root in roots {
        for path in walk_files(root, extensions) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let note = parser::parse(&path, &content);
                let delta = index.index(note);
                all_affected.extend(delta.affected_paths);
            }
        }
    }

    (index, IndexDelta { affected_paths: all_affected })
}

fn walk_files(root: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut results = Vec::new();
    walk_dir(root, extensions, &mut results);
    results
}

fn walk_dir(dir: &Path, extensions: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_dir(&path, extensions, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && extensions.contains(&ext)
        {
            out.push(path);
        }
    }
}
