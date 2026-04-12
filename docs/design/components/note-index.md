# Note Index

The server's central knowledge base. Maintains a live, queryable model of all notes. All mutations go through the Protocol Handler; all reads go through Request Handlers.

In v0.1 the index runs on a single thread — the same thread as the main message loop. No locking is needed.

---

## Types

```rust
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
/// Type alias used throughout index operations.
type AffectedPaths = HashSet<PathBuf>;
```

---

## resolve()

Stem lookup with no side effects. Used by both index mutations and request handlers.

```rust
pub fn resolve(&self, stem: &str) -> ResolvedLink {
    match self.by_stem.get(stem).map(|v| v.as_slice()) {
        Some([path]) => ResolvedLink::Found(path.clone()),
        Some(paths)  => ResolvedLink::Ambiguous(paths.to_vec()),
        _            => ResolvedLink::Broken,
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

    // 2. Add to by_stem.
    self.by_stem
        .entry(note.stem.clone())
        .or_default()
        .push(note.path.clone());

    // 3. Resolve each wiki-link and populate links_to.
    for link in &note.wiki_links {
        if let ResolvedLink::Found(target) = self.resolve(&link.stem) {
            self.links_to
                .entry(target.clone())
                .or_default()
                .push(LocatedLink {
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
```

### Resolving previously broken links (step 4)

When a new note is indexed, links in *other* notes that previously pointed at its stem (and were broken) now resolve. We find these by scanning the wiki-links of every note that references this stem — but only notes whose links are NOT already in `links_to` (i.e. were previously unresolved).

```rust
fn recheck_links_to(&mut self, new_stem: &str) -> AffectedPaths {
    let mut affected = AffectedPaths::default();
    let new_path = match self.resolve(new_stem) {
        ResolvedLink::Found(p) => p,
        _ => return affected,  // still ambiguous or broken after add
    };

    for note in self.by_path.values() {
        for link in &note.wiki_links {
            if link.stem == new_stem {
                let already_tracked = self.links_to
                    .get(&new_path)
                    .map(|ls| ls.iter().any(|l| l.source_path == note.path))
                    .unwrap_or(false);

                if !already_tracked {
                    self.links_to
                        .entry(new_path.clone())
                        .or_default()
                        .push(LocatedLink {
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
        if paths.is_empty() {
            self.by_stem.remove(&note.stem);
        }
    }

    // Collect files that link TO this note before we mutate links_to.
    // These files now have broken links and need their diagnostics republished.
    if let Some(incoming) = self.links_to.get(path) {
        for l in incoming {
            affected.insert(l.source_path.clone());
        }
    }

    // Remove all links_to entries sourced FROM this file
    // (i.e. clean up the reverse index for files this note linked to).
    for links in self.links_to.values_mut() {
        links.retain(|l| l.source_path != path);
    }
    self.links_to.retain(|_, v| !v.is_empty());

    // Files that this note linked to may also have changed diagnostics
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
        self.links_to
            .get(path)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}
```

---

## IndexDelta

Every mutation returns an `IndexDelta` describing which files were affected. The Protocol Handler uses this to decide which files need their diagnostics republished.

```rust
pub struct IndexDelta {
    /// Paths whose diagnostic state may have changed.
    /// Includes the mutated file itself, plus any other files
    /// whose link resolutions changed as a result.
    pub affected_paths: HashSet<PathBuf>,
}
```

---

## Initial crawl

Called from the Protocol Handler after `initialized`:

```rust
pub fn build(roots: &[PathBuf], extensions: &[String]) -> (NoteIndex, IndexDelta) {
    let mut index = NoteIndex::default();
    let mut all_affected = HashSet::new();

    for root in roots {
        for path in walk_files(root, extensions) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let note = parse(&path, &content);
                let delta = index.index(note);
                all_affected.extend(delta.affected_paths);
            }
        }
    }

    (index, IndexDelta { affected_paths: all_affected })
}
```

`walk_files` is a simple recursive directory walk filtered to the configured extensions. No dependency needed — `std::fs::read_dir` is sufficient for v0.1.
