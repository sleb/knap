# v0.11 Design — Headless CLI: `knap lsp` / `knap lint` / `knap index`

Covers the stories in the v0.11 release:

| Story  | Feature                                                                               |
| ------ | ------------------------------------------------------------------------------------- |
| US-D04 | `knap lint [path] [--json]` — headless link/anchor/frontmatter diagnostics, no editor |
| US-D05 | `knap index <path> --json` — structured workspace snapshot for agent navigation       |
| US-D06 | `knap lsp` — explicit LSP-server subcommand; bare `knap` no longer starts it          |
| US-D07 | `knap.toml` — project config file, shared by `lsp`/`lint`/`index`                     |

---

## Goal

knap today is only useful headlessly through debug commands (`knap parse`,
`knap index`, `knap check`) plus an LSP server that starts by default with no
subcommand. That serves the human, IDE-driven workflow — diagnostics surface
in the editor as you type. It does not serve coding agents: agents frequently
break markdown links while editing, but the diagnostics that would catch this
only exist inside a live LSP session, and Claude's LSP tool doesn't expose
diagnostics. There is no agent-invokable, single-shot way to ask "did my
edits break any links?" or to get a fast structural view of a workspace
without grepping.

This release reframes knap's CLI around three first-class entry points,
inspired by `tombi-toml/tombi`'s `tombi lsp`/`tombi lint`/`tombi format`
split: `knap lsp` (explicit server start), `knap lint` (headless diagnostics
— the missing piece for agents), and a rewritten `knap index --json`
(structured workspace snapshot). The change that makes both `lint` and a
_correct_ `index` possible is a new shared config source, `knap.toml` —
today `Config` (extensions, frontmatter schema, `new_note_dir`) only exists
via LSP `initializationOptions`, which no headless command can supply.

**Breaking change / release blocker:** bare `knap` (no subcommand) no longer
starts the LSP server — `knap lsp` is now required. `zed-knap` and
`vscode-knap` (separate repos) invoke bare `knap` to launch the server today
and must be updated to `knap lsp` before this version reaches users of those
extensions. That update happens in those repos, not here — this release must
not be tagged/pushed until it's coordinated.

---

## Config Changes

New shared config-loading module, `src/config.rs`, replacing config code
that today lives only in `src/server/mod.rs`. `Config`, `FrontmatterSchema`,
`SchemaField` move there unchanged in shape; only their module path changes
(`crate::server::Config` → `crate::config::Config`).

New on-disk source, optional `knap.toml` at a workspace root:

```toml
extensions = ["md"]
new_note_dir = "inbox"

[frontmatter_schema]
require_frontmatter = false
warn_unknown_keys = false

[frontmatter_schema.fields.title]
required = true

[frontmatter_schema.fields.status]
values = ["draft", "published"]
```

This mirrors `initializationOptions`' fields exactly but in idiomatic
snake_case TOML, via its own deserialize struct (`KnapToml`) — **not** a
reused/shared struct with the existing camelCase `InitOptions`, since that
JSON shape is a wire contract editor extensions already depend on and must
not change.

Two loader entry points in `src/config.rs`:

```rust
pub(crate) fn for_lsp(init_params: &InitializeParams) -> anyhow::Result<Config>
pub(crate) fn for_path(root: &Path, extensions_override: Option<Vec<String>>) -> anyhow::Result<Config>
```

- `for_lsp` — `index_roots` from `workspaceFolders` as today. Looks for
  `knap.toml` in `index_roots[0]`. Layers `initializationOptions` over it
  field-by-field (editor value wins where present, `knap.toml` value used
  otherwise, built-in default (`extensions = ["md"]`, etc.) as the final
  fallback). A malformed `knap.toml` fails `initialize` outright (propagated
  via `?`). A malformed `initializationOptions` payload keeps today's
  existing lenient behavior — `warn!` and fall back to defaults for that
  field — since it's an editor-side concern the user doesn't directly
  control, unlike a `knap.toml` they wrote themselves.
- `for_path` — used by `lint`/`index`. If the given path is a file, its
  parent directory is the root. Looks for `knap.toml` there only (no
  `initializationOptions` layer, no editor involved). `extensions_override`
  is unused today (both callers pass `None`) — reserved for a possible
  future `--ext` flag so a second config-loading path never has to be added
  later.

`find_knap_toml(start: &Path) -> Option<PathBuf>` looks for `knap.toml`
directly in `start` only — no ancestor-directory search, no implicit magic.
`load_knap_toml(path: &Path) -> anyhow::Result<Option<KnapToml>>` returns
`Ok(None)` if the file is absent (defaults apply), `Err` if it exists but is
malformed (bad TOML syntax, or a field of the wrong type) — fail loud, never
silently fall back, per `AGENTS.md`.

The `FrontmatterSchema`-building logic, currently inline in
`Config::from_params` (`src/server/mod.rs:100-115`), is factored into a
shared helper so both the TOML and JSON deserialize paths converge on
identical construction rather than two hand-written copies drifting apart.

---

## Note Index Changes

New serializable report types in `src/index/mod.rs` (or a `report`
submodule), built entirely from the existing query API
(`all_notes()`/`resolve()`/`links_to()`/`all_tags()`/`notes_by_tag()`) — no
new resolution logic:

```rust
#[derive(serde::Serialize)]
pub struct IndexReport {
    pub notes: Vec<NoteSummary>,
    pub tags: std::collections::BTreeMap<String, Vec<PathBuf>>,
}

#[derive(serde::Serialize)]
pub struct NoteSummary {
    pub path: PathBuf,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub headings: Vec<HeadingSummary>,
    pub links: Vec<LinkSummary>,
    pub backlinks: Vec<PathBuf>,
}

#[derive(serde::Serialize)]
pub struct HeadingSummary {
    pub text: String,
    pub level: u8,
    pub range: lsp_types::Range,
}

#[derive(serde::Serialize)]
pub struct LinkSummary {
    pub target: String,
    pub anchor: Option<String>,
    pub resolved: Option<PathBuf>, // Some(path) if resolved, None if broken
}

impl NoteIndex {
    pub fn report(&self) -> IndexReport { ... }
}
```

`report()` iterates `all_notes()` sorted by path (matching today's
`cmd_index` sort), resolves each link via `self.resolve(...)`, and pulls
backlinks/tags via the existing reverse-index methods.

---

## CLI Changes

`src/cli.rs` becomes `src/cli/` — `mod.rs` (clap `Cli`/`Commands` + dispatch),
`lsp.rs`, `lint.rs`, `index.rs`, `parse.rs`, `check.rs`, `version.rs`.
`parse`/`check`/`version` move verbatim, behavior unchanged.

```rust
#[derive(clap::Parser)]
#[command(name = "knap", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    Lsp,
    Lint { #[arg(default_value = ".")] path: PathBuf, #[arg(long)] json: bool },
    Index { path: PathBuf, #[arg(long)] json: bool },
    Parse { path: PathBuf },
    Check,
    Version,
}
```

`command` is required (not `Option`), so bare `knap` triggers clap's
built-in "missing required subcommand" behavior — usage to stderr, exit 2.
No custom code needed for the breaking-change decision above.

### `knap lsp`

Moves today's `main.rs` bootstrap (`Connection::stdio()` +
`knap::server::run(...)`) into `src/cli/lsp.rs`, otherwise unchanged.

### `knap lint` (new)

Resolves config via `config::for_path`, builds the index via
`index::build(&config.index_roots, &exts)`, then calls the existing pure
`handlers::compute_diagnostics(path, &index, &config)` per target file — no
new diagnostic logic. Only files with ≥1 diagnostic appear in output.

Default text output, rustc/clippy-style, 1-indexed `line:col`:

```
notes/index.md:12:3: warning: broken link to 'notes/missing.md'
notes/project.md:1:1: error: missing required frontmatter field 'title'

2 problem(s) in 2 file(s)
```

`--json`:

```json
{
  "diagnostics": [
    {
      "path": "notes/index.md",
      "diagnostics": [
        {
          "range": {
            "start": { "line": 11, "character": 2 },
            "end": { "line": 11, "character": 20 }
          },
          "severity": 2,
          "source": "knap",
          "message": "broken link to 'notes/missing.md'"
        }
      ]
    }
  ],
  "problem_count": 2,
  "file_count": 2
}
```

Per-diagnostic JSON reuses `lsp_types::Diagnostic`'s existing `Serialize`
impl directly — no bespoke DTO for the diagnostic shape itself, only the
wrapping `LintReport`/`FileDiagnostics` structs (defined in
`src/cli/lint.rs`, CLI-output-shaping only, not part of `handlers`/`index`).

Exit code: `0` if `problem_count == 0`, else `1`. No severity threshold in
this release (see Backlog note in `docs/ROADMAP.md`).

### `knap index` (rewritten)

Same `config::for_path` loading as `lint` — this is the fix for the known
bug where today's `cmd_index` hardcodes `extensions: &["md"]` and ignores
`frontmatter_schema`/`new_note_dir` entirely, diverging from what the LSP
would do for the same workspace. Text output keeps today's format
(unchanged). `--json` serializes `NoteIndex::report()` directly:

```json
{
  "notes": [
    {
      "path": "notes/index.md",
      "title": "Home",
      "tags": ["project"],
      "headings": [
        {
          "text": "Overview",
          "level": 1,
          "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 10 }
          }
        }
      ],
      "links": [
        { "target": "notes/missing.md", "anchor": null, "resolved": null }
      ],
      "backlinks": ["notes/other.md"]
    }
  ],
  "tags": { "project": ["notes/index.md"] }
}
```

`src/main.rs` collapses to logging setup + `knap::cli::run()`. The
hand-rolled `args[1]` matching and bare-args-falls-through-to-LSP branch are
deleted outright — no fallback, no alias, per `AGENTS.md` Fearless
Refactoring.

---

## Testing

### Unit tests

| Test (file)                                                     | What it verifies                                                                              |
| --------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `for_path_absent_knap_toml_uses_defaults` (`src/config.rs`)     | No `knap.toml` → built-in defaults                                                            |
| `for_path_loads_knap_toml` (`src/config.rs`)                    | `extensions` from `knap.toml` reflected in `Config`                                           |
| `for_path_malformed_knap_toml_errors` (`src/config.rs`)         | Invalid TOML syntax → `Err`, not defaults                                                     |
| `for_path_knap_toml_wrong_type_errors` (`src/config.rs`)        | `extensions = "md"` (string, not array) → `Err`                                               |
| `for_lsp_knap_toml_and_init_options_merge` (`src/config.rs`)    | `knap.toml` sets `extensions`, `initializationOptions` sets `new_note_dir` — both present     |
| `for_lsp_init_options_overrides_knap_toml` (`src/config.rs`)    | Conflicting `extensions` in both — `initializationOptions` wins                               |
| `for_lsp_malformed_init_options_falls_back` (`src/config.rs`)   | Existing lenient behavior preserved (relocated from `server/tests.rs` if present there today) |
| `for_lsp_malformed_knap_toml_errors` (`src/config.rs`)          | Same fail-loud behavior as `for_path`, via the LSP entry point                                |
| `report_includes_all_notes_sorted_by_path` (`src/index/mod.rs`) | Deterministic ordering                                                                        |
| `report_link_summary_marks_broken_links` (`src/index/mod.rs`)   | `resolved: None` for a broken target                                                          |
| `report_link_summary_marks_resolved_links` (`src/index/mod.rs`) | `resolved: Some(path)` for a valid target                                                     |
| `report_tags_map_groups_by_tag` (`src/index/mod.rs`)            | Notes sharing a tag are grouped together                                                      |

### Integration tests (`tests/cli.rs`)

Spawns the real binary — needs on-disk fixtures under new `tests/fixtures/`
(this repo has no filesystem-fixture tests today; `tests/lsp.rs` uses
synthetic in-memory `didOpen` content, so a new `tempfile` dev-dependency is
needed for the `src/config.rs` unit tests above, while `tests/cli.rs` uses
static checked-in fixture files).

| Test                                        | What it verifies                                                                                                           |
| ------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `no_args_exits_nonzero_and_prints_usage`    | Bare `knap` — non-success exit, usage text on stderr                                                                       |
| `lint_text_output_reports_broken_link`      | `knap lint tests/fixtures/lint_basic` — expected text line, exit code 1                                                    |
| `lint_json_output_parses_and_matches_shape` | `--json` output parses, `problem_count > 0`                                                                                |
| `lint_clean_dir_exits_zero`                 | Fixture with no broken links — exit 0, empty diagnostics                                                                   |
| `lint_respects_knap_toml_extensions`        | Non-default extension picked up only via a `knap.toml` fixture (proves the fix)                                            |
| `lint_malformed_knap_toml_fails_loudly`     | Broken TOML syntax fixture — non-zero exit, parse-error message on stderr                                                  |
| `index_json_output_shape`                   | `knap index --json` — `notes[].headings`, `notes[].links[].resolved`, `tags` present and correct for known fixture content |
| `index_text_output_unchanged_format`        | Human-readable format still matches today's shape                                                                          |
| `version_subcommand_prints_version`         | Existing behavior unchanged                                                                                                |
| `parse_subcommand_still_works`              | New coverage — behavior unchanged, now behind clap                                                                         |
| `check_subcommand_still_works`              | New coverage — behavior unchanged, now behind clap                                                                         |
