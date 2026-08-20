# v0.20 Design — Configurable Skip-Dir Defaults

Covers the stories in the v0.20 release:

| Story | Feature                                                                        |
| ----- | ------------------------------------------------------------------------------- |
| US-58 | `knap.toml` `skip_dirs` — the dot-prefix/`node_modules`/`target` crawl defaults are a configurable, overridable list instead of a binary hardcode |

Delivers GitHub issue #69.

---

## Goal

A workspace owner who intentionally keeps a dot-prefixed vault directory
(`.obsidian/`, a deliberately-dotted notes folder), or who vendors
`node_modules`/`target` and actually wants notes indexed there, currently has
no way to say so — `PathFilter::is_skip_dir_name` (`src/config/mod.rs`)
hardcodes the crawl's directory-skip rule into the binary, invisible and
unconditional, answering the same "should this directory be crawled"
question `exclude` already answers through a different, user-facing path.
This release turns that hardcode into a `skip_dirs` config field on
`PathFilter` itself: still `[".*", "node_modules", "target"]` by default —
the same strong default, unchanged for everyone who never touches it — but
now compiled from data a `knap.toml` (or `initializationOptions`) can list,
override, or empty out, the same way `exclude`'s patterns already are.

---

## Config Changes

New field added to `InitOptions`, `KnapToml`, `RawConfig`, and `Config`
(`src/config/mod.rs`):

```rust
skip_dirs: Vec<String>,  // glob patterns matched against a directory's bare name; matching dirs are pruned from the crawl before being opened
```

Unlike `exclude` (unioned across sources — see `merge()`'s existing
special case), `skip_dirs` follows the same precedence as `extensions`/
`new_note_dir`: `primary.skip_dirs.or(fallback.skip_dirs)`, one source wins
outright. A union can only ever add patterns, never remove one — but the
whole point of this field is letting someone opt a legitimately-dotted
directory *out* of the default prune, so `initializationOptions.skip_dirs`
(when the editor sets it) must be able to fully replace `knap.toml`'s list,
not just add to it. `finalize()` defaults to `default_skip_dirs()` — a new
free function returning `vec![".*", "node_modules", "target"]` as owned
`String`s — when neither source sets the field, the same default-when-unset
posture `extensions` already has.

```rust
pub(crate) fn default_skip_dirs() -> Vec<String>
```

`PathFilter::compile` gains a third parameter, compiled the same way
`exclude`'s patterns are — eagerly, `Err` on a malformed pattern rather than
silently ignored:

```rust
pub(crate) fn compile(exclude: &[String], extensions: &[String], skip_dirs: &[String]) -> anyhow::Result<Self>
```

`PathFilter` gains a `skip_dirs: Vec<glob::Pattern>` field alongside the
existing `excludes`/`extensions`. The free function `is_skip_dir_name` is
deleted; its two call sites (`should_skip_dir`, `should_index`'s ancestor
loop) call a new instance method instead:

```rust
fn matches_skip_dir(&self, name: &str) -> bool {
    self.skip_dirs.iter().any(|pattern| pattern.matches(name))
}
```

`Pattern::matches` (not `matches_path_with`) — `name` is always a bare file
name from `DirEntry::file_name`/a single path `Component`, never a
multi-segment path, so there's no separator-crossing concern the way
`exclude`'s `require_literal_separator` option addresses.

`PathFilter` currently derives `Default`, which existing tests
(`cli/apply.rs`'s `copy_tree_copies_files_and_skips_hidden_dirs`, plus every
`PathFilter::default()` used to build a bare `Config` in `handlers.rs`
tests) rely on to skip `.git`-style directories without going through
`Config::finalize`. A derived `Default` would now produce an *empty*
`skip_dirs`, silently changing that test's meaning. `Default` becomes a
hand-written impl instead, so "default" keeps meaning "the same skip-dir
defaults `finalize` would produce," not "no filtering at all":

```rust
impl Default for PathFilter {
    fn default() -> Self {
        // default_skip_dirs()'s patterns are fixed, valid literals — this
        // can't fail.
        PathFilter::compile(&[], &[], &default_skip_dirs())
            .expect("default skip_dirs patterns are valid")
    }
}
```

### Verified: `".*"` reproduces the current dot-prefix rule

The current hardcode is `name.starts_with('.')`, not a literal `.git` name
— the issue's own wording ("ship `.git`... as a default value") glosses
over that distinction, but the design has to preserve it, since `.obsidian`,
`.vscode`, and any other dotfile/dotdir depend on the prefix check, not a
name list. `glob::Pattern::new(".*")` reproduces it exactly: the `glob`
crate has no shell-style "dotglob" special-casing, so `*` matches `git`,
`obsidian`, even a leading-dot-followed-by-another-dot, the same as
`starts_with('.')` would. Confirmed directly against the `glob` crate
already vendored in this workspace:

| `name`         | `starts_with('.')` | `Pattern::new(".*").matches(name)` |
| -------------- | ------------------- | ----------------------------------- |
| `.git`         | true                 | true                                 |
| `.obsidian`    | true                 | true                                 |
| `node_modules` | false                | false                                |
| `a.git`        | false                | false                                |
| `.a.b`         | true                 | true                                 |

---

## Edge cases

- **`skip_dirs = []`** — opts out of pruning entirely, including `.git`.
  Legal, not special-cased — same "you asked for this" posture the existing
  `exclude = ["."]`/`["**"]` root-exclusion edge case takes. Documented as a
  "know what you're doing" option in the `knap.toml` reference, not
  silently guarded against.
- **`skip_dirs = ["node_modules"]`** — drops `target`/`.*` from the prune
  entirely, not just adds `node_modules` on top of the default three: this
  field replaces, it doesn't merge with the built-in default. A user who
  wants to *add* a fourth name lists all four, mirroring how `extensions`
  already works (set the whole list, don't diff against a hidden base).
- **Malformed pattern** (e.g. unbalanced `[`) — `PathFilter::compile`
  returns `Err`, propagated through `finalize`, same as a malformed
  `exclude` pattern today; never silently dropped.
- **A pattern that also happens to match an `exclude` glob** — redundant,
  not conflicting; a directory pruned by either check is pruned, `||` not
  `&&`, unchanged from today's `should_skip_dir`.
- **`skip_dirs` set only in `initializationOptions`, no `knap.toml`** —
  same `Option::or` precedence as `extensions`: the editor's list is the
  only source, used as-is.

---

## Note Index Changes

None beyond what already threads `PathFilter` through — `index::build`,
`walk_dir`, `should_skip_dir`, and `should_index` keep their existing
signatures; only what's compiled inside `PathFilter` changes.

---

## Handler Changes

None — the three live-index LSP handlers already consult `Config`/
`PathFilter` exclusively through `should_index`/`is_note`, which are
unaffected by this change's internals.

---

## Protocol Handler Changes

None — `for_lsp` already resolves `skip_dirs` the same way it resolves
`exclude`/`extensions` today; no new dispatch or capability is involved.

---

## Testing

### Unit tests (`src/config/tests.rs`)

| Test                                                    | What it verifies                                                                                          |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `path_filter_default_skip_dirs_matches_hardcoded_names`  | `PathFilter::default()` (and a filter compiled with `default_skip_dirs()`) skips `.git`, `.obsidian`, `node_modules`, `target` |
| `path_filter_skip_dirs_empty_disables_pruning`            | A filter compiled with `skip_dirs: &[]` does not skip `.git`                                               |
| `path_filter_skip_dirs_custom_pattern`                    | A filter compiled with `skip_dirs: &["vendor".into()]` skips `vendor` but not `node_modules`               |
| `path_filter_skip_dirs_malformed_pattern_errors`          | An invalid glob in `skip_dirs` returns `Err` from `compile`, not a panic or silent drop                     |
| `for_path_absent_knap_toml_skip_dirs_defaults_to_builtin` | `config.skip_dirs` is `default_skip_dirs()` when `knap.toml` doesn't set it                                 |
| `for_path_loads_knap_toml_skip_dirs`                      | `skip_dirs = ["vendor"]` in `knap.toml` appears verbatim in `config.skip_dirs` — not unioned with the default |
| `for_lsp_skip_dirs_init_options_overrides_knap_toml`       | `initializationOptions.skipDirs` fully replaces (not unions with) `knap.toml`'s `skip_dirs` when both are set |
| `for_lsp_skip_dirs_default_when_unset`                     | `config.skip_dirs` is `default_skip_dirs()` when neither source sets it                                     |

Existing `PathFilter::compile(...)` call sites in `config/tests.rs` and
`index/tests.rs` gain a third argument: tests specifically isolating
`exclude`-pattern behaviour (e.g. `path_filter_should_skip_dir_true_for_exclude_match`)
pass `&[]` so the assertion isn't coupled to the skip-dir defaults; the
existing hardcoded-name test is renamed/rewritten as
`path_filter_default_skip_dirs_matches_hardcoded_names` above and switches
to asserting against `default_skip_dirs()` explicitly instead of relying on
an internal, unconfigurable constant. `index/tests.rs`'s `filter()` test
helper passes `default_skip_dirs()` for its new third argument so existing
crawl tests keep seeing the same default pruning behaviour they do today.

### Integration tests (`tests/skip_dirs.rs`)

| Test                                        | What it verifies                                                                                                 |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `index_json_indexes_dotted_dir_when_opted_out` | `knap index --json` on a vault with `knap.toml` `skip_dirs = []` lists notes under a `.notes/` directory that would otherwise be pruned |
| `lint_default_skip_dirs_prunes_node_modules`   | `knap lint` on a vault with a `node_modules/broken.md` reports no diagnostics from it, with no `knap.toml` present (default applies) |
| `index_custom_skip_dirs_replaces_default`      | `knap.toml` `skip_dirs = ["vendor"]` on a vault with both `vendor/` and `node_modules/` prunes only `vendor/`; `node_modules/` is indexed |

---

## Documentation Changes

- `README.md`'s `knap.toml` reference gains a `skip_dirs` example alongside
  `exclude`, with the same "matched against a bare directory name, not a
  path" note `exclude`'s literal-vs-glob doc already models, plus the
  override-not-union precedence callout.
- `docs/ARCHITECTURE.md`'s Configuration section replaces the "resolves the
  hardcoded `.git`/`node_modules`/`target` skip-list" sentence with a
  description of `skip_dirs` as a compiled, configurable field like
  `exclude`.
- `docs/USER_STORIES.md` gains US-58 in the same section as US-55.
