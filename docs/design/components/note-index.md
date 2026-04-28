# Note Index

The server's central knowledge base. Maintains a live, queryable model of all notes. All mutations go through the Protocol Handler; all reads go through Request Handlers.

The index runs on a single thread — the same thread as the main message loop. No locking is needed.

---

## Types

```rust
pub struct NoteIndex {
    /// Primary store: absolute path → parsed note.
    by_path: HashMap<PathBuf, Note>,

    /// Stem → all paths that have that stem.
    /// Length 0: broken. Length 1: resolved. Length >1: ambiguous.
    by_stem: HashMap<String, Vec<PathBuf>>,

    /// Full filename (including extension) → all paths that have that filename.
    /// Covers every file in the workspace, not just note extensions.
    by_filename: HashMap<String, Vec<PathBuf>>,

    /// Reverse index: target path → all wiki-links pointing to it.
    /// Only contains links that resolved successfully at index time.
    links_to: HashMap<PathBuf, Vec<LocatedLink>>,

    /// Lowercase tag name → all paths whose frontmatter carries that tag.
    by_tag: HashMap<String, Vec<PathBuf>>,
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
/// Type alias used throughout index operations.
type AffectedPaths = HashSet<PathBuf>;
```

---

## resolve()

Checks `by_stem` first; if not found, falls through to `by_filename`. This handles both note links (`[[stem]]`) and attachment links (`[[image.png]]`) without any classification step.

```rust
pub fn resolve(&self, target: &str) -> ResolvedLink {
    match self.by_stem.get(target).map(|v| v.as_slice()) {
        Some([path]) => return ResolvedLink::Found(path.clone()),
        Some(paths)  => return ResolvedLink::Ambiguous(paths.to_vec()),
        None         => {}
    }
    match self.by_filename.get(target).map(|v| v.as_slice()) {
        Some([path]) => ResolvedLink::Found(path.clone()),
        Some(paths)  => ResolvedLink::Ambiguous(paths.to_vec()),
        None         => ResolvedLink::Broken,
    }
}
```

---

## index()

Adds or replaces a note. Calling `index()` on an already-known path replaces it cleanly.

```rust
pub fn index(&mut self, note: Note) -> IndexDelta {
    // 1. Remove the old version if present, collecting affected paths.
    let mut affected = if self.by_path.contains_key(&note.path) {
        self.remove_internal(&note.path)
    } else {
        AffectedPaths::default()
    };

    // 2. Add to by_stem and by_filename.
    self.by_stem.entry(note.stem.clone()).or_default().push(note.path.clone());
    self.by_filename.entry(note.filename()).or_default().push(note.path.clone());

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
```

### Resolving previously broken links (step 4)

When a new note is indexed, links in _other_ notes that previously pointed at its stem (and were broken) now resolve. We find these by scanning the wiki-links of every note that references this stem — but only notes whose links are NOT already in `links_to` (i.e. were previously unresolved).

```rust
fn recheck_links_to(&mut self, new_stem: &str) -> AffectedPaths {
    let mut affected = AffectedPaths::default();
    let new_path = match self.resolve(new_stem) {
        ResolvedLink::Found(p) => p,
        _ => return affected,  // still ambiguous or broken after add
    };

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
```

---

## remove()

```rust
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
        if paths.is_empty() { self.by_stem.remove(&note.stem); }
    }

    // Remove from by_tag.
    if let Some(fm) = &note.frontmatter {
        for tag in &fm.tags {
            let key = tag.name.to_lowercase();
            if let Some(paths) = self.by_tag.get_mut(&key) {
                paths.retain(|p| p != path);
                if paths.is_empty() { self.by_tag.remove(&key); }
            }
        }
    }

    // Remove from by_filename.
    let filename = note.filename();
    if let Some(paths) = self.by_filename.get_mut(&filename) {
        paths.retain(|p| p != path);
        if paths.is_empty() { self.by_filename.remove(&filename); }
    }

    // Files that link TO this note now have broken links — republish diagnostics.
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
```

---

## Read methods

```rust
impl NoteIndex {
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
            .map(|paths| paths.iter().filter_map(|p| self.by_path.get(p)).collect())
            .unwrap_or_default()
    }

    /// Register a non-note file (attachment) in `by_filename`. Rechecks all
    /// existing notes that link to this filename so their diagnostics clear.
    pub fn add_attachment(&mut self, path: PathBuf) -> IndexDelta {
        let filename = path.file_name()
            .expect("attachment path must have a filename")
            .to_string_lossy()
            .into_owned();
        self.by_filename.entry(filename.clone()).or_default().push(path);
        let affected = self.recheck_links_to(&filename);
        IndexDelta { affected_paths: affected }
    }

    /// Remove a non-note file from `by_filename`. Notes that linked to it now
    /// have broken links and are returned in the delta.
    pub fn remove_attachment(&mut self, path: &Path) -> IndexDelta {
        let filename = path.file_name()
            .expect("attachment path must have a filename")
            .to_string_lossy()
            .into_owned();
        if let Some(paths) = self.by_filename.get_mut(&filename) {
            paths.retain(|p| p != path);
            if paths.is_empty() { self.by_filename.remove(&filename); }
        }
        let mut affected = AffectedPaths::default();
        if let Some(incoming) = self.links_to.remove(path) {
            for l in &incoming { affected.insert(l.source_path.clone()); }
        }
        affected.insert(path.to_path_buf());
        IndexDelta { affected_paths: affected }
    }
}
```

---

## IndexDelta

Every mutation returns an `IndexDelta` describing which files were affected. The Protocol Handler uses this to decide which files need their diagnostics republished.

```rust
#[must_use]
pub struct IndexDelta {
    /// Paths whose diagnostic state may have changed.
    /// Includes the mutated file itself, plus any other files
    /// whose link resolutions changed as a result.
    pub affected_paths: HashSet<PathBuf>,
}
```

---

## Initial crawl

Called from the Protocol Handler after `initialized`. Note files (matching `extensions`) are fully parsed; all other files are registered in `by_filename` only so attachment links resolve immediately.

```rust
pub fn build(roots: &[PathBuf], extensions: &[&str]) -> (NoteIndex, IndexDelta) {
    let mut index = NoteIndex::default();
    let mut all_affected = HashSet::new();

    for root in roots {
        for path in walk_files(root) {
            let is_note = path.extension()
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
```

`walk_files` is a recursive directory walk. It uses `entry.file_type()` (not
`path.is_dir()`) so symlinked directories are never followed, preventing infinite
loops. Directories whose name starts with `.` (e.g. `.git`, `.obsidian`) and the
well-known build/dependency directories `node_modules` and `target` are skipped.
Every remaining file is returned — no extension filter — so that attachments can
be registered alongside notes.
