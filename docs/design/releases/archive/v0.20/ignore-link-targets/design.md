# v0.20 Design — Ignore Link Targets

Covers the stories in the v0.20 release:

| Story | Feature                                                                            |
| ----- | ----------------------------------------------------------------------------------- |
| US-59 | Doc-scoped `ignore-link-targets` frontmatter key — per-link broken-link suppression |
| US-60 | `knap.toml` `ignore_link_targets` (+ `--ignore-link-target` flag) — workspace-wide and one-off broken-link suppression by glob |

Delivers GitHub issue #70.

---

## Goal

A writer whose doc has a relative link that intentionally points outside the
current workspace — into a sibling workspace's docs, e.g.
`../../other-repo/docs/thing.md` — currently has no way to tell knap the link
is fine. `exclude` (v0.16) doesn't fit: it's shaped for leaving a local
path/glob out of indexing entirely, not for accepting one specific outbound
reference as valid while everything else about the doc (and the link itself,
if it's ever fixed to point in-workspace) keeps working normally. This
release adds two ways to say "don't flag this link as broken," both acting
purely on `broken-link` diagnostics — indexing, rename, and backlinks are
untouched:

- An `ignore-link-targets` frontmatter key, scoped to the doc it's written
  in, for the common one-off case.
- `knap.toml`'s `ignore_link_targets` (plus a `--ignore-link-target` CLI
  flag on `knap lint`/`knap index`), a workspace-wide glob list, for the
  same external reference recurring across many docs (e.g. every doc in a
  vault links back into one sibling repo).

The frontmatter key and the `knap.toml`/CLI field are deliberately named the
same concept in each surface's own casing convention (`ignore-link-targets`
in YAML frontmatter, `ignore_link_targets` in TOML/`initializationOptions`,
same as `require_frontmatter`/`requireFrontmatter` already do across
TOML/JSON) — not two different-sounding mechanisms for the same idea.

Both list glob patterns matched against the link's raw target text — the
string written between `(` and `)`, before it's resolved against the
workspace — since the whole point is exempting targets that don't resolve to
anything inside the index.

---

## Config Changes

New field added to `InitOptions`, `KnapToml`, `RawConfig`, and `Config`
(`src/config/mod.rs`), mirroring `exclude`'s shape and union precedence:

```rust
ignore_link_targets: Vec<String>,  // glob patterns matched against a link's raw target text; broken-link diagnostics are suppressed for matches
```

`merge()` unions `ignore_link_targets` the same way it unions `exclude` —
`initializationOptions.ignoreLinkTargets` adds to `knap.toml`'s list rather
than replacing it, since this is opt-in noise reduction, not a setting one
source should silently override. `finalize()` defaults to an empty `Vec`
when neither source sets it and compiles the patterns once into
`Config.ignore_link_target_patterns: Vec<glob::Pattern>`, alongside
`path_filter` — eagerly validated (`Err` on a malformed pattern, same
posture as `exclude`), since this is config, not doc content.

```rust
pub(crate) struct Config {
    // existing fields unchanged...
    pub(crate) ignore_link_targets: Vec<String>,           // raw, unparsed — kept for tests, same posture as `exclude`
    pub(crate) ignore_link_target_patterns: Vec<glob::Pattern>,  // compiled once in finalize()
}
```

This is deliberately **not** folded into `PathFilter`: `PathFilter` answers
"does this path belong in the index," a question about workspace-relative
paths under a root. `ignore_link_target_patterns` answers a different
question — "is this specific outbound link target, exactly as written,
something the workspace owner has already accepted" — matched against a raw
link-target string that may not resolve to any path under any root at all.
Keeping it a separate field on `Config` avoids stretching `PathFilter`'s
contract to cover a match target it was never designed around.

`for_path`'s existing `exclude_additions: &[String]` parameter (the CLI's
`--exclude`) gains a sibling for `--ignore-link-target`:

```rust
pub(crate) fn for_path(
    path: &Path,
    extensions_override: Option<Vec<String>>,
    exclude_additions: &[String],
    ignore_link_target_additions: &[String],
) -> Result<Config>
```

`ignore_link_target_additions` are **appended** to whatever `knap.toml`
already lists, same append-not-replace posture as `exclude_additions`.
`for_lsp` is unchanged in signature — it has no CLI flag to layer in, only
`initializationOptions.ignoreLinkTargets`.

### CLI flag

New repeatable flag on `knap lint` and `knap index`
(`src/cli/mod.rs`), mirroring `--exclude`:

```rust
/// Glob pattern to ignore for broken-link diagnostics, in addition to any
/// `ignore_link_targets` entries in `knap.toml`. Matched against a link's
/// raw target text, not a workspace-relative path. Repeatable.
#[arg(long)]
ignore_link_target: Vec<String>,
```

Threaded through `lint::run`/`index::run` the same way `exclude` already is,
down to the `config::for_path(path, None, &exclude, &ignore_link_target)`
call site in each.

---

## Parser Changes

New field on `Frontmatter` (`src/parser/mod.rs`):

```rust
pub struct Frontmatter {
    pub title: Option<String>,
    pub tags: Vec<Tag>,
    pub ignore_link_targets: Vec<String>,  // new — raw glob patterns from the `ignore-link-targets:` key, unresolved
    pub fields: Vec<FrontmatterField>,
}
```

New extraction function, structurally identical to `extract_tags` but
simpler — no ranges are needed since `ignore-link-targets` entries are never
hover-targeted, renamed, or offered as completions, only read once per note
during diagnostics:

```rust
fn extract_ignore_link_targets(content: &str) -> Vec<String>
```

Same three forms `tags:` supports, scanned the same way (first
`ignore-link-targets:` key in the block wins, scan stops there):

- Inline list: `ignore-link-targets: [../../other-repo/**, ../shared/notes.md]`
- Block list: `ignore-link-targets:\n  - ../../other-repo/**`
- Bare scalar: `ignore-link-targets: ../../other-repo/**`

`parse()` gains one line alongside the existing `fm.tags = extract_tags(...)`
assignment:

```rust
fm.ignore_link_targets = extract_ignore_link_targets(content);
```

The key still passes through `extract_frontmatter_fields` like `tags` does
today — a list-valued `ignore-link-targets:` line ends up in `fm.fields`
with `value: None`, same as `tags:` — no special-casing needed there.

Edge cases:

- **No frontmatter block, or block has no `ignore-link-targets:` key** →
  `vec![]`, same empty-default posture as `extract_tags`.
- **Block scalar (`|`, `>`) value** → ignored, `vec![]`, matching `tags:`.
- **Duplicate entries** (`ignore-link-targets: [a, a]`) → both kept;
  deduplication happens implicitly at match time (a target either matches
  one of the patterns or it doesn't), so no dedup pass is needed in the
  parser.

---

## Note Index Changes

None. `NoteIndex::resolve`, `index()`, and every crawl/rename/backlink path
are unchanged — an ignored target is still `ResolvedLink::Broken` in every
sense except which diagnostics that fact produces. This is the same
boundary `exclude` drew against handler concerns, mirrored in the opposite
direction: `exclude` never lets a path enter `NoteIndex` at all;
`ignore-link-targets`/`ignore_link_targets` never touch `NoteIndex` at all.

---

## Handler Changes

### `compute_diagnostics` — US-59, US-60

Gains one new check inside the existing `ResolvedLink::Broken` arm, run
before the diagnostic is pushed:

```rust
pub(crate) fn compute_diagnostics(
    path: &Path,
    index: &NoteIndex,
    config: &crate::config::Config,
) -> Vec<Diagnostic> {
    let Some(note) = index.get_note(path) else {
        return vec![];
    };

    let mut diagnostics = Vec::new();

    for link in &note.md_links {
        match index.resolve(path, &link.target) {
            ResolvedLink::Broken => {
                if is_ignored_link_target(&link.target, note, config) {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    range: link.target_range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!("Link target not found: '{}'", link.target),
                    source: Some(DIAG_SOURCE.to_owned()),
                    code: Some(NumberOrString::String(CODE_BROKEN_LINK.to_string())),
                    ..Default::default()
                });
            }
            ResolvedLink::Found(target_path) => { /* unchanged */ }
        }
    }
    // ...frontmatter-schema block unchanged...
}
```

New private helper, checked in this order (frontmatter first — it's the
narrower, doc-authored scope, so it's the more likely match and the one a
writer edited most recently):

```rust
/// Does `target` (a link's raw destination text) match one of the doc's own
/// `ignore-link-targets` patterns, or one of `knap.toml`'s workspace-wide
/// `ignore_link_targets` patterns? A malformed pattern in doc frontmatter is
/// skipped with a warning log rather than erroring — frontmatter is note
/// content a writer can retype at any time, unlike `knap.toml`, which
/// already fails loud on a malformed `exclude`/`ignore_link_targets`
/// pattern at config-load time.
fn is_ignored_link_target(target: &str, note: &parser::Note, config: &crate::config::Config) -> bool {
    if let Some(fm) = &note.frontmatter {
        for pattern in &fm.ignore_link_targets {
            match glob::Pattern::new(pattern) {
                Ok(p) if p.matches(target) => return true,
                Ok(_) => {}
                Err(e) => warn!("malformed ignore-link-targets pattern '{pattern}': {e}"),
            }
        }
    }
    config
        .ignore_link_target_patterns
        .iter()
        .any(|p| p.matches(target))
}
```

Matching is against `link.target` exactly as parsed — the raw text between
`(` and `)`, minus any `#anchor` (the parser already separates that into
`link.anchor`), and **not** run through `unescape_link_target` first, so a
pattern in `knap.toml`/frontmatter is written the same way the link itself
is written in the doc. `glob::Pattern::matches` (not `matches_path_with`)
is used — a plain string match, no path-separator semantics — since a raw
link target is doc-authored text with `/`-separated components as written,
not a filesystem path to walk.

### `compute_diagnostics_with_suggestions`

No change needed. It calls `compute_diagnostics` and only post-processes
diagnostics that come back with `code == CODE_BROKEN_LINK`; an ignored
target never produces one, so it's automatically excluded from repoint
suggestions too — the same "don't flag this link as broken" boundary the
issue asks for, without a second check.

---

## Protocol Handler Changes

None. `compute_diagnostics` already receives `config` at every call site
(`src/server/mod.rs`), so no new plumbing is needed — `Config` just carries
one more field the existing call already threads through.

---

## Testing

### Unit tests (`src/parser/tests.rs`)

| Test                                                              | What it verifies                                                                 |
| -------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `extract_ignore_link_targets_bare_scalar`                          | `ignore-link-targets: ../a/**` → `vec!["../a/**"]`                                |
| `extract_ignore_link_targets_inline_list`                          | `ignore-link-targets: [../a/**, ../b.md]` → both entries, in order                |
| `extract_ignore_link_targets_block_list`                            | `ignore-link-targets:\n  - ../a/**\n  - ../b.md` → both entries, in order         |
| `extract_ignore_link_targets_absent_key_returns_empty`               | Frontmatter block with no `ignore-link-targets:` key → `vec![]`                   |
| `extract_ignore_link_targets_no_frontmatter_returns_empty`           | No `---` block at all → `vec![]`                                                  |
| `extract_ignore_link_targets_block_scalar_ignored`                  | `ignore-link-targets: \|\n  ...` → `vec![]`, same as `tags:`                      |
| `extract_ignore_link_targets_duplicate_entries_both_kept`            | `ignore-link-targets: [a, a]` → `vec!["a", "a"]`, no dedup in the parser           |
| `parse_populates_frontmatter_ignore_link_targets`                   | `parser::parse` on a note with `ignore-link-targets:` → `note.frontmatter.ignore_link_targets` populated |

### Unit tests (`src/config/tests.rs`)

| Test                                                        | What it verifies                                                                 |
| ------------------------------------------------------------ | ----------------------------------------------------------------------------------- |
| `for_path_absent_knap_toml_ignore_link_targets_defaults_empty` | `config.ignore_link_targets` is `[]` when unset                                 |
| `for_path_loads_knap_toml_ignore_link_targets`                | `ignore_link_targets = ["../a/**"]` in `knap.toml` appears in `config.ignore_link_targets` |
| `for_path_ignore_link_target_additions_appended`              | `--ignore-link-target` additions passed to `for_path` are appended to, not replacing, `knap.toml`'s list |
| `for_lsp_ignore_link_targets_unions_knap_toml_and_init_options` | Both sources' patterns present in the result, no duplicates dropped            |
| `finalize_malformed_ignore_link_targets_pattern_errors`       | An invalid glob in `ignore_link_targets` returns `Err`, same as `exclude`         |

### Unit tests (`src/handlers.rs`)

| Test                                                                | What it verifies                                                                              |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `compute_diagnostics_broken_link_ignored_by_frontmatter_exact_match`   | Doc's `ignore-link-targets: [../out/x.md]`; link to `../out/x.md` (outside index) → no `broken-link` diagnostic |
| `compute_diagnostics_broken_link_ignored_by_frontmatter_glob`         | Doc's `ignore-link-targets: [../out/**]`; link to `../out/x.md` → no `broken-link` diagnostic   |
| `compute_diagnostics_broken_link_not_ignored_by_other_docs_frontmatter` | Doc B has no `ignore-link-targets`; same broken target Doc A ignores → Doc B still gets the diagnostic |
| `compute_diagnostics_broken_link_ignored_by_knap_toml_pattern`        | `config.ignore_link_target_patterns` matches the target, doc has no frontmatter → no diagnostic |
| `compute_diagnostics_broken_link_still_reported_when_no_pattern_matches` | Target matches neither doc nor config patterns → `broken-link` diagnostic unchanged            |
| `compute_diagnostics_found_link_unaffected_by_ignore_patterns`        | A link that resolves (`Found`) is never suppressed, even if it happens to match a pattern       |
| `compute_diagnostics_broken_anchor_diagnostic_unaffected_by_ignore_patterns` | A `Found` link's `broken-anchor` diagnostic is untouched by an unrelated `ignore_link_targets` pattern |
| `compute_diagnostics_malformed_frontmatter_pattern_skipped_not_panicking` | Doc's `ignore-link-targets: ["["]` (malformed glob) → no panic, diagnostic still reported for that target |
| `compute_diagnostics_with_suggestions_omits_suggestions_for_ignored_target` | Same broken target as an ignored-link test, run through `compute_diagnostics_with_suggestions` → no diagnostic and no suggestion data |

### Integration tests (`tests/ignore_link_targets.rs`)

| Test                                                        | What it verifies                                                                                              |
| --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `lint_frontmatter_ignore_link_targets_suppresses_diagnostic`     | `knap lint` on a doc with `ignore-link-targets:` naming its own out-of-workspace target → no `broken-link` diagnostic for that link |
| `lint_knap_toml_ignore_link_targets_suppresses_across_docs`      | `knap.toml` `ignore_link_targets = ["../sibling/**"]`; two docs both link into `../sibling/` → neither reports `broken-link` |
| `lint_ignore_link_target_flag_adds_to_config`                   | `knap lint --ignore-link-target other/** path` suppresses both the flag's pattern and `knap.toml`'s existing ones |
| `lint_ignore_link_targets_does_not_affect_other_broken_links`    | A doc with one ignored link and one genuinely broken in-workspace link → only the second is reported          |
| `index_json_still_reports_ignored_link_as_unresolved`            | `knap index --json` on a vault with `ignore_link_targets` still shows the matching link's `resolved: null` — ignoring is diagnostics-only, not an indexing fact |
| `lsp_initialize_applies_knap_toml_ignore_link_targets`           | An in-process LSP session started against a vault with `knap.toml` `ignore_link_targets` never publishes a `broken-link` diagnostic for a matching target, even after the file is edited |
