# v0.17 Design — Directory Links

Covers the stories in the v0.17 release:

| Story | Feature                                                                                                                         |
| ----- | ------------------------------------------------------------------------------------------------------------------------------- |
| US-56 | Links to an existing directory resolve (no broken-link diagnostic); Go to Definition navigates to it; Find References tracks it |
| US-57 | Path completions let a directory be accepted as the finished link target, not just a step to drill further into                 |

---

## Goal

A writer can link to a whole folder — `[LLDs](../docs/lld/)` — and have it
behave like a link to a file instead of a permanent false-positive
broken-link diagnostic. Today `NoteIndex` only ever tracks file paths in
`all_files`; any link whose target resolves to a directory is unconditionally
`ResolvedLink::Broken`, so it's flagged broken, Go to Definition does
nothing, Find References sees nothing, and path completion never lets a
directory itself be the finished target — only a step to drill through on
the way to a file. This release makes an existing directory a first-class,
resolvable link target: `resolve()` treats it like a file for every purpose
that doesn't require file content (existence, navigation, backlinks), and
completion offers it as something the user can stop on.

**Non-goal:** live directory _deletion_ tracking. The LSP client only
watches files matching `config.extensions` (`workspace/didChangeWatchedFiles`
is registered with `**/*.{ext}` globs — see `docs/ARCHITECTURE.md` § File
Change Notifications); it never reports directory create/delete events. This
release makes directory creation visible incrementally by piggybacking on
file events (below), but a directory whose last file was just deleted, or an
empty directory removed outright, stays resolvable until the next full
crawl (server restart, or a fresh `knap lint`/`knap index` invocation, which
always crawls fresh). This is a stale-positive, not a crash: a link that
used to be valid keeps resolving for the rest of the session instead of
correctly flipping to broken. Symmetrically, a brand-new _empty_ directory
(no files inside yet) isn't visible mid-session until either a file lands in
it or the server restarts — the initial crawl walks every directory
regardless of contents, so this gap only affects directories created live.

---

## Note Index Changes

New field on `NoteIndex`, parallel to `all_files`:

```rust
/// Every directory in the workspace that is known to exist — from the
/// initial crawl (every directory walked, including empty ones) and from
/// live file events (an ancestor directory is registered the first time a
/// file appears under it). Kept separate from `all_files` so directories
/// never show up in `all_attachment_paths()` (which is `all_files` minus
/// `by_path`).
all_dirs: HashSet<PathBuf>,
```

`resolve()`, `index()`'s `links_to` population (step 3), and
`recheck_incoming()` each currently ask "does this candidate path exist" via
`self.all_files.contains(&candidate)`. All three now ask a small shared
helper instead, so the "does this path resolve to something real" check has
one definition instead of three:

```rust
/// Does `path` refer to something this index knows about — a file or a
/// directory? The single existence check `resolve()`, `index()`, and
/// `recheck_incoming()` all defer to.
fn target_exists(&self, path: &Path) -> bool {
    self.all_files.contains(path) || self.all_dirs.contains(path)
}
```

`resolve()` changes its one `if` to call it:

```rust
pub fn resolve(&self, source: &Path, target: &str) -> ResolvedLink {
    // ...unchanged up to the candidate computation...
    if self.target_exists(&candidate) {
        ResolvedLink::Found(candidate)
    } else {
        ResolvedLink::Broken
    }
}
```

`index()` step 3 and `recheck_incoming()` swap their
`self.all_files.contains(&candidate)` checks for `self.target_exists(&candidate)`
the same way. No other change to either — `links_to` is already just
`HashMap<PathBuf, Vec<LocatedLink>>`, keyed by whatever `resolve()` produced,
so a directory target populates it exactly like a file target does. This is
what makes Find References on a directory work for free once `resolve()`
recognizes it.

### `add_dir()` / `remove_dir()`

Mirrors `add_attachment()`/`remove_attachment()` exactly, operating on
`all_dirs` instead of `all_files`:

```rust
/// Register a directory in the workspace. Notes that link to this path and
/// were previously broken may now resolve.
pub fn add_dir(&mut self, path: PathBuf) -> IndexDelta {
    self.all_dirs.insert(path.clone());
    let affected = self.recheck_incoming(&path);
    IndexDelta { affected_paths: affected }
}

/// Remove a directory from the workspace. Notes that linked to it now have
/// broken links and are returned in the delta. Not called from any live
/// event this release (see Goal's Non-goal) — present for symmetry and so
/// the stale-positive behavior above is exercised directly by a unit test
/// rather than only implied by it.
pub fn remove_dir(&mut self, path: &Path) -> IndexDelta {
    self.all_dirs.remove(path);
    let mut affected = AffectedPaths::default();
    if let Some(incoming) = self.links_to.remove(path) {
        for l in &incoming {
            affected.insert(l.source_path.clone());
        }
    }
    affected.insert(path.to_path_buf());
    IndexDelta { affected_paths: affected }
}

/// Is `path` a directory this index knows about? Used by completion to
/// gate the "link to this folder" item on a real, indexed directory.
pub fn is_dir_indexed(&self, path: &Path) -> bool {
    self.all_dirs.contains(path)
}

/// Immediate subdirectories of `dir` — every entry in `all_dirs` whose
/// parent is exactly `dir`. Replaces completion's previous file-derived
/// directory inference (which only surfaced a directory if it contained at
/// least one note or attachment); `all_dirs` is a superset, so this also
/// surfaces empty directories.
pub fn child_dirs(&self, dir: &Path) -> impl Iterator<Item = &Path> {
    self.all_dirs
        .iter()
        .filter(move |d| d.parent() == Some(dir))
        .map(PathBuf::as_path)
}
```

`recheck_incoming()` itself needs no code change (see above — it now goes
through `target_exists`), but its doc comment's "new file" language now
covers "new file or directory."

### Initial crawl (`index::build`)

`walk_dir` currently returns files only. It now also collects every
directory it doesn't skip, so `build()` can register them before indexing
any file — registering directories first means a link to one resolves
correctly the moment its linking note is indexed, with no reliance on
`recheck_incoming` to catch up:

```rust
fn walk_dir(
    dir: &Path,
    root: &Path,
    filter: &PathFilter,
    out_files: &mut Vec<PathBuf>,
    out_dirs: &mut Vec<PathBuf>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        let entry_path = entry.path();
        if ft.is_dir() {
            let name = entry.file_name();
            if !filter.should_skip_dir(root, &entry_path, &name.to_string_lossy()) {
                out_dirs.push(normalize_path(&entry_path));
                walk_dir(&entry_path, root, filter, out_files, out_dirs);
            }
        } else if ft.is_file() && filter.should_index(root, &entry_path) {
            out_files.push(normalize_path(&entry_path));
        }
    }
}
```

```rust
pub(crate) fn build(roots: &[PathBuf], filter: &PathFilter) -> anyhow::Result<(NoteIndex, IndexDelta)> {
    let mut index = NoteIndex::default();
    let mut all_affected = HashSet::new();

    for root in roots {
        let (files, dirs) = walk_files_and_dirs(root, filter);
        for d in std::iter::once(normalize_path(root)).chain(dirs) {
            let delta = index.add_dir(d);
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

The workspace root itself (`normalize_path(root)`) is registered too — a
link like `[Home](..)` from a first-level note is a directory link like any
other.

---

## Protocol Handler Changes

### Live directory discovery (`server/mod.rs`)

A new private helper, called after every `index.index(note)` and
`index.add_attachment(path)` in the three live-index sites
(`on_did_open`, `on_did_change`, `on_did_change_watched_files`):

```rust
/// After indexing a file at `path`, register any of its ancestor
/// directories (up to and including the containing index root) that the
/// index doesn't already know about. Stops climbing as soon as it reaches
/// an already-known directory — everything above that must already be
/// registered too, so the common case (editing an already-indexed file)
/// costs one `HashSet` lookup and nothing more.
fn register_ancestor_dirs(
    path: &Path,
    roots: &[PathBuf],
    index: &mut NoteIndex,
) -> HashSet<PathBuf> {
    let mut affected = HashSet::new();
    let Some(root) = roots
        .iter()
        .filter(|r| path.starts_with(r))
        .max_by_key(|r| r.as_os_str().len())
    else {
        return affected; // config.should_index() already required a root match
    };

    let mut dir = path.parent();
    while let Some(d) = dir {
        if index.is_dir_indexed(d) {
            break;
        }
        let delta = index.add_dir(d.to_path_buf());
        affected.extend(delta.affected_paths);
        if d == root.as_path() {
            break;
        }
        dir = d.parent();
    }
    affected
}
```

`register_ancestor_dirs`'s `affected` extends the same `affected_paths` set
each call site already passes to `handlers::publish_diagnostics` — a
directory appearing mid-session can flip other notes' broken-link
diagnostics to resolved, exactly like `add_attachment` already does for a
new file.

`on_did_change_watched_files`'s `Deleted` branches are intentionally
unchanged — no `remove_dir` call is wired to any event this release (see
Goal's Non-goal).

No capability advertisement changes — no new LSP method is introduced.
`definition_provider`/`references_provider`/`completion_provider` are
already advertised; this release changes what they resolve, not whether
they're offered.

---

## Handler Changes

### Go to Definition / Find References — no code change

`handle_definition` and `handle_references` (`src/handlers.rs`) are both
already pure functions of `index.resolve()` and `index.links_to()` — neither
touches `all_files` or `by_path` directly for the target side. Once
`resolve()` recognizes a directory, `handle_definition` returns a `Location`
pointing at it (`GotoDefinitionResponse::Scalar`, `range: Range::default()`
— same "no heading to navigate to" fallback a same-file no-anchor link
already uses) and `handle_references` returns its `links_to` backlinks
unchanged. A directory link with an anchor (`[x](docs/#foo)`) already falls
through the existing `index.get_note(&target_path)` → `None` → "no heading
matched" path the same way an anchor on an attachment link does today — no
new branch needed.

### Diagnostics (`compute_diagnostics`) — no code change

Same reasoning: the `ResolvedLink::Found(target_path)` arm's anchor check
already treats `index.get_note(&target_path) == None` as "anchor not
found," which is the correct diagnostic for `[x](docs/#foo)` (a directory
has no headings). A directory link with no anchor produces no diagnostic,
same as a resolved file link with no anchor.

### Code Actions (`handle_code_actions`) — no code change

Same `target_note = index.get_note(&target_path)` → `None` →
`target_note.iter().flat_map(...)` (empty) path: a directory link with a
broken anchor offers zero "Change anchor to..." actions rather than
crashing or offering nonsense. Consistent with the existing attachment case.

### Completion (`handle_completion`) — directory trigger, `src/handlers.rs`

Two changes inside the `check_dir_trigger` branch.

**1. Directory items now come from the index, not from file paths.** The
existing `dirs: BTreeSet<String>` is inferred today by splitting every note
and attachment path's relative path under `base_dir` — a directory only
appears if it (transitively) contains a file. Replace that inference with
`index.child_dirs(&base_dir)`, which is sourced from `all_dirs` and so also
surfaces empty directories:

```rust
let dirs: std::collections::BTreeSet<String> = index
    .child_dirs(&base_dir)
    .filter_map(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
    .collect();
```

The rest of tier 0's construction (`sort_text: "0_{dir_name}"`, label
`"{dir_name}/"`, re-triggers on `/`) is unchanged.

**2. A new "accept this folder" item** — added once, after tier 0's loop,
only when the user has already drilled into a directory
(`base_dir != note_dir`) and that directory is real
(`index.is_dir_indexed(&base_dir)`):

```rust
if base_dir != note_dir && index.is_dir_indexed(&base_dir) {
    let full_rel = relative_path(note_dir, &base_dir) + "/";
    let dir_name = base_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| full_rel.trim_end_matches('/').to_string());
    items.push(CompletionItem {
        label: dir_name.clone(),
        kind: Some(CompletionItemKind::FOLDER),
        detail: Some("Link to this folder".to_string()),
        filter_text: Some(dir_name),
        sort_text: Some("0!".to_string()), // sorts before "0_..." drill-in items
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range: replace_range,
            new_text: full_rel,
        })),
        ..Default::default()
    });
}
```

Unlike a tier-0 drill-in item, `new_text` is the already-typed `base_dir`
path itself — selecting it re-inserts the identical text (no further
retrigger), which is what lets the user stop here instead of being pushed
one level deeper. Label carries no trailing slash (`"lld"` vs. the drill-in
item's `"lld/"`), which is the visual cue distinguishing "stop here" from
"keep going." `sort_text: "0!"` sorts before every `"0_{dir_name}"` drill-in
item (`!` is `0x21`, `_` is `0x5F`) so it appears first within the folder
tier, for editors that respect `sort_text`.

Tier 1 and tier 2 (file items) are unchanged.

---

## Testing

### Unit tests

| Test                                                              | File                 | What it verifies                                                                                                               |
| ----------------------------------------------------------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `resolve_existing_dir_found`                                      | `src/index/tests.rs` | `resolve()` returns `Found` for a target that normalizes to a registered directory                                             |
| `resolve_nonexistent_dir_broken`                                  | `src/index/tests.rs` | `resolve()` returns `Broken` for a directory-shaped target that was never registered                                           |
| `index_populates_links_to_for_dir_target`                         | `src/index/tests.rs` | indexing a note whose link targets a known directory populates `links_to` for that directory                                   |
| `add_dir_resolves_previously_broken_link`                         | `src/index/tests.rs` | `add_dir` on a path a note already links to flips that note into `affected_paths` (mirrors `attachment_recheck_heals_link`)    |
| `remove_dir_breaks_links`                                         | `src/index/tests.rs` | `remove_dir` on a linked-to directory returns the linking note's path in `affected_paths`                                      |
| `is_dir_indexed_true_for_known_dir`                               | `src/index/tests.rs` | `is_dir_indexed` returns `true` only for a directory registered via `add_dir`/`build`                                          |
| `child_dirs_returns_immediate_children_only`                      | `src/index/tests.rs` | `child_dirs` returns only directories whose parent is exactly the queried directory, not deeper descendants                    |
| `child_dirs_includes_empty_directory`                             | `src/index/tests.rs` | an empty directory registered via `add_dir` appears in its parent's `child_dirs`                                               |
| `build_registers_every_directory_including_empty`                 | `src/index/tests.rs` | `build()` over a fixture tree with an empty subdirectory registers it as a known directory                                     |
| `build_registers_workspace_root_as_dir`                           | `src/index/tests.rs` | `build()` registers each root itself so `[Home](..)`-shaped links to the root resolve                                          |
| `handle_definition_directory_link_returns_location`               | `src/handlers.rs`    | Go to Definition on a link to an existing directory returns a `Location` at that directory, `Range::default()`                 |
| `handle_definition_directory_link_with_anchor_returns_no_heading` | `src/handlers.rs`    | a directory link with an anchor falls back to `Range::default()` (no headings to match)                                        |
| `handle_references_directory_link_returns_backlinks`              | `src/handlers.rs`    | Find References from a link to a directory returns every other note linking to that same directory                             |
| `compute_diagnostics_no_broken_link_for_existing_dir`             | `src/handlers.rs`    | a link to an existing directory produces no `broken-link` diagnostic                                                           |
| `compute_diagnostics_broken_anchor_on_dir_link`                   | `src/handlers.rs`    | `[x](docs/#foo)` produces a `broken-anchor` diagnostic (directories have no headings)                                          |
| `handle_code_actions_no_anchor_fix_offered_for_dir_link`          | `src/handlers.rs`    | a broken-anchor directory link offers zero "Change anchor to..." actions (mirrors the attachment case)                         |
| `completion_dir_trigger_lists_child_dirs_including_empty`         | `src/handlers.rs`    | directory-trigger completion includes a subdirectory with no files in it                                                       |
| `completion_dir_trigger_offers_accept_item_when_drilled_in`       | `src/handlers.rs`    | completing inside `docs/lld/` includes a FOLDER item labeled `"lld"` (no trailing slash) whose `new_text` equals `"docs/lld/"` |
| `completion_dir_trigger_no_accept_item_at_note_own_dir`           | `src/handlers.rs`    | completing at the note's own directory (no path segment typed yet) offers no "accept this folder" item                         |
| `completion_dir_trigger_no_accept_item_for_unindexed_partial`     | `src/handlers.rs`    | a typed-but-nonexistent directory prefix offers no "accept this folder" item                                                   |
| `register_ancestor_dirs_stops_at_known_ancestor`                  | `src/server/mod.rs`  | climbing halts at the first already-known directory and doesn't call `add_dir` above it                                        |
| `register_ancestor_dirs_registers_new_nested_dirs`                | `src/server/mod.rs`  | a file created two levels under a previously-unknown directory registers both new ancestor directories                         |

### Integration tests (`tests/lsp.rs`)

| Test                                              | What it verifies                                                                                                                                                                              |
| ------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `directory_link_resolves_end_to_end`              | a workspace with a note linking to an existing subdirectory reports no diagnostics after `initialize`                                                                                         |
| `directory_link_definition_and_references`        | `textDocument/definition` on the directory link returns the directory's `Location`; `textDocument/references` from another note linking to the same directory returns both                    |
| `directory_created_live_resolves_without_restart` | a `didChangeWatchedFiles` `Created` event for a new note under a brand-new nested directory clears a previously-broken directory-link diagnostic in another open note, with no server restart |
