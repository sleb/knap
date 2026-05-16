# Note Index

The server's central knowledge base. Maintains a live, queryable model of all notes. All mutations go through the Protocol Handler; all reads go through Request Handlers.

The index runs on a single thread — the same thread as the main message loop. No locking is needed.

---

## Types

```rust
pub struct NoteIndex {
    /// Primary store: absolute path → parsed note.
    by_path: HashMap<PathBuf, Note>,

    /// All file paths in the workspace (notes + attachments).
    /// Used to validate link targets without resolving them through `by_path`.
    all_files: HashSet<PathBuf>,

    /// Reverse index: target absolute path → all links pointing to it.
    /// Only contains links that resolved successfully at index time.
    links_to: HashMap<PathBuf, Vec<LocatedLink>>,

    /// Lowercase tag name → all paths whose frontmatter carries that tag.
    by_tag: HashMap<String, Vec<PathBuf>>,
}

/// A standard Markdown link together with the file it lives in.
pub struct LocatedLink {
    pub source_path: PathBuf,
    pub md_link: MarkdownLink,
}

pub enum ResolvedLink {
    Found(PathBuf),
    Broken,
}

/// Paths whose diagnostic state may have changed after a mutation.
/// Type alias used throughout index operations.
type AffectedPaths = HashSet<PathBuf>;
```

---

## resolve()

Resolves a link target to an absolute file path. The target is a standard
relative path (relative to the source file's location). External URLs are always
`Found` without a filesystem lookup — they are intentional and never diagnosed.

```rust
pub fn resolve(&self, source: &Path, target: &str) -> ResolvedLink {
    if looks_like_url(target) {
        return ResolvedLink::Found(PathBuf::from(target));
    }
    let candidate = source
        .parent()
        .expect("note path must have a parent directory")
        .join(target);
    // Normalise away `..` components without requiring the path to exist on disk.
    let candidate = normalize_path(&candidate);
    if self.all_files.contains(&candidate) {
        ResolvedLink::Found(candidate)
    } else {
        ResolvedLink::Broken
    }
}
```

Empty targets (anchor-only links like `[text](#heading)`) are resolved against
the source file itself by the caller before invoking `resolve`.

`normalize_path` collapses `.` and `..` components lexically (without syscalls),
since the path may not exist on disk yet (e.g. during a Quick Fix preview).

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

    // 2. Register in all_files.
    self.all_files.insert(note.path.clone());

    // 3. Resolve each local link and populate links_to.
    for link in &note.md_links {
        if link.target.is_empty() || looks_like_url(&link.target) {
            continue;
        }
        let candidate = normalize_path(
            &note.path.parent().unwrap().join(&link.target)
        );
        if self.all_files.contains(&candidate) {
            self.links_to.entry(candidate.clone()).or_default().push(LocatedLink {
                source_path: note.path.clone(),
                md_link: link.clone(),
            });
            affected.insert(candidate);
        }
    }

    // 4. Adding this note may fix broken links in other notes that pointed here.
    affected.extend(self.recheck_incoming(&note.path));

    // 5. Populate by_tag.
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

When a new file appears at path P, notes that linked to P but were previously
unresolved may now resolve. We find them by scanning `by_path` for any note
whose `md_links` contain a target that resolves to P and that is not yet tracked
in `links_to[P]`.

```rust
fn recheck_incoming(&mut self, new_path: &Path) -> AffectedPaths {
    let mut affected = AffectedPaths::default();
    let links_to = &mut self.links_to;

    for note in self.by_path.values() {
        for link in &note.md_links {
            if link.target.is_empty() || looks_like_url(&link.target) {
                continue;
            }
            let candidate = normalize_path(
                &note.path.parent().unwrap().join(&link.target)
            );
            if candidate != new_path { continue; }

            let already_tracked = self.links_to
                .get(new_path)
                .map(|ls| ls.iter().any(|l| l.source_path == note.path))
                .unwrap_or(false);

            if !already_tracked {
                self.links_to.entry(new_path.to_path_buf()).or_default().push(LocatedLink {
                    source_path: note.path.clone(),
                    md_link: link.clone(),
                });
                affected.insert(note.path.clone());
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

    self.all_files.remove(path);

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

    // Files that link TO this note now have broken links — republish diagnostics.
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

    /// All non-note file paths registered in the workspace (attachments).
    pub fn all_attachment_paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.all_files.iter().filter(|p| !self.by_path.contains_key(*p))
    }

    /// Register a non-note file (attachment) in `all_files`. Notes that link
    /// to this path and were previously broken may now resolve.
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
            for l in &incoming { affected.insert(l.source_path.clone()); }
        }
        // Remove any links_to entries sourced from this path (no-op for
        // attachments in practice, but keeps the index consistent).
        for links in self.links_to.values_mut() {
            links.retain(|l| l.source_path != path);
        }
        self.links_to.retain(|_, v| !v.is_empty());
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

Called from the Protocol Handler after `initialized`. Note files (matching `extensions`) are fully parsed; all other files are registered in `all_files` only so attachment links resolve immediately.

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
loops. Directories whose name starts with `.` (e.g. `.git`) and the well-known
build/dependency directories `node_modules` and `target` are skipped. Every
remaining file is returned — no extension filter — so that attachments can be
registered alongside notes.
