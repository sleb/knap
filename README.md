# knap

[![Crates.io](https://img.shields.io/crates/v/knap.svg)](https://crates.io/crates/knap)
[![docs.rs](https://img.shields.io/docsrs/knap)](https://docs.rs/knap)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Keeps linked Markdown docs correct — for the human writing them and the
agent editing alongside — without the grep-and-pray that usually goes with
renaming a file in a big linked workspace. Built on standard Markdown syntax
with no proprietary extensions.

## Overview

A workspace of linked Markdown docs breaks quietly: a file gets renamed and
every link to it goes stale, a heading gets reworded and its anchors dangle,
a tag gets retired in one file but not the other forty. Finding and fixing
that by hand means grepping for every possible spelling of a path, opening
each hit, and hoping you didn't miss one — for a human, tedious; for a
coding agent, it burns tool calls and context on work that's mechanical, not
hard. knap replaces that with a real index: `[text](path.md)` links,
`#anchor`s, and frontmatter tags are parsed once and kept correct as both a
human and a coding agent edit the same files — the same index, the same
diagnostics, the same atomic refactors, whichever one is holding the pen. A
rename that would be a multi-file grep-sed-verify loop by hand is one
command, done exactly once, with nothing missed. Docs stay plain Markdown
that renders correctly anywhere — GitHub, static site generators, other
editors — without knap present; the tooling supplies the correctness, the
files stay clean. This works for any workspace of linked docs: a wiki, a
knowledge base, a docs site, a spec repo, a personal note-taking vault —
knap doesn't care what you call the files, only that they link to each
other. See [Architecture](docs/ARCHITECTURE.md) for the full design tenets.

knap ships as a single binary with two faces built on the same engine:

- **`knap lsp`** — for the human — a
  [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
  server that brings IDE-quality linking, navigation, and refactoring to any
  LSP-compatible editor: Neovim, VS Code, Helix, Zed, and others. Dedicated
  extensions are available for [VS Code](https://github.com/sleb/vscode-knap)
  and [Zed](https://github.com/sleb/zed-knap).
- **`knap lint` / `knap index` / `knap rename-*` / `knap
apply`** — for the agent — the same checks and refactors, headlessly from
  the command line, so a coding agent without an editor in the loop can
  verify its own edits, rename with the same guarantees a human gets
  from the LSP, and apply a whole batch of chosen changes in one call.

Both faces share one indexing and configuration core: `src/config/mod.rs` loads
`initializationOptions` or `knap.toml` the same way for every entry point, and
the doc index — files, headings, links, backlinks, tags — is built once and
reused across all commands. A human renaming a file in their editor and an
agent running `knap rename-file` get the identical rewrite; a diagnostic the
LSP would have squiggled is the same one `knap lint` reports.

## Install

```bash
cargo install knap
```

Requires Rust stable; installs to `~/.cargo/bin`. Pre-built binaries and
build-from-source instructions are in [Getting Started](docs/GETTING_STARTED.md#1-install-the-server).
Zed and VS Code have dedicated extensions ([zed-knap](https://github.com/sleb/zed-knap),
[vscode-knap](https://github.com/sleb/vscode-knap)) that manage or locate the
binary for you.

## Performance

The efficiency case in one number: a manual A/B benchmark ([full protocol
and trial log](docs/design/experiments/agentic-efficiency-benchmark.md))
compares a coding agent editing a 200-note linked Markdown vault with knap
against the same agent editing it with only `grep`/`sed`/`Read`/`Edit`, on
an identical 7-step rename/retag/split/fix task run repeatedly against
seeded, ground-truth-checked defects. The clearest official (N≥3) run:

| Metric      | Baseline | knap-assisted | Δ                  |
| ----------- | -------- | -------------- | ------------------ |
| Wall time   | 348.5s   | 199.6s         | **42.7% faster**   |
| Tokens      | 66,099   | 45,365         | **31.4% fewer**    |
| Tool calls  | 113      | 52             | **54.0% fewer**    |

with every knap-assisted run beating every baseline run on every metric
(zero overlap) — fewer tool calls because a rename is one command instead of
a grep-sed-verify loop, and fewer tokens because that one command doesn't
need to paste file contents into context to reason about them. Correctness
— links/anchors left broken, and whether each seeded fix landed on the
actual right target, not just something that happened to resolve — is
checked independently every trial and has mostly tied at zero; two trials
also caught real regressions this way (a plausible-but-wrong repoint, and a
range-miscopy that corrupted a link), both since fixed and confirmed closed
in later trials. Treat the numbers as directional, not a statistically
powered study — see the doc's
[Threats to validity](docs/design/experiments/agentic-efficiency-benchmark.md#threats-to-validity-call-these-out-alongside-results-dont-bury-them)
section for what to discount and why.

## LSP server

`knap lsp` starts the server on stdio. This is what your editor's LSP client
should invoke — see your editor's extension docs for how to point it at the
`knap` binary. (Prior to v0.11, bare `knap` did this; that fallback is gone —
editor extensions must invoke `knap lsp` explicitly.) The index stays live as
files change, so there's no restart needed after edits.

### Linking & completions

- **Path completions** — type `(` inside a Markdown link for a directory
  browser; drill into subfolders one level at a time (including empty ones),
  or type any filename segment to jump directly to any doc or attachment in
  the workspace (images and PDFs included); once drilled into a folder, an
  item lets you link to that folder itself instead of picking a file inside it
- **Directory links** — link to a folder (`[LLDs](docs/lld/)`) and it
  resolves like a link to a file: no `broken-link` diagnostic, Go to
  Definition navigates to the folder, Find References tracks every doc
  linking to it
- **Anchor completions** — type `#` after a file path to pick from that file's
  headings, or `[text](#` to pick from the current file's headings; inserts the
  GFM slug automatically (`## My Section` → `my-section`)

### Navigation

- **Go to Definition** — jump to the linked doc; navigates to the heading line
  when an anchor is present (`[text](doc.md#heading)` or `[text](#heading)` for
  same-file headings)
- **Document Symbols** — outline of every heading in the current file, jumpable
  from your editor's symbol panel
- **Workspace Symbols** — fuzzy-search headings across the entire workspace

### Frontmatter schema

- **Key completions** — define allowed keys in `frontmatterSchema`; typing in
  a frontmatter key position offers unused schema keys as `FIELD` items
- **Value completions** — when a key has a `values` list, typing after the `:`
  offers allowed values as `VALUE` items with prefix filtering
- **Schema diagnostics** — warnings for required keys that are absent, values
  outside the allowed list (exact-case), unknown keys (opt-in via
  `warnOnUnknownKeys`), and docs missing a frontmatter block entirely (opt-in
  via `requireFrontmatter`)
- **`ignore-link-targets`** — a doc's own `ignore-link-targets:` frontmatter
  key lists glob patterns (bare scalar, inline list, or block list); any of
  that doc's links matching one is never reported as `broken-link`, on top
  of whatever `knap.toml`'s `ignore_link_targets` and `--ignore-link-target`
  already suppress

### Tags

- **Tag completions** — inside a frontmatter `tags:` value, your workspace tag
  index appears as a pick list; already-used tags are excluded and prefix
  filtering narrows results as you type
- **Find References on a tag** — shows every doc that carries the tag, with
  each result pointing directly at the tag range
- **Go to Definition on a tag** — same set of locations, letting you jump to
  any doc using the tag
- **Workspace Symbols includes tags** — tags appear alongside headings in the
  symbol search with `SymbolKind::KEY` so editors can style them distinctly

### Editor experience

- **Folding ranges** — heading sections and fenced code blocks fold in any
  editor that supports `textDocument/foldingRange`
- **Selection range** — smart expand/contract grows the selection through
  word → link → paragraph → heading section → document
- **Inlay hints** — linked docs with a `title:` frontmatter field show the
  title inline next to the link path (e.g. `-> My Doc`)

### Backlinks & code lens

- **Backlinks code lens** — a `↑ N backlinks` annotation above the first line
  of any doc with incoming links; click to open the References panel in VS Code
- **Heading anchor-link lens** — headings that are anchor targets show
  `↑ N anchor link(s)` counting both same-file and cross-file `#slug` references

### Finding references

- **Find References** — every standard Markdown link pointing to the current
  file; on a heading, collects same-file bare anchors and cross-file anchors to
  that heading; or every doc using a tag when the cursor is on a tag value

### Refactoring

- **Rename a file** — all incoming and outgoing links rewritten atomically via
  `workspace/willRenameFiles`
- **Rename a heading** — all `[text](doc.md#old-slug)` anchor links updated in
  place to the new slug
- **Rename a tag** — every frontmatter occurrence of the tag across the workspace
  updated atomically; rename dialog pre-fills with the current tag text; all
  three YAML tag forms supported (bare scalar, inline list, block list)

### Diagnostics & fixes

- **Broken link diagnostics** — warnings for links to missing files, cross-file
  missing anchors, and same-file bare anchors that don't match any heading;
  attachment links (images, PDFs) resolve against the full workspace
- **Quick Fix** — create a missing file from a broken link, or pick a valid
  heading to replace a broken anchor; both via standard `textDocument/codeAction`

## Linter (`knap lint`)

Headless diagnostics — the same broken-link, broken-anchor, and frontmatter
checks the editor shows, without a running LSP session. Useful in CI or for
an agent to check whether its edits broke any links.

```
$ knap lint .
docs/index.md:12:3: warning: broken link to 'docs/missing.md'

1 problem(s) in 1 file(s)
```

`path` defaults to the current directory; pass a single file to lint just
that file. `--json` emits a machine-readable report (`diagnostics`,
`problem_count`, `file_count`, `blocking_count`). Every diagnostic carries a
stable `code` (`broken-link`, `broken-anchor`, `missing-frontmatter`,
`missing-required-field`, `invalid-field-value`, `unknown-field`) — branch
on `code`, not on the message text, when scripting against `--json` output.
Exit code is `0` if no problems were found, `1` otherwise.

Usage: `knap lint [path] [--json] [--fail-on <severity>] [--since <git-ref>] [--suggest [N]] [--exclude <glob>]... [--ignore-link-target <glob>]...`

- `--exclude <glob>` — glob pattern (relative to `path`) to leave out of
  linting entirely; repeatable. Adds to, rather than replaces, any
  `exclude` patterns already set in `knap.toml`. `--ignore-link-target
  <glob>` is the diagnostics-only counterpart — repeatable, and adds to
  `knap.toml`'s `ignore_link_targets` rather than replacing it — a matching
  link target is still indexed and still resolved, it just never produces a
  `broken-link` diagnostic.
- `--fail-on <severity>` — minimum severity that causes a non-zero exit
  (`error`, `warning` (default), `info`, or `hint`). `blocking_count` in
  `--json` output counts diagnostics at or above this threshold; exit code
  follows `blocking_count`, not `problem_count`. Every diagnostic knap
  emits today is `warning`, so `--fail-on warning` (the default) preserves
  today's exit behavior exactly, and `--fail-on error` always passes until
  some future diagnostic is promoted to `error`.
- `--since <git-ref>` — scope linting to files changed since `<git-ref>`
  (tracked diffs plus untracked new files), instead of the whole workspace.
  Requires a git repository; errors otherwise.
- `--suggest [N]` — attach up to `N` ranked candidate fixes to each
  `broken-link`/`broken-anchor` diagnostic's `data` field in `--json` output,
  bare `--suggest` defaults to 3. Ranking blends two distance signals: how
  close the broken target/slug is to each candidate's path or heading
  (`distance`), and how close the link's own visible text is to each
  candidate's name (`text_distance`, `null` when the link has no usable
  text). `--suggest` shows the whole ranked list rather than collapsing it
  to one answer, so an agent already running `knap lint --json` to verify an
  edit gets every candidate for the ambiguous cases in the same call. When
  the two signals disagree on
  the top candidate, the diagnostic's `data` also carries
  `"text_mismatch": true` — a signal that the top-ranked candidate may be
  wrong even though it looks unambiguous by path distance alone:
  ```
  $ knap lint . --json --suggest
  { "...": "...",
    "code": "broken-link",
    "data": { "suggestions": [
      { "target": "reference/config.md", "distance": 8, "text_distance": 0 },
      { "target": "reference/cache.md", "distance": 11, "text_distance": 6 }
    ] } }
  ```

## Indexer (`knap index`)

Builds and prints the doc index for a directory: files, headings, links
(with resolved/broken status), backlinks, and tags. `--json` emits a
structured snapshot — handy for an agent to get a fast structural view of a
workspace without grepping.

Usage: `knap index <path> [--json] [--exclude <glob>]... [--ignore-link-target <glob>]...`

- `--exclude <glob>` — glob pattern (relative to `path`) to leave out of the
  index entirely; repeatable. Adds to, rather than replaces, any `exclude`
  patterns already set in `knap.toml`. `--ignore-link-target <glob>` also
  accepts here — it doesn't change indexing (a matching link still shows its
  true `resolved` status), only whether `knap lint` reports it.

When `<path>` is a single file, `knap index` scopes to that one file's
neighborhood instead: `--json` emits a single doc object (`headings`,
`links`, `backlinks`, `tags`) rather than the `{ "notes": [...], "tags":
{...} }` workspace envelope. Useful for an agent to inspect a just-edited
file without paging the full index. A directory `<path>` prints the
full-workspace listing, unchanged.

## Headless rename (`knap rename-*`)

The same rename computations the editor's rename dialog uses
(`workspace/rename`, `workspace/willRenameFiles`), run against a workspace
without a client — the edit is computed and written to disk in one step.
Useful for scripting a rename across a workspace, or for an agent that needs
to retarget links without opening an editor.

```
$ knap rename-file docs/old.md docs/new.md
docs/old.md → docs/new.md (2 file(s) touched)

$ knap rename-heading docs/a.md "Old Section" "New Section"
"Old Section" → "New Section" in docs/a.md (2 file(s) touched)

$ knap rename-tag draft published
#draft → #published (3 file(s) touched)
```

- `knap rename-file <old> <new>` (alias: `knap move-file`) — moves `<old>` to
  `<new>` on disk and rewrites every incoming and outgoing link affected by
  the move. Fails if `<old>` doesn't exist or `<new>` already does.
- `knap rename-heading <file> <old> <new>` — renames a heading in `<file>`
  (`<old>` may be the heading's text or its GFM slug) and rewrites every
  same-file and cross-file anchor link that targets it. Fails if no heading
  in `<file>` matches `<old>`.
- `knap rename-tag <old> <new>` — rewrites every frontmatter occurrence of
  `<old>` across the workspace, in all three YAML tag forms (bare scalar,
  inline list, block list). Fails if no file uses `<old>`.

All three scope their index to the current directory (like `knap lint .`),
apply their edit atomically, and print a summary line on success.

## Batch apply (`knap apply`)

Applies a whole set of change operations in one call instead of one
subprocess per change. Reads a JSON array of operations from stdin — one
`rename-file`/`rename-heading`/`rename-tag`/`repoint-link`/
`repoint-anchor` per entry — and applies them in order, all-or-nothing: the
workspace ends up either fully changed or untouched, never partially
applied.

```
$ echo '[
  {"op":"rename-tag","old":"wip","new":"draft"}
]' | knap apply
applied rename-tag: #wip → #draft
1 operation(s), 3 file(s) touched
```

- `rename-file`/`rename-heading`/`rename-tag` — same field names as
  that subcommand's arguments.
- `repoint-link { file, range, target }` / `repoint-anchor { file, range,
anchor }` — retarget a broken link or anchor at exactly `range` (a
  diagnostic's own range, e.g. from `knap lint --suggest --json`) to a
  candidate you've already picked, rather than deriving one automatically.
  Useful for the ambiguous or `text_mismatch` cases `knap lint --suggest`
  surfaces:
  ```
  $ echo '[{"op":"repoint-link","file":"docs/a.md",
    "range":{"start":{"line":0,"character":7},"end":{"line":0,"character":18}},
    "target":"reference/config.md"}]' | knap apply
  applied repoint-link: docs/a.md: repoint → 'reference/config.md'
  1 operation(s), 1 file(s) touched
  ```

Useful after `knap lint --suggest --json` — pick the right fix for each
finding yourself, then apply the whole batch in one call rather than
shelling out once per change.

- `--dry-run` — prints the plan without changing anything in the real
  workspace; the reported file count is exactly what a real run would touch.
- `--json` — emits an `ApplyReport` (`dry_run`, `operations`, `files_touched`)
  instead of the text summary.

Operations run in the order given against a scratch copy of the workspace,
so a later operation in the same batch sees an earlier one's effects (e.g.
renaming a heading in a file the same batch just renamed). If any operation
fails, nothing in the real workspace is touched — the same all-or-nothing
guarantee `rename-*` gives a single operation, extended across the whole
batch.

`repoint-link`/`repoint-anchor` re-parse the file right after writing it and
reject the operation (naming its position and kind, e.g.
`operation 2 (repoint-link)`) if a well-formed link no longer sits where
expected — a guard against a stale or skewed `range` silently corrupting
the file. Always copy `range` verbatim from the diagnostic; never
recompute it.

Usage: `knap apply [--dry-run] [--json]`

## Coding agents

This is the other half of the synergy: an agent editing a workspace a human
also writes in shouldn't have to re-derive knap's conventions from `--help`
text, and shouldn't leave broken links behind for the human to find later.
`skill/knap/SKILL.md` documents the `lint` → `rename-*`/`apply` → `lint`
edit-verify loop for a coding agent working in a workspace that has `knap`
installed — the six diagnostic `code`s, `--fail-on`/`--since`, and
`knap index <file> --json` for a fast, scoped read of just the file it
touched. Install it into a workspace's skill directory with `knap skill` to
teach an agent knap's conventions directly:

```
knap skill --global
# or, project-scoped:
knap skill --path <workspace>/.claude/skills/knap
```

## Configuration

Configuration (doc subdirectory, file extensions, frontmatter schema) comes
from two sources, both read by every command above — `knap lint` and `knap
index` see the same config an editor session would:

- your editor's native LSP settings via `initializationOptions`, or
- a `knap.toml` file at your workspace root

### `knap.toml`

An optional project config file at a workspace root, read by `knap lsp`,
`knap lint`, and `knap index` alike (for `lint`/`index`, the root is the
target directory, or the parent directory when the target is a single
file):

```toml
extensions = ["md"]
new_note_dir = "inbox"
# Glob patterns matched against each entry's path relative to the index
# root; matching directories are never crawled and matching files are
# skipped entirely. Don't exclude the root itself (e.g. `"."` or `"**"`) —
# that produces an empty index rather than an error.
exclude = ["tests/fixtures/**"]
# Glob patterns matched against bare directory names (not paths) during the
# crawl; a matching directory is pruned outright. Replaces rather than
# unions with the built-in default (`.*`, `node_modules`, `target`) — list
# everything you want skipped, including any of the defaults you still want.
skip_dirs = ["vendor"]
# Glob patterns matched against a link's raw target text. A matching link is
# still indexed and still resolved normally — this only suppresses the
# `broken-link` diagnostic knap would otherwise report for it. Unioned with
# any `--ignore-link-target` flag values and with each file's own
# `ignore-link-targets:` frontmatter key.
ignore_link_targets = ["../sibling-workspace/**"]

[frontmatter_schema]
require_frontmatter = false
warn_unknown_keys = false

[frontmatter_schema.fields.title]
required = true

[frontmatter_schema.fields.status]
values = ["draft", "published"]
```

When running under `knap lsp`, `initializationOptions` from the editor
layers over `knap.toml` field-by-field — the editor value wins where
present, `knap.toml` fills in the rest. `exclude` is the exception: instead
of one source winning, the editor's `initializationOptions.exclude` and
`knap.toml`'s `exclude` are unioned, so patterns from both apply.
`ignore_link_targets` follows the same union rule as `exclude` — the
editor's `initializationOptions.ignoreLinkTargets` and `knap.toml`'s
`ignore_link_targets` both apply, rather than one replacing the other.
`skipDirs` follows the same override precedence as `extensions`, not
`exclude`'s union — when set, `initializationOptions.skipDirs` fully
replaces `knap.toml`'s `skip_dirs`.

## Documentation

- [Getting Started](docs/GETTING_STARTED.md) — install the server, connect your
  editor, and understand what each feature does
- [User Stories](docs/USER_STORIES.md) — what knap does, told from the writer's
  perspective
- [Roadmap](docs/ROADMAP.md) — features grouped into releases, starting with the
  MVP
- [Architecture](docs/ARCHITECTURE.md) — component design and contracts
