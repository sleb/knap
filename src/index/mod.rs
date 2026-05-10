use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use crate::parser::{self, MarkdownLink, Note};

#[cfg(test)]
mod tests;

#[derive(Default)]
pub struct NoteIndex {
    /// Primary store: absolute path → parsed note.
    by_path: HashMap<PathBuf, Note>,

    /// Every file in the workspace — notes and attachments alike.
    /// Used for path-relative link resolution.
    all_files: HashSet<PathBuf>,

    /// Reverse index: target path → all standard Markdown links pointing to it.
    /// Only contains links that resolved successfully at index time.
    links_to: HashMap<PathBuf, Vec<LocatedLink>>,

    /// Lowercase tag name → all paths whose frontmatter carries that tag.
    by_tag: HashMap<String, Vec<PathBuf>>,
}

/// A Markdown link together with the file it lives in.
pub struct LocatedLink {
    pub source_path: PathBuf,
    pub md_link: MarkdownLink,
}

#[derive(Debug)]
pub enum ResolvedLink {
    Found(PathBuf),
    Broken,
}

/// Paths whose diagnostic state may have changed after a mutation.
type AffectedPaths = HashSet<PathBuf>;

/// Returned by every index mutation; tells the caller which files need
/// their diagnostics republished.
#[must_use]
pub struct IndexDelta {
    pub affected_paths: AffectedPaths,
}

/// Returns `true` for targets that are external URLs.
pub fn looks_like_url(s: &str) -> bool {
    s.starts_with("https://")
        || s.starts_with("http://")
        || s.starts_with("ftp://")
        || s.starts_with("mailto:")
}

/// Collapse `.` and `..` components in `path` lexically (no syscalls).
///
/// This works correctly for paths that don't yet exist on disk, which is
/// needed during link resolution where the target file may not exist yet.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}           // drop `.`
            Component::ParentDir => { out.pop(); } // resolve `..`
            c => out.push(c),
        }
    }
    out.iter().collect()
}

impl NoteIndex {
    /// Resolve a link target relative to `source` (the linking note's path).
    ///
    /// External URLs resolve `Found` immediately without any filesystem check.
    /// Relative paths are joined to `source`'s parent directory, normalized,
    /// and looked up in `all_files`.
    pub fn resolve(&self, source: &Path, target: &str) -> ResolvedLink {
        if looks_like_url(target) {
            return ResolvedLink::Found(PathBuf::from(target));
        }
        let candidate = source
            .parent()
            .expect("note path must have a parent directory")
            .join(target);
        let candidate = normalize_path(&candidate);
        if self.all_files.contains(&candidate) {
            ResolvedLink::Found(candidate)
        } else {
            ResolvedLink::Broken
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

        // 2. Register in all_files so incoming links can resolve to this note.
        self.all_files.insert(note.path.clone());

        // 3. Resolve each md_link and populate links_to.
        for link in &note.md_links {
            if link.target.is_empty() || looks_like_url(&link.target) {
                continue;
            }
            let candidate = note
                .path
                .parent()
                .expect("note path must have a parent directory")
                .join(&link.target);
            let candidate = normalize_path(&candidate);
            if self.all_files.contains(&candidate) {
                self.links_to.entry(candidate.clone()).or_default().push(LocatedLink {
                    source_path: note.path.clone(),
                    md_link: link.clone(),
                });
                affected.insert(candidate);
            }
        }

        // 4. Adding this note may resolve previously broken links in other notes.
        affected.extend(self.recheck_incoming(&note.path));

        // 5. Populate by_tag. Deduplicate so `tags: [rust, rust]` doesn't
        //    push the same path twice into by_tag["rust"].
        if let Some(fm) = &note.frontmatter {
            let mut seen = HashSet::new();
            for tag in &fm.tags {
                let key = tag.name.to_lowercase();
                if seen.insert(key.clone()) {
                    self.by_tag.entry(key).or_default().push(note.path.clone());
                }
            }
        }

        // 6. Store the note.
        affected.insert(note.path.clone());
        self.by_path.insert(note.path.clone(), note);

        IndexDelta { affected_paths: affected }
    }

    /// When a new file is added to `all_files`, links in other notes that
    /// previously couldn't resolve to it (because it didn't exist) now resolve.
    /// Find and record them in `links_to`.
    fn recheck_incoming(&mut self, new_path: &Path) -> AffectedPaths {
        let mut affected = AffectedPaths::default();

        // Collect what we need while borrowing by_path immutably.
        // Each entry: (source_path, md_link_clone) for links that now resolve.
        let mut new_links: Vec<(PathBuf, MarkdownLink)> = Vec::new();

        for note in self.by_path.values() {
            for link in &note.md_links {
                if link.target.is_empty() || looks_like_url(&link.target) {
                    continue;
                }
                let candidate = note
                    .path
                    .parent()
                    .expect("note path must have a parent directory")
                    .join(&link.target);
                let candidate = normalize_path(&candidate);
                if candidate != new_path {
                    continue;
                }
                let already_tracked = self
                    .links_to
                    .get(new_path)
                    .map(|ls| ls.iter().any(|l| l.source_path == note.path))
                    .unwrap_or(false);
                if !already_tracked {
                    new_links.push((note.path.clone(), link.clone()));
                }
            }
        }

        for (source_path, md_link) in new_links {
            self.links_to
                .entry(new_path.to_path_buf())
                .or_default()
                .push(LocatedLink { source_path: source_path.clone(), md_link });
            affected.insert(source_path);
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

        // Remove from all_files.
        self.all_files.remove(path);

        // Remove from by_tag.
        if let Some(fm) = &note.frontmatter {
            for tag in &fm.tags {
                let key = tag.name.to_lowercase();
                if let Some(paths) = self.by_tag.get_mut(&key) {
                    paths.retain(|p| p != path);
                    if paths.is_empty() {
                        self.by_tag.remove(&key);
                    }
                }
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

    /// Distinct lowercase tag names across all indexed notes.
    pub fn all_tags(&self) -> impl Iterator<Item = &str> {
        self.by_tag.keys().map(String::as_str)
    }

    /// All notes carrying the given tag (case-insensitive match).
    pub fn notes_by_tag(&self, tag: &str) -> Vec<&Note> {
        self.by_tag
            .get(&tag.to_lowercase())
            .map(|paths| {
                paths.iter().filter_map(|p| self.by_path.get(p)).collect()
            })
            .unwrap_or_default()
    }

    /// Register a non-note file (attachment) in `all_files`. Rechecks all
    /// existing notes that link to this path so their diagnostics clear.
    pub fn add_attachment(&mut self, path: PathBuf) -> IndexDelta {
        self.all_files.insert(path.clone());
        let affected = self.recheck_incoming(&path);
        IndexDelta { affected_paths: affected }
    }

    /// Remove a non-note file from `all_files`. Notes that linked to it now
    /// have broken links and are returned in the delta.
    pub fn remove_attachment(&mut self, path: &Path) -> IndexDelta {
        self.all_files.remove(path);
        let mut affected = AffectedPaths::default();
        if let Some(incoming) = self.links_to.remove(path) {
            for l in &incoming {
                affected.insert(l.source_path.clone());
            }
        }
        // Remove any outgoing entries sourced from this path
        // (attachments have no md_links so this is a no-op in practice,
        // but keeps the index consistent).
        for links in self.links_to.values_mut() {
            links.retain(|l| l.source_path != path);
        }
        self.links_to.retain(|_, v| !v.is_empty());
        affected.insert(path.to_path_buf());
        IndexDelta { affected_paths: affected }
    }
}

/// Build an initial index by crawling `roots`. Note files (matching
/// `extensions`) are fully parsed; all other files are registered in
/// `all_files` only so attachment links resolve immediately.
pub fn build(roots: &[PathBuf], extensions: &[&str]) -> (NoteIndex, IndexDelta) {
    let mut index = NoteIndex::default();
    let mut all_affected = HashSet::new();

    for root in roots {
        for path in walk_files(root) {
            let is_note = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|ext| extensions.contains(&ext))
                .unwrap_or(false);

            if is_note {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let delta = index.index(parser::parse(&path, &content));
                    all_affected.extend(delta.affected_paths);
                }
            } else {
                let delta = index.add_attachment(path);
                all_affected.extend(delta.affected_paths);
            }
        }
    }

    (index, IndexDelta { affected_paths: all_affected })
}

fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();
    walk_dir(root, &mut results);
    results
}

fn walk_dir(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            let name = entry.file_name();
            if !should_skip_dir(name.to_string_lossy().as_ref()) {
                walk_dir(&entry.path(), out);
            }
        } else if ft.is_file() {
            out.push(entry.path());
        }
        // symlinks: ft.is_symlink() → skip to prevent infinite loops
    }
}

/// Returns `true` for directory names that should not be crawled.
/// Skips hidden directories (`.git`, `.obsidian`, …) and well-known
/// build/dependency directories that are never part of a note vault.
fn should_skip_dir(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "target")
}

#[cfg(test)]
impl NoteIndex {
    /// Index a note and discard the delta. Use in test setup where the
    /// affected-paths set is irrelevant.
    pub(crate) fn seed(&mut self, note: Note) {
        let _ = self.index(note);
    }
}
