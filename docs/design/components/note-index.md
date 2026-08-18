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

    /// Every directory in the workspace (including index roots), normalized.
    /// Used so links to a directory resolve like links to a file.
    all_dirs: HashSet<PathBuf>,

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
    if is_url_like(target) {
        return ResolvedLink::Found(PathBuf::from(target));
    }
    if target.is_empty() {
        return ResolvedLink::Found(source.to_path_buf());
    }
    let target = unescape_link_target(target);
    let candidate = source
        .parent()
        .expect("note path must have a parent directory")
        .join(target.as_ref());
    // Normalise away `..` components without requiring the path to exist on disk.
    let candidate = normalize_path(&candidate);
    if self.target_exists(&candidate) {
        ResolvedLink::Found(candidate)
    } else {
        ResolvedLink::Broken
    }
}
```

`target_exists` (private) is `self.all_files.contains(path) || self.all_dirs.contains(path)`
(v0.17) — a link resolves the same way whether it targets a file or a
directory. `index()` step 3 and `recheck_incoming()` (below) both call
`target_exists` too, so a link to a directory populates `links_to` and
participates in backlink tracking exactly like a link to a file.

Empty targets (anchor-only links like `[text](#heading)`) resolve to the
source file itself directly inside `resolve()` (v0.11.1, #60) — previously
this was a special case duplicated across each caller (`compute_diagnostics`,
`handle_definition`, completion's anchor trigger); those callers now flow
through this same branch instead of special-casing the empty target
themselves.

`normalize_path` collapses `.` and `..` components lexically (without syscalls),
since the path may not exist on disk yet (e.g. during a Quick Fix preview).

`unescape_link_target` (`pub(crate)`, defined alongside `resolve`) strips an
existing `<...>` wrapping and un-escapes backslash-escaped `<`, `>`, `\`
inside it — `link.target` for an already-valid, already-wrapped link (e.g.
`[text](<My File>)`, produced by completion or "Create note" when the path
needs escaping, see `docs/design/releases/archive/v0.10.2/design.md`) is the literal
wrapped string, since the parser records the raw text between `(` and `)`
verbatim. Targets that were never wrapped pass through unchanged.

`index()`'s `links_to` population (step 3, below) and `recheck_incoming()`
also call `unescape_link_target` before joining, matching `resolve()` —
fixed in #61 after v0.10.2 shipped without it, where a link to a file whose
name needed escaping (`[text](<My File>)`) resolved correctly via
`resolve()` but was silently missing from `links_to`, undercounting
backlinks and Find References even though diagnostics showed it as
resolved.

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
    // Matches resolve(): unescape before joining, so an already-wrapped
    // target (`<My File>`) still matches all_files here (#61).
    for link in &note.md_links {
        if link.target.is_empty() || is_url_like(&link.target) {
            continue;
        }
        let target = unescape_link_target(&link.target);
        let candidate = normalize_path(
            &note.path.parent().unwrap().join(target.as_ref())
        );
        if self.target_exists(&candidate) {
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
            if link.target.is_empty() || is_url_like(&link.target) {
                continue;
            }
            let target = unescape_link_target(&link.target);
            let candidate = normalize_path(
                &note.path.parent().unwrap().join(target.as_ref())
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
    pub fn notes_by_tag<'a>(&'a self, tag: &str) -> impl Iterator<Item = &'a Note> {
        self.by_tag
            .get(&tag.to_lowercase())
            .into_iter()
            .flat_map(|paths| paths.iter().filter_map(|p| self.by_path.get(p)))
    }

    /// All non-note file paths registered in the workspace (attachments).
    pub fn all_attachment_paths(&self) -> impl Iterator<Item = &Path> {
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

    /// Register a directory (including a workspace root) as a known link
    /// target. Rechecks all existing notes that link to this path so their
    /// diagnostics clear.
    pub fn add_dir(&mut self, path: PathBuf) -> IndexDelta {
        self.all_dirs.insert(path.clone());
        let affected = self.recheck_incoming(&path);
        IndexDelta { affected_paths: affected }
    }

    /// Remove a directory from the index. Notes that linked to it now have
    /// broken links and are returned in the delta.
    pub fn remove_dir(&mut self, path: &Path) -> IndexDelta {
        self.all_dirs.remove(path);
        let mut affected = AffectedPaths::default();
        if let Some(incoming) = self.links_to.remove(path) {
            for l in &incoming { affected.insert(l.source_path.clone()); }
        }
        for links in self.links_to.values_mut() {
            links.retain(|l| l.source_path != path);
        }
        self.links_to.retain(|_, v| !v.is_empty());
        affected.insert(path.to_path_buf());
        IndexDelta { affected_paths: affected }
    }

    /// `true` if `path` (already normalized) is a directory registered via
    /// `add_dir`.
    pub fn is_dir_indexed(&self, path: &Path) -> bool {
        self.all_dirs.contains(path)
    }

    /// Directories whose parent is exactly `dir` (immediate children only).
    pub fn child_dirs(&self, dir: &Path) -> impl Iterator<Item = &Path> {
        self.all_dirs
            .iter()
            .filter(move |p| p.parent() == Some(dir))
            .map(PathBuf::as_path)
    }
}
```

`add_dir`/`remove_dir` (v0.17) mirror `add_attachment`/`remove_attachment` —
directories live in their own `all_dirs` set rather than `all_files`, since a
directory has no note/attachment content of its own, only its role as a
resolvable link target. `is_dir_indexed` and `child_dirs` back the
directory-trigger completion branch (folder listing, "accept this folder"
item) in `handle_completion` — see `docs/design/components/handlers.md`.

---

## report()

Builds a serializable snapshot of the whole index for `knap index --json`.
Pure composition of the read methods above — no new resolution logic; a
note's `links` reuses `resolve()`, its `backlinks` reuses `links_to()`, and
the top-level `tags` map reuses `all_tags()`/`notes_by_tag()`.

```rust
#[derive(Serialize)]
pub struct IndexReport {
    pub notes: Vec<NoteSummary>,
    pub tags: BTreeMap<String, Vec<PathBuf>>,
}

#[derive(Serialize)]
pub struct NoteSummary {
    pub path: PathBuf,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub headings: Vec<HeadingSummary>,
    pub links: Vec<LinkSummary>,
    pub backlinks: Vec<PathBuf>,
}

#[derive(Serialize)]
pub struct HeadingSummary {
    pub text: String,
    pub level: u8,
    pub range: lsp_types::Range,
}

#[derive(Serialize)]
pub struct LinkSummary {
    pub target: String,
    pub anchor: Option<String>,
    pub resolved: Option<PathBuf>, // Some(path) if resolved, None if broken
}

impl NoteIndex {
    pub fn report(&self) -> IndexReport { /* ... */ }
    pub fn note_report(&self, path: &Path) -> Option<NoteSummary> { /* ... */ }
}
```

`notes` is sorted by path for deterministic output. `tags` uses a
`BTreeMap` for the same reason — both matter for diffable CI output and
test assertions.

`note_report` (v0.13) returns one note's `NoteSummary` without building
every other note's — the same per-note construction `report()` uses
internally (a private `note_summary` helper both call), just addressable by
path instead of always building the whole workspace's summaries. `None` for
a path the index doesn't have. Used by `knap index <file>` (`src/cli/
index.rs`) to scope its output to a single note.

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

Called from the Protocol Handler after `initialized`. Note files (matching
`extensions`) are fully parsed; all other files are registered in
`all_files` only so attachment links resolve immediately; every directory
walked (plus each root itself) is registered in `all_dirs` (v0.17) so
directory links resolve immediately too. `filter` — a compiled `PathFilter`
(see `docs/design/components/protocol-handler.md`) — is the single
exclude/index authority consulted here and by the three live-index LSP
handlers, so a path excluded at startup stays excluded for the rest of the
session.

```rust
pub(crate) fn build(roots: &[PathBuf], filter: &PathFilter) -> anyhow::Result<(NoteIndex, IndexDelta)> {
    let mut index = NoteIndex::default();
    let mut all_affected = HashSet::new();

    for root in roots {
        let delta = index.add_dir(normalize_path(root));
        all_affected.extend(delta.affected_paths);

        let (files, dirs) = walk_files_and_dirs(root, filter);
        for dir in dirs {
            let delta = index.add_dir(dir);
            all_affected.extend(delta.affected_paths);
        }
        for path in files {
            if filter.is_note(&path) {
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

    Ok((index, IndexDelta { affected_paths: all_affected }))
}
```

`walk_files_and_dirs`/`walk_dir` form a recursive directory walk, returning
`(files, dirs)` — every non-skipped directory visited, plus every accepted
file. It uses `entry.file_type()` (not `path.is_dir()`) so symlinked
directories are never followed, preventing infinite loops. Before recursing
into a subdirectory, `filter.should_skip_dir(root, &entry_path, &name)` is
checked — this prunes both the hardcoded dotfile/`node_modules`/`target`
directories and any directory matching a `knap.toml` `exclude` pattern, so
an excluded subdirectory is never even opened with `read_dir`, and never
pushed to `dirs`. Each candidate file is checked with
`filter.should_index(root, &entry_path)` before being returned, which
applies the same `exclude` patterns to files — so excluded files never
reach `build` and show up as neither a note nor an attachment.

After startup, newly created directories are discovered lazily rather than
by re-crawling: `register_ancestor_dirs` (Protocol Handler, see
`docs/design/components/protocol-handler.md`) climbs from a changed file's
parent upward, calling `add_dir` on each not-yet-known ancestor, and stops
at the first already-registered one.

Each collected file path is passed through `normalize_path` before being
pushed to the result (v0.11.1, #62). Without this, a root given with a
leading `./` (e.g. `knap lint .`) produced paths with a leading `CurDir`
component that never matched the lexically-normalized candidates `resolve()`
computes from link targets, causing every valid relative link under that
root to be misdiagnosed as broken.
