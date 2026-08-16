# v0.16 Design — Exclude Paths

Covers the stories in the v0.16 release:

| Story | Feature                                                                      |
| ----- | ---------------------------------------------------------------------------- |
| US-55 | `knap.toml` `exclude` glob patterns; `knap lint`/`knap index --exclude` flag |

---

## Goal

A writer working inside knap's own repository (or any vault that keeps
intentionally-broken test fixtures alongside real notes) gets a diagnostics
page cluttered with findings from files that were never meant to be linted —
`tests/fixtures/**` exists specifically to exercise knap's broken-link
detection, so every one of its "broken" links is a false positive on the Zed
Problems panel. This release adds an `exclude` list of glob patterns, read
from `knap.toml` so both the editor and every headless command agree on what
counts as part of the vault, plus a `--exclude` flag on `knap lint`/`knap
index` for one-off exclusions without touching the config file. Excluded
paths are left out of indexing entirely — not just diagnostics — so they also
never surface in completions, Go to Definition, or Workspace Symbols, the
same as if they didn't exist in the vault at all.

---

## Config Changes

New field added to `InitOptions`, `KnapToml`, `RawConfig`, and `Config`
(`src/config/mod.rs`):

```rust
exclude: Vec<String>,  // glob patterns (relative to the index root) for files/dirs left out of indexing entirely
```

`merge()` gains a matching field. Unlike `extensions`/`new_note_dir`
(`Option<T>::or`, primary wins outright), `exclude` **unions** the two
sources instead of one replacing the other — an editor's
`initializationOptions.exclude` adds to `knap.toml`'s list rather than
hiding it, since exclusions are opt-in noise reduction, not a setting a user
would expect one source to silently override. `finalize()` defaults to an
empty `Vec` (no exclusions) when neither source sets it, same default-empty
posture as `frontmatter_schema`.

`for_path`'s existing `extensions_override: Option<Vec<String>>` parameter
gains a sibling for the CLI's `--exclude`:

```rust
pub(crate) fn for_path(
    path: &Path,
    extensions_override: Option<Vec<String>>,
    exclude_additions: &[String],
) -> Result<Config>
```

`exclude_additions` are **appended** to whatever `knap.toml` already lists
(again a union, not a replace) — `--exclude` is framed as "also skip this,
just for this run," not "here is the full list." `for_lsp` is unchanged in
signature; it has no CLI flag to layer in.

---

## Note Index Changes

`index::build`'s signature gains an `exclude` parameter:

```rust
pub fn build(roots: &[PathBuf], extensions: &[&str], exclude: &[String]) -> (NoteIndex, IndexDelta)
```

Patterns are compiled once at the top of `build` via `glob::Pattern::new`
(new dependency — `glob = "0.3"`; see Testing for the malformed-pattern
error path) and threaded down into `walk_dir`, which gains the same
exclusion check `should_skip_dir` already performs for hidden/build
directories — but pattern-based, and applied to both directories and files:

```rust
fn walk_dir(dir: &Path, root: &Path, excludes: &[glob::Pattern], out: &mut Vec<PathBuf>)
```

For each entry, `entry.path().strip_prefix(root)` gives the path relative to
the index root; that relative path (with `/`-separated components, even on
Windows, so patterns are portable) is tested against every compiled pattern
via `Pattern::matches`. A directory whose relative path matches is not
descended into — mirroring `should_skip_dir`'s early return, so the whole
subtree is skipped in one check rather than filtering each file afterward. A
file whose relative path matches is left out of `out` entirely, so it never
reaches `NoteIndex::index` or `NoteIndex::add_attachment` — no note, no
attachment, no diagnostics, no completion candidate, no backlink target.

This means a pattern that names a directory exactly (`tests/fixtures`, no
wildcard) excludes everything under it, because the directory itself is
checked once, before recursion — no separate "does any ancestor match" walk
is needed. `tests/fixtures/**`, `**/*.draft.md`, and `docs/private/*` are all
valid finer-grained alternatives for cases that don't want to name a whole
directory.

Edge cases:

- **Malformed glob pattern** (e.g. unbalanced `[`) → `glob::Pattern::new`
  errors; `index::build` returns `Result` instead of a bare tuple so this
  propagates instead of panicking or silently ignoring the pattern. (This is
  a breaking change to `build`'s return type — see Testing for the updated
  call sites.)
- **Pattern matches the index root itself** (`exclude = ["."]` or `["**"]`)
  → the root directory's _contents_ are still walked (the exclusion check
  runs on each child, not on `root` itself, since `walk_dir` is first called
  with `root` already assumed included) — matches everything under it,
  producing an empty index. Not special-cased; documented as "don't do
  this" in the `knap.toml` reference in `README.md`.
- **A note outside the vault links to an excluded file** → the link is
  reported as broken (the target genuinely isn't in the index), same as
  linking to any nonexistent file. Excluding a path is a statement that it
  isn't part of the vault, not a promise that links to it still resolve.
- **`--exclude` passed with no `knap.toml`** → `exclude_additions` is the
  whole list; no merge needed, same as today's `extensions_override` path
  when no config file exists.

---

## Handler Changes

None — every handler already reads exclusively from `NoteIndex`, and
excluded files never enter it. No handler needs to know exclusions exist.

---

## Protocol Handler Changes

None — `for_lsp` already resolves `Config.exclude` from `knap.toml`
(unioned with `initializationOptions.exclude`, if the editor sets it) before
`server::mod.rs` calls `index::build`; the server only needs to pass
`&config.exclude` through at its existing `index::build` call site
(`src/server/mod.rs:121`).

---

## Testing

### Unit tests (`src/config/tests.rs`)

| Test                                                | What it verifies                                                                                             |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `for_path_absent_knap_toml_exclude_defaults_empty`  | `config.exclude` is `[]` when `knap.toml` doesn't set it and no `--exclude` is passed                        |
| `for_path_loads_knap_toml_exclude`                  | `exclude = ["a/**"]` in `knap.toml` appears in `config.exclude`                                              |
| `for_path_exclude_additions_appended`               | `exclude_additions` passed to `for_path` are appended to, not replacing, `knap.toml`'s list                  |
| `for_lsp_exclude_unions_knap_toml_and_init_options` | `initializationOptions.exclude` and `knap.toml`'s `exclude` both appear in the result, no duplicates dropped |
| `for_lsp_exclude_default_empty`                     | `config.exclude` is `[]` when neither source sets it                                                         |

### Unit tests (`src/index/tests.rs`)

| Test                                               | What it verifies                                                                                    |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `build_excludes_directory_by_exact_path`           | A file under a directory matching an exact-path pattern (`tests/fixtures`) is absent from the index |
| `build_excludes_directory_by_glob`                 | `tests/fixtures/**` excludes the same subtree as the exact-path form                                |
| `build_excludes_file_by_glob`                      | `**/*.draft.md` excludes matching files while leaving sibling files indexed                         |
| `build_excluded_file_not_registered_as_attachment` | A non-note file matching an exclude pattern is absent from `all_files`, not just unparsed           |
| `build_no_excludes_is_unchanged`                   | Empty `exclude` slice produces the same index as the pre-v0.16 two-argument `build`                 |
| `build_malformed_pattern_errors`                   | An invalid glob pattern returns `Err` instead of panicking                                          |

### Integration tests (`tests/exclude.rs`)

| Test                                       | What it verifies                                                                                                                                                                   |
| ------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lint_excludes_configured_directory`       | `knap lint` on a vault with `knap.toml` `exclude = ["fixtures/**"]` reports no diagnostics from `fixtures/`                                                                        |
| `lint_exclude_flag_adds_to_config`         | `knap lint --exclude other/** path` skips both the flag's pattern and `knap.toml`'s existing ones                                                                                  |
| `index_json_omits_excluded_notes`          | `knap index --json` on a vault with excludes doesn't list excluded files under `notes`                                                                                             |
| `lsp_initialize_applies_knap_toml_exclude` | An in-process LSP session (`knap check`-style harness) started against a vault with `knap.toml` excludes never publishes diagnostics for the excluded file, even after it's edited |
