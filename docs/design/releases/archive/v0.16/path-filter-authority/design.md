# v0.16 Design — Path Filter Authority

Covers the stories in the v0.16 release:

| Story   | Feature                                                                                                                                                                   |
| ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Bug #68 | LSP handlers bypass `config.exclude`; consolidate "is this path part of the vault" into one `PathFilter` authority used by the initial crawl and every live-index handler |

---

## Goal

A vault owner who excludes `fixtures/**` (or any other path) in `knap.toml`
expects that exclusion to hold for the entire editor session, not just at
startup. Today it doesn't: opening an excluded file directly, or any tool
touching a watched path under an excluded tree (`git checkout`, a formatter,
`sed -i`), silently re-admits it into the live index — no error, no log, and
none of `exclude`'s promised protection (no completions, no navigation, no
backlinks) actually holds once that happens.

The root cause is structural: "should this path be part of the vault" is
answered by three independent, inconsistent mechanisms —
`index::should_skip_dir`'s hardcoded dotfile/`node_modules`/`target` check,
`config.exclude`'s glob patterns (compiled and consulted only inside
`index::build`), and ad hoc extension filtering in
`on_did_change_watched_files`. `on_did_open`/`on_did_change` consult none of
them. This release fixes the bug by giving all four call sites (the initial
crawl and all three live-index handlers) one shared answer: a `PathFilter`
compiled once from `Config` and threaded everywhere a path is considered for
indexing. As a side effect, `exclude` pattern compilation moves out of
`index::build` (previously recompiled 2-3 times per `knap lint --fix`
invocation) and into `Config`, validated once at config-load time.

---

## Config Changes

New field on `Config` (`src/config/mod.rs`), alongside the existing raw
`exclude: Vec<String>` (kept as-is — still the union of `knap.toml`'s
`exclude` and the CLI/LSP override, still asserted directly in
`config/tests.rs`'s merge tests):

```rust
pub(crate) struct Config {
    // ...existing fields unchanged...
    pub(crate) exclude: Vec<String>,       // raw patterns, as today
    pub(crate) path_filter: PathFilter,    // compiled from `exclude` + `extensions`; the single "is this path part of the vault" authority
}
```

`finalize()` becomes fallible — compiling `exclude`'s glob patterns can fail
on a malformed pattern, and that must surface as a config error, not a panic
or a silent no-op:

```rust
fn finalize(raw: RawConfig, index_roots: Vec<PathBuf>) -> anyhow::Result<Config>
```

`for_lsp` and `for_path` (`src/config/mod.rs`) both already return
`anyhow::Result<Config>`; they change their final line from
`Ok(finalize(raw, index_roots))` to `finalize(raw, index_roots)`.

### `PathFilter` (new type, `src/config/mod.rs`)

The single authority for "does this path belong in the index." Compiled once
per `Config` build, from the same two config values every current mechanism
was already deriving its own partial answer from:

```rust
pub(crate) struct PathFilter {
    /// Compiled `exclude` patterns, plus the `/**`-stripped directory-equivalent
    /// form for each (see `compile`'s doc comment) — moved here verbatim from
    /// `index::build`.
    excludes: Vec<glob::Pattern>,
    /// Note file extensions (e.g. `["md"]`), for the note-vs-attachment split.
    extensions: Vec<String>,
}

impl PathFilter {
    /// Compiles `exclude`'s glob patterns once, validated eagerly (`Err` on a
    /// malformed pattern, never silently ignored). For each pattern ending in
    /// `/**`, also compiles the suffix-stripped directory-equivalent form, so
    /// `dir` itself is recognized as excluded (matching `dir/**`'s intent)
    /// without ever being `read_dir`'d — logic moved verbatim from
    /// `index::build`.
    pub(crate) fn compile(exclude: &[String], extensions: &[String]) -> anyhow::Result<Self> { ... }

    /// Hardcoded skip-dir names — `.`-prefixed, `node_modules`, `target` —
    /// pruned from every crawl regardless of `exclude`. Moved verbatim from
    /// `index::should_skip_dir`.
    fn is_skip_dir_name(name: &str) -> bool { ... }

    fn matches_exclude(&self, relative: &Path) -> bool { ... }

    /// Crawl-only: should this directory be pruned (never `read_dir`'d)? Used
    /// by `index::walk_dir` on directory entries, where `dir_path` is the
    /// entry's full path and `dir_name` its file name.
    pub(crate) fn should_skip_dir(&self, root: &Path, dir_path: &Path, dir_name: &str) -> bool {
        Self::is_skip_dir_name(dir_name)
            || self.matches_exclude(dir_path.strip_prefix(root).unwrap_or(dir_path))
    }

    /// The authoritative check: does `path` (under `root`) belong in the
    /// index? True unless some ancestor directory component is a hardcoded
    /// skip-dir, or the path itself matches an `exclude` pattern. Used by the
    /// crawl's file-handling branch *and* every live-index handler — the same
    /// question, asked the same way, everywhere a path is considered for
    /// indexing.
    pub(crate) fn should_index(&self, root: &Path, path: &Path) -> bool { ... }

    /// Should `path` be parsed as a note (vs. registered as an attachment)?
    pub(crate) fn is_note(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|ext| self.extensions.iter().any(|e| e == ext))
            .unwrap_or(false)
    }
}
```

`should_index` only checks _ancestor_ directory components against
`is_skip_dir_name` — never the leaf itself. This matches the crawl's existing
behaviour exactly (`should_skip_dir` is only ever applied to directory
entries, never file entries — a dotfile like `.hidden.md` sitting directly
under an included root has always been indexed by the crawl). It also fixes
a second, smaller inconsistency the issue didn't call out by name: today's
`on_did_change_watched_files` → `should_skip_path` checks _every_ path
component including the leaf, so a direct edit to `.hidden.md` is silently
dropped by the watched-files handler even though the initial crawl indexes
it. After this change, both paths ask the same question and get the same
answer.

`Config` gains two thin convenience methods so handlers (which only have an
absolute `path`, not a `root`) don't each re-derive which `index_roots` entry
a path lives under:

```rust
impl Config {
    /// Picks the `index_roots` entry `path` lives under (longest-prefix
    /// match, for nested workspace folders) and asks `path_filter`. A path
    /// that isn't under any configured root is never excluded — there's
    /// nothing to match against, so admitting it is the safer default.
    pub(crate) fn should_index(&self, path: &Path) -> bool {
        self.index_roots
            .iter()
            .filter(|r| path.starts_with(r))
            .max_by_key(|r| r.as_os_str().len())
            .is_none_or(|root| self.path_filter.should_index(root, path))
    }

    pub(crate) fn is_note(&self, path: &Path) -> bool {
        self.path_filter.is_note(path)
    }
}
```

---

## Note Index Changes

None. `NoteIndex` itself is untouched — this fix is entirely about which
paths reach `index.index(...)`/`index.add_attachment(...)` in the first
place, not about how the index stores or queries them.

---

## Handler Changes

### `on_did_open` (`textDocument/didOpen`, `src/server/mod.rs`)

Gains an early return when the opened path fails `config.should_index`:

```rust
fn on_did_open(notif: Notification, index: &mut NoteIndex, sender: &Sender<Message>, config: &Config) {
    // ...existing parse of `params`...
    let Some(path) = uri_to_path(&params.text_document.uri) else { return; };
    if !config.should_index(&path) {
        return;
    }
    let note = parser::parse(&path, &params.text_document.text);
    let delta = index.index(note);
    handlers::publish_diagnostics(&delta.affected_paths, index, config, sender);
}
```

### `on_did_change` (`textDocument/didChange`, `src/server/mod.rs`)

Same guard, in the same place (after `uri_to_path`, before `parser::parse`):

```rust
if !config.should_index(&path) {
    return;
}
```

### `on_did_change_watched_files` (`workspace/didChangeWatchedFiles`, `src/server/mod.rs`)

Replaces the standalone `should_skip_path` function (deleted — its one
caller now calls `config.should_index` instead) and the ad hoc extension
check:

```rust
fn on_did_change_watched_files(notif: Notification, index: &mut NoteIndex, sender: &Sender<Message>, config: &Config) {
    let params: DidChangeWatchedFilesParams = /* ...unchanged... */;
    for event in params.changes {
        let Some(path) = uri_to_path(&event.uri) else { continue };
        if !config.should_index(&path) {
            continue;
        }
        let is_note = config.is_note(&path);
        // ...rest unchanged (CREATED/CHANGED/DELETED branches)...
    }
}
```

`should_skip_path` (`src/server/mod.rs:395`) is deleted entirely — it's now
subsumed by `config.should_index`.

---

## Note Index Build Changes

`index::build` (`src/index/mod.rs`) stops taking `extensions`/`exclude`
separately and stops compiling glob patterns itself — it now takes the
already-compiled `PathFilter` and asks it the same `should_index` question
the handlers ask:

```rust
pub fn build(roots: &[PathBuf], filter: &PathFilter) -> anyhow::Result<(NoteIndex, IndexDelta)>
```

`walk_dir` keeps its directory-pruning optimization (never `read_dir`-ing an
excluded directory) via `filter.should_skip_dir`, but its file branch now
calls `filter.should_index(root, &entry_path)` — the exact same method the
handlers call, rather than a locally-duplicated `excluded` check:

```rust
fn walk_dir(dir: &Path, root: &Path, filter: &PathFilter, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        let entry_path = entry.path();
        if ft.is_dir() {
            let name = entry.file_name();
            if !filter.should_skip_dir(root, &entry_path, &name.to_string_lossy()) {
                walk_dir(&entry_path, root, filter, out);
            }
        } else if ft.is_file() && filter.should_index(root, &entry_path) {
            out.push(normalize_path(&entry_path));
        }
        // symlinks: ft.is_symlink() → skip to prevent infinite loops
    }
}
```

`index::build`'s note-vs-attachment split changes from a locally-computed
`is_note` (`path.extension()...extensions.contains(...)`) to
`filter.is_note(&path)`.

`index::should_skip_dir` (`src/index/mod.rs:589`, `pub(crate)`) is deleted;
its logic moves into `PathFilter::is_skip_dir_name` (private). Its other
caller, `src/cli/apply.rs` (two call sites, both walking the filesystem to
find stub-note candidates — not indexing), switches to
`config.path_filter`'s equivalent check via a small `pub(crate)` re-export or
by calling `PathFilter::should_skip_dir` directly with the `Config` it
already has in scope; it doesn't need the `exclude` matching `should_index`
adds, only the hardcoded-name prune, so confirm at implementation time
whether it should move to the full `should_index` (picking up `exclude` too,
arguably desirable — a stub-note search probably _should_ skip excluded
paths) or keep the narrower hardcoded-only check. Default to
`should_index`/`should_skip_dir` parity with the rest of the codebase unless
`apply.rs`'s tests show a reason not to.

### Every `index::build` call site

Every call site passes `&config.path_filter` instead of
`&exts, &config.exclude`:

- `src/server/mod.rs:121`
- `src/cli/index.rs:23`
- `src/cli/rename.rs:88,129,163`
- `src/cli/lint.rs:58,66,77`
- `src/cli/fix.rs:63`

This is also where the "recompiled 2-3 times per `knap lint --fix`"
inefficiency the issue calls out disappears: `lint.rs`'s three `build` calls
now share one already-compiled `config.path_filter` instead of each
recompiling `exclude`'s patterns from scratch.

---

## Testing

### Unit tests

| Test                                                       | What it verifies                                                                                                                        |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `path_filter_should_index_true_for_plain_file`             | A path with no excluded ancestor and no exclude-glob match returns `true`                                                               |
| `path_filter_should_index_false_for_excluded_glob_match`   | A path matching an `exclude` pattern returns `false`, even though it was never pruned during a crawl                                    |
| `path_filter_should_index_false_under_hardcoded_skip_dir`  | A path under `.git/`, `node_modules/`, or `target/` returns `false` regardless of `exclude`                                             |
| `path_filter_should_index_true_for_leaf_dotfile`           | A dotfile leaf (`.hidden.md`) directly under an included root returns `true` — only ancestor dirs are checked, matching crawl semantics |
| `path_filter_should_index_true_for_path_outside_all_roots` | A path that doesn't start with any `index_roots` entry is never excluded by `Config::should_index`                                      |
| `path_filter_should_skip_dir_true_for_hardcoded_name`      | `should_skip_dir` prunes `.git`/`node_modules`/`target` regardless of `exclude`                                                         |
| `path_filter_should_skip_dir_true_for_exclude_match`       | `should_skip_dir` prunes a directory matching an `exclude` pattern                                                                      |
| `path_filter_is_note_true_for_configured_extension`        | `is_note` returns `true` for a path whose extension is in `extensions`                                                                  |
| `path_filter_is_note_false_for_other_extension`            | `is_note` returns `false` for an extension not in `extensions`                                                                          |
| `path_filter_compile_dir_form_from_glob_star_star_suffix`  | `compile` adds the `/**`-stripped directory-equivalent pattern, same as today's `index::build` behaviour                                |
| `path_filter_compile_rejects_malformed_pattern`            | `compile` returns `Err` for an invalid glob pattern instead of panicking                                                                |
| `config_finalize_propagates_path_filter_compile_error`     | `finalize` (via `for_lsp`/`for_path`) surfaces a malformed `exclude` pattern as an `Err`, not a default                                 |

### Integration tests (`tests/exclude.rs`)

| Test                                                       | What it verifies                                                                                                                                          |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lsp_did_open_on_excluded_file_is_not_indexed`             | Sending `didOpen` directly for a path under `exclude` never publishes diagnostics for it and it's absent from `workspace/symbol`/completions              |
| `lsp_did_change_on_excluded_file_is_not_indexed`           | Sending `didChange` directly for a path under `exclude` after it was somehow opened has no indexing effect                                                |
| `lsp_did_change_watched_files_on_excluded_path_is_ignored` | A `didChangeWatchedFiles` `Created`/`Changed` event for a path under `exclude` (simulating `git checkout`/a formatter touching it) never indexes the file |
| `lsp_did_change_watched_files_admits_non_excluded_sibling` | A watched-file event for a path that is _not_ excluded still indexes normally — the fix doesn't over-exclude                                              |

`lsp_initialize_applies_knap_toml_exclude` (existing, `tests/exclude.rs:271`)
keeps passing unchanged — its final assertions (excluded file stays invisible
across `didOpen`/`didChange` of _other_ notes) still hold; only its doc
comment needs updating, since it currently asserts "no handler special-cases
excluded paths," which this release makes untrue.
