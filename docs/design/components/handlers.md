# Request Handlers & Diagnostics

Covers all LSP request handlers and the diagnostic publisher.

Each request handler receives the decoded params and a shared reference to the
`NoteIndex`. Handlers are pure functions — they do not mutate the index or send
messages directly. They return a value that the Protocol Handler serialises and
sends.

---

## Shared helpers

### find_md_link_at_position()

Used by Definition and References. Finds the Markdown link in a note whose
range contains a given cursor position.

```rust
fn find_md_link_at_position(note: &Note, pos: Position) -> Option<&MarkdownLink> {
    note.md_links.iter().find(|link| contains(link.range, pos))
}

fn contains(range: Range, pos: Position) -> bool {
    (pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character))
    && (pos.line < range.end.line
        || (pos.line == range.end.line && pos.character <= range.end.character))
}
```

### escape_link_target()

Wraps a link destination in angle brackets (`<...>`) when it contains
characters that would otherwise break or truncate a bare `](...)`
destination: whitespace, ASCII control characters, or parentheses. Inner `<`,
`>`, `\` are backslash-escaped so the wrapped form round-trips. Destinations
with none of those characters are returned unchanged.

```rust
fn escape_link_target(target: &str) -> String {
    if !crate::parser::link_destination_needs_wrapping(target) {
        return target.to_string();
    }
    // wrap in `<...>`, backslash-escaping inner `<`, `>`, `\`
}
```

The wrapping predicate itself (`link_destination_needs_wrapping`) lives in
`src/parser/mod.rs`, not here — the parser's own fallback link scan (see
[parser.md](parser.md)) needs the identical condition to decide which
`[text](...)` spans pulldown-cmark refused to parse as links, so both the
read side and the write side share one definition of "needs wrapping".

Used by:

- **Completion** (below) — terminal file-item insertions
- **Code Actions** — "Create note" rewrites the broken link's text when its
  target needs wrapping

### find_heading(), compute_heading_rename(), compute_tag_rename()

Position-independent counterparts to the cursor-driven logic in `handle_rename`
(below) — same edit computation, no `Position` required. Added in v0.12 so the
headless `knap rename-heading`/`knap rename-tag` CLI subcommands can compute
the exact same `WorkspaceEdit` an editor's rename dialog would produce,
without a live LSP session or a cursor.

```rust
fn find_heading<'a>(note: &'a Note, query: &str) -> Option<&'a Heading>
fn compute_heading_rename(
    path: &Path,
    note: &Note,
    heading: &Heading,
    new_name: &str,
    index: &NoteIndex,
) -> WorkspaceEdit
fn compute_tag_rename(old_name: &str, new_name: &str, index: &NoteIndex) -> WorkspaceEdit
```

- `find_heading` — turns a `<old>` CLI argument into a `Heading`: exact text
  match first (case-insensitive), then GFM slug match. `None` if neither
  matches.
- `compute_heading_rename` — the heading-text edit, same-file self-link
  edits, and incoming cross-file anchor edits described under "Heading
  rename" below, given the `Heading` directly instead of locating it from a
  cursor position.
- `compute_tag_rename` — an edit for every occurrence of `old_name` across
  every note in `index.notes_by_tag(old_name)`. No "current file" special
  case — that only exists for a cursor's unindexed buffer, which doesn't
  apply to a purely index-driven caller.

---

## Completion (`textDocument/completion`)

### When it fires

The client sends a completion request when the user types `(`, `#`, or `/`
(all three are registered as trigger characters). Two distinct completion modes
are dispatched:

```rust
pub(crate) fn handle_completion(
    params: CompletionParams,
    index: &NoteIndex,
    config: &Config,
) -> Vec<CompletionItem>
```

### Anchor completion (`](path#` or `](#`)

When `check_anchor_trigger` detects that the cursor is immediately after a `#`
inside a link destination, the handler returns one item per heading. Both
cases go through the same `index.resolve(&path, &target_rel)` call (v0.11.1,
#60) — there is no separate empty-target branch:

- **Same-file anchor** (`[text](#`) — `target_rel` is empty; `resolve()`
  resolves an empty target to `path` itself, so headings come from the
  current note.
- **Cross-file anchor** (`[text](file.md#`) — `target_rel` is non-empty; the
  handler resolves the target note via `index.resolve` and returns its headings.

Each item has:

- `label`: heading text as written (e.g. `"My Section"`)
- `insert_text`: GFM slug (e.g. `"my-section"`)
- `filter_text`: heading text (for editor-side fuzzy matching)
- `kind`: `REFERENCE`

### Directory completion (`](` or `](partial/`)

When `check_dir_trigger` detects that the cursor is inside a link destination
with no `#`, the handler returns items in an optional "accept this folder"
item plus three sorted tiers. `sort_text` uses a string prefix so editors
that respect the field keep the tiers ordered even when their fuzzy scorer
would otherwise rerank items:

| Tier                 | `sort_text`  | Contents                                                                                                                                                                                                                                                                                                                                                                                                      |
| -------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| "accept this folder" | `""` (empty) | **FOLDER** item, offered only when `base_dir != note_dir && index.is_dir_indexed(&base_dir)` — i.e. once drilled into an indexed directory. Label is the directory's bare name (no trailing slash), `detail` is `"Link to this folder"`. Its `new_text` is the finished `base_dir/` path, letting the link target the folder itself instead of requiring a further drill-down. Sorts before every tier below. |
| 0                    | `"0_"`       | **FOLDER** items — immediate subdirectories of `base_dir`, sourced from `index.child_dirs(&base_dir)` (v0.17; includes directories with no files in them, which file-path inference alone could never see). Label is `subdir/`; selecting one re-triggers completion (via the registered `/` trigger character) to show its contents.                                                                         |
| 1                    | `"1_"`       | **FILE** items — notes and attachments directly inside `base_dir`. For notes with a frontmatter `title`, the label is the title and `detail` is the filename.                                                                                                                                                                                                                                                 |
| 2                    | `"2_"`       | **FILE** items — every other workspace file not already shown as a tier-1 item and not the current file. Label is the frontmatter `title` if present, otherwise the bare filename. `filter_text` is the full relative path so editors surface the item when the user types any path segment (e.g. `sub` surfaces `sub/b.md`). `detail` is the full relative path.                                             |

Files already shown in tier 1 are tracked in a `HashSet` and excluded from
tier 2 to avoid duplicates.

Every item uses `text_edit: CompletionTextEdit::Edit(TextEdit { range, new_text
})` where `range` replaces everything from right after `](` to the cursor, and
`new_text` is the full relative path from the current note's directory (e.g.
`sub/` for a folder item, `sub/b.md` for a file). This ensures that
re-triggering after selecting a folder item, or selecting a global item while a
partial prefix is typed, replaces the prefix cleanly.

For tier-1 and tier-2 **file** items, `new_text` is passed through
`escape_link_target()` — a path containing whitespace, control characters, or
parentheses (e.g. `My File.md`) is wrapped in `<...>` so the inserted link is
valid CommonMark and actually resolves. Tier-0 **folder** items are
deliberately left unescaped: their `new_text` is an intentionally incomplete
destination (`sub/`) that the user keeps typing after, and re-triggering
completion depends on `check_dir_trigger` re-reading the raw, unwrapped text
between `](` and the cursor — wrapping a partial destination in `<` would
break that re-read on the next keystroke. `filter_text`, `label`, and
`detail` are unaffected by escaping — they're display/matching fields, never
inserted into the document.

### Frontmatter value completion

When `check_frontmatter_value_trigger` detects the cursor is after the `:` on a
frontmatter key line, the handler looks up the key (case-insensitive) in
`config.frontmatter_schema.fields`. If the matching `SchemaField` has a `values`
list, it returns one `VALUE` item per allowed value whose string starts with the
typed partial (exact-case prefix match). Returns `vec![]` when the key is absent
from the schema or has no `values` list.

### Frontmatter key completion

When `check_frontmatter_key_trigger` detects the cursor is in key position inside
the frontmatter block, the handler returns one `FIELD` item per schema key that:

- is not already present in the note's frontmatter (case-insensitive), and
- starts with the typed partial (case-insensitive prefix match).

Each item's `new_text` is `"key: "` (key name followed by colon-space). Returns
`vec![]` when `config.frontmatter_schema.fields` is empty.

**Priority order** within `handle_completion`: tag trigger → frontmatter value trigger → anchor trigger → directory trigger → frontmatter key trigger.

---

## Go to Definition (`textDocument/definition`)

```rust
pub(crate) fn handle_definition(
    params: GotoDefinitionParams,
    index: &NoteIndex,
) -> Option<GotoDefinitionResponse>
```

Finds the `MarkdownLink` at the cursor position and returns a `Location`.
Both same-file and cross-file links go through the same
`index.resolve(&path, &link.target)` call (v0.11.1, #60) — there is no
separate empty-target branch.

**Same-file anchor** (`link.target` is empty): `resolve()` resolves the
empty target to the current file itself. When the link has an anchor,
navigates to the matching heading's `range` in `note.headings` (falling back
to `Range::default()`, top of file, if no heading matches). When there is no
anchor, returns `Range::default()`.

**Cross-file link** (`link.target` is non-empty): resolves via `resolve()`
to the target note. Returns `None` for broken links. When the link has an
anchor, navigates to the matching heading's `range` in the target note
(falling back to `Range::default()` if the anchor doesn't match). When there
is no anchor, returns `Range::default()`.

Response is always `GotoDefinitionResponse::Scalar(Location)`.

---

## Find References (`textDocument/references`)

```rust
pub(crate) fn handle_references(params: ReferenceParams, index: &NoteIndex) -> Vec<Location>
```

Priority:

1. **Tag at cursor** → returns all notes carrying that tag.
2. **Markdown link at cursor** → resolves the target; returns all
   `LocatedLink`s from `index.links_to(target)`. Returns `vec![]` for broken
   links. A same-file anchor link under the cursor now resolves to the
   current file (v0.11.1, #60 side effect) instead of `Broken`, so it
   returns real backlinks to the current file rather than nothing.
3. **Heading at cursor** (no link at cursor) → collects all anchor references
   to that heading: same-file bare anchors (`[text](#slug)` in the current note
   whose anchor slug matches) plus cross-file anchors (`[text](this.md#slug)`
   from `index.links_to(current_path)` filtered by anchor slug).
4. **No link or heading at cursor** → returns all backlinks to the current
   document (`index.links_to(current_path)`).

---

## Diagnostics

Diagnostics are not a request handler — they are published proactively by the
Protocol Handler whenever the index changes. The Protocol Handler calls
`publish_diagnostics` with the set of affected paths returned by `IndexDelta`.

```rust
pub(crate) fn publish_diagnostics(
    paths: &HashSet<PathBuf>,
    index: &NoteIndex,
    config: &Config,
    sender: &Sender<Message>,
)
```

### compute_diagnostics()

```rust
pub(crate) fn compute_diagnostics(path: &Path, index: &NoteIndex, config: &Config) -> Vec<Diagnostic>
```

For each Markdown link in the note:

| Link type                                              | Diagnostic range               | Message                                    |
| ------------------------------------------------------ | ------------------------------ | ------------------------------------------ |
| Bare anchor `[text](#slug)` — slug not in this file    | `link.anchor_range` (or range) | `Heading not found: '#slug'`               |
| Bare anchor `[text](#)` — empty slug (`anchor = None`) | —                              | No diagnostic                              |
| Cross-file — `Broken` target                           | `link.target_range`            | `Link target not found: 'path/to/note.md'` |
| Cross-file — `Found` + anchor not matching any heading | `link.anchor_range`            | `Heading not found: '#anchor'`             |
| Cross-file — `Found` + no anchor (or valid anchor)     | —                              | No diagnostic                              |

Bare anchor-only links (`target = ""`) flow through the same
`match index.resolve(path, &link.target)` as cross-file links (v0.11.1, #60)
— `resolve()` resolves the empty target to `path` itself, so the "Cross-file
— `Found`" row above is what actually validates the anchor against the
current note's headings (via GFM slug comparison). A link `[text](#)` with
an empty anchor (`link.anchor = None`) produces no diagnostic.

### Schema diagnostics

When `config.frontmatter_schema` is non-empty (has fields, `require_frontmatter`,
or `warn_unknown_keys` set), an additional validation pass runs after the
link-diagnostics loop:

| Condition                                                                   | Diagnostic range    | Message                                          |
| --------------------------------------------------------------------------- | ------------------- | ------------------------------------------------ |
| Note has no frontmatter + `require_frontmatter: true` + field is `required` | `(0,0)`             | `Required frontmatter key missing: 'key'`        |
| Note has frontmatter + required schema key absent (case-insensitive match)  | `(0,0)`             | `Required frontmatter key missing: 'key'`        |
| Field value not in schema `values` list (exact-case equality)               | `field.value_range` | `Value 'X' is not in the allowed list for 'key'` |
| Key not in schema + `warn_unknown_keys: true`                               | `field.key_range`   | `Unknown frontmatter key: 'key'`                 |
| Field has no scalar value (`value: None`) or schema has no `values` list    | —                   | No diagnostic                                    |

Key matching is case-insensitive (`eq_ignore_ascii_case`). Value matching is
exact-case.

### `code` (v0.13)

Every `Diagnostic` literal `compute_diagnostics` builds also carries a
`code` — a stable `NumberOrString::String` an agent (or an editor's Problems
panel) can branch on instead of parsing the message text, which is free to
change wording between releases:

| Diagnostic                                                   | `code`                     |
| ------------------------------------------------------------ | -------------------------- |
| Cross-file — `Broken` target                                 | `"broken-link"`            |
| Bare anchor or cross-file anchor not matching any heading    | `"broken-anchor"`          |
| Required field missing, note has no frontmatter block at all | `"missing-frontmatter"`    |
| Required field missing, frontmatter block exists             | `"missing-required-field"` |
| Value not in the schema's allowed list                       | `"invalid-field-value"`    |
| Frontmatter key not recognized by the schema                 | `"unknown-field"`          |

The six values are module-level constants (`CODE_BROKEN_LINK`, etc.) next
to `DIAG_SOURCE`. This flows through both existing consumers of
`compute_diagnostics` for free: `knap lint --json`'s
`FileDiagnostics.diagnostics` (already `Vec<lsp_types::Diagnostic>`, so
`code` serializes with no report-shape change) and real
`textDocument/publishDiagnostics` notifications.

---

## Rename (`workspace/willRenameFiles`)

```rust
#[allow(clippy::mutable_key_type)]
pub(crate) fn handle_will_rename_files(params: RenameFilesParams, index: &NoteIndex) -> WorkspaceEdit
```

Called by the editor before applying a rename. Returns a `WorkspaceEdit` that
rewrites all affected links atomically — editors apply the edit and the rename
together so no link is left broken.

For each `FileRename { old_uri, new_uri }` in `params.files`:

1. **Incoming links** — iterates `index.links_to(&old_path)`. For each
   `LocatedLink`, computes `new_target = relative_path(source_dir, new_path)`
   and pushes a `TextEdit` on `located.md_link.target_range` into the source
   file's entry in `changes`.

2. **Outgoing links** — fetches `index.get_note(&old_path).md_links`. For each
   link, skips empty targets and URLs; computes
   `abs_target = normalize_path(old_dir.join(&link.target))`, then
   `new_target = relative_path(new_dir, &abs_target)`; pushes a `TextEdit` on
   `link.target_range` into `old_path`'s entry only when
   `new_target != link.target` (i.e. the rename changes the relative path).

Returns `WorkspaceEdit { changes: Some(changes), ..Default::default() }`. The
`changes` map is keyed by `lsp_types::Uri`; an empty map is returned for files
with no affected links.

---

## Document Symbols (`textDocument/documentSymbol`)

```rust
pub(crate) fn handle_document_symbols(
    params: DocumentSymbolParams,
    index: &NoteIndex,
) -> Option<DocumentSymbolResponse>
```

Returns a flat list of `DocumentSymbol` entries, one per heading in document
order. Each symbol carries the heading text as its `name`, `SymbolKind::STRING`,
and a `range` / `selection_range` covering the full heading line. Returns `None`
when the file is not indexed; returns an empty list for a file with no headings.

---

## Workspace Symbols (`workspace/symbol`)

```rust
pub(crate) fn handle_workspace_symbols(
    params: WorkspaceSymbolParams,
    index: &NoteIndex,
) -> Vec<SymbolInformation>
```

Returns headings from all indexed notes whose text contains the query string
(case-insensitive). An empty query returns every heading in the workspace.
Each result carries the heading text as `name`, the containing filename (without
directory) as `container_name`, and `SymbolKind::STRING`.

---

## Prepare Rename (`textDocument/prepareRename`)

```rust
pub(crate) fn handle_prepare_rename(
    params: TextDocumentPositionParams,
    index: &NoteIndex,
) -> Option<PrepareRenameResponse>
```

Returns `Some(PrepareRenameResponse::RangeWithPlaceholder { range, placeholder
})` when the cursor is on a frontmatter tag or a heading line. Returns `None`
otherwise — the editor shows no rename UI in that case.

**Priority order:** tag check first, then heading check.

- **Tag at cursor** — `range` covers the tag text only (not surrounding YAML
  punctuation); `placeholder` is the tag name. Works for all three YAML tag
  forms: bare scalar (`tags: rust`), inline list (`tags: [rust, go]`), and
  block list (`- rust` under `tags:`). Uses `find_tag_at_position`.
- **Heading at cursor** — `range` is the heading text range (excluding the
  `## ` prefix); `placeholder` is the **raw source text** at that range
  (extracted from `note.content`, not `heading.text`). `heading.text` is
  pulldown-cmark's rendered text with inline markup stripped, so for a
  heading like `## My _Fancy_ Heading` it would mismatch `text-at-range`
  (`"My _Fancy_ Heading"`) — editors that validate `placeholder ==
text-at-range` per the LSP spec would then refuse to show the rename
  dialog (v0.3.4, #3).

The handler uses the indexed note when available. If the file is absent from the
index (e.g. the server started without workspace folders configured and no
`didOpen` has been received yet), it falls back to reading the file from disk
and parsing it on the fly. Returns `None` if the file cannot be read.

---

## Rename (`textDocument/rename`)

```rust
pub(crate) fn handle_rename(
    params: RenameParams,
    index: &NoteIndex,
) -> Option<WorkspaceEdit>
```

Renames a tag across the workspace, or renames a heading and all anchor links
that point to it. Applies the same indexed-note / disk-parse fallback as
`handle_prepare_rename`. Returns `None` when the cursor is on neither a tag
nor a heading.

**Priority order:** tag check first, then heading check.

Both branches delegate the actual edit computation to a position-independent
helper (see "Shared helpers" above) — `handle_rename` itself only locates
_what_ is being renamed from the cursor position, then hands off. This split
exists so `knap rename-heading`/`knap rename-tag` (the headless CLI
subcommands, v0.12) can reuse the exact same computation without a cursor:
they call `find_heading`/`compute_heading_rename` or `compute_tag_rename`
directly.

### Tag rename

When the cursor is on a frontmatter tag, collects `TextEdit`s for every
occurrence of that tag name (case-insensitive) across the workspace:

1. **Current note** — always handled directly (covers both the indexed case and
   the disk-parse fallback). Iterates `note.frontmatter.tags`, matches via
   `eq_ignore_ascii_case`, pushes one `TextEdit` per matching tag.
2. **Other indexed notes** — delegates to `compute_tag_rename(&old_name,
&new_name, index)`, then merges its edits in, skipping `current_path` (the
   current note was already handled by step 1, with its disk-parse fallback
   `compute_tag_rename` — index-only — can't cover).

The replacement text is exactly the string the user typed — casing is not
normalised.

### Heading rename

1. Locates the `Heading` under the cursor via the note's `headings` list
   directly (not `find_heading`, which is text/slug based and has no cursor
   position to work from).
2. Delegates to `compute_heading_rename(&path, note, heading, &new_name,
index)` for the edit itself:
   - **Heading text edit** — rewrites the heading text in place (preserving
     the `## ` prefix) to the new name.
   - **Self-link edits** — anchor-only links (`[text](#old-slug)`) within the
     same file are rewritten to the new slug.
   - **Incoming anchor edits** — for every note in the workspace that links to
     this file via `index.links_to`, finds `[text](path#old-slug)` links whose
     slug matches the old heading (via `slug()`) and rewrites the anchor to
     the new slug. When the file was not in the index (disk-parse fallback),
     `links_to` returns an empty slice and no incoming-link edits are
     produced.

URL targets are skipped. Returns `Some(WorkspaceEdit { changes: Some(map) })`.

---

## Code Actions (`textDocument/codeAction`)

```rust
pub(crate) fn handle_code_actions(
    params: CodeActionParams,
    index: &NoteIndex,
    config: &Config,
) -> Vec<CodeActionOrCommand>
```

Re-derives link context from the index by iterating `note.md_links` and
checking `contains(link.range, cursor)` where `cursor = params.range.start`.
Anchor-only links (`link.target.is_empty()`) are always skipped.

For each link under the cursor:

| Condition                                       | Action offered                                                                                                                                    |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `index.resolve(…) == Broken`                    | **Create note** — a `CreateFile` workspace edit (`ignore_if_exists: true`), plus a `TextDocumentEdit` fixing the link text when it needs escaping |
| `Found(target)` + broken anchor (slug mismatch) | One **Change anchor to "…"** per heading in the target note — a `TextEdit` on `link.anchor_range`                                                 |
| `Found(target)` + valid anchor (or no anchor)   | No action                                                                                                                                         |

**Create note** first unescapes `link.target` via `index::unescape_link_target()`
— `link.target` is the raw text between `(` and `)`, so an already-wrapped
broken link (`[text](<My File>)`) has it literally including the `<...>`;
unescaping strips that before the target is reused, so the file isn't named
`<My File>.md` and the link text isn't double-wrapped if rewritten. From the
unescaped target it builds a `WorkspaceEdit` with:

1. A `CreateFile` op at `new_note_path(&clean_target, …)` (`ignore_if_exists: true`).
2. A `TextDocumentEdit` op rewriting `link.target_range` to
   `escape_link_target(&clean_target)`, **only** when that differs from the
   original `link.target` — i.e. only when the link's existing text isn't
   already valid. `link.target_range` excludes any `#anchor`, so anchored
   broken links (`[text](missing file#x)`) get only their path segment
   rewritten.

New-file path logic for **Create note** (`new_note_path`):

```rust
fn new_note_path(link_target: &str, source: &Path, config: &Config) -> PathBuf {
    let path = match config.new_note_dir.as_deref().zip(config.index_roots.first()) {
        Some((dir, root)) => root.join(dir).join(Path::new(link_target).file_name()),
        None => normalize_path(&source.parent().join(link_target)),
    };
    if path.extension().is_none() { path.with_extension("md") } else { path }
}
```

`.md` is appended when the target has no extension — otherwise a link typed
as `[text](My New Note)` would create a file literally named `My New Note`
with no extension.

### `compute_create_missing_file_fix()`, `compute_anchor_fix()` (v0.13)

Position-independent counterparts to **Create note**/**Change anchor
to...**, extracted the same way `find_heading`/`compute_heading_rename`/
`compute_tag_rename` were in v0.12 (see "Shared helpers" above), extracted
for reuse across every caller that computes the same edit
`handle_code_actions` does.

```rust
pub(crate) fn compute_create_missing_file_fix(
    link: &parser::MarkdownLink,
    source: &Path,
    config: &crate::config::Config,
) -> WorkspaceEdit
pub(crate) fn compute_anchor_fix(
    source: &Path,
    anchor_range: Range,
    new_anchor: &str,
) -> WorkspaceEdit
```

- `compute_create_missing_file_fix` — today's `ResolvedLink::Broken` arm
  body verbatim: a `CreateFile` op at `new_note_path(...)`, plus a
  `TextEdit` rewriting `link.target_range` when the unescaped target needed
  `<...>` escaping. `handle_code_actions`'s Broken arm now just calls this
  and wraps the result in a `CodeAction { title: "Create note", .. }`.
- `compute_anchor_fix` — a single `TextEdit` at `anchor_range` replacing it
  with `new_anchor`. `handle_code_actions`'s per-heading loop calls this
  once per candidate heading (a human picks); it's also the execution side
  of `knap apply`'s `repoint-anchor` op (`src/cli/apply.rs`).

### Text-aware ranking: `RankedCandidate`, `combined_distance()`, `unambiguous_winner()`, `text_mismatch()` (v0.16)

Added to stop `rank_link_candidates`/`rank_anchor_candidates` from being
fooled by a broken target/slug that happens to be textually close to the
_wrong_ note when the link's own visible text names the _right_ one (e.g.
`[Sync 835](sync-800.md)` — the raw target is one edit from `sync-800.md`
but the link text names `sync-835.md`). Every ranked candidate now carries
two distance signals blended into one sort key, and
`compute_diagnostics_with_suggestions` reports the disagreement via
`text_mismatch` rather than picking a winner, instead of trusting the
blended score alone.

```rust
fn edit_distance(a: &str, b: &str) -> usize
struct RankedCandidate<T> {
    candidate: T,
    path_distance: usize,
    text_distance: Option<usize>, // None when the link has no usable display text
    combined: f64,
}
const PATH_WEIGHT: f64 = 0.5;
const TEXT_WEIGHT: f64 = 0.5;
fn normalized_distance(distance: usize, a: &str, b: &str) -> f64
fn combined_distance(
    path_distance: usize, path_a: &str, path_b: &str,
    text_distance: Option<usize>, text_a: &str, text_b: &str,
) -> f64
fn text_mismatch<T: PartialEq>(ranked: &[RankedCandidate<T>]) -> bool
```

- `edit_distance` — byte-wise Levenshtein distance. GFM slugs are already
  lowercase ASCII alphanumerics and hyphens, so byte-wise is equivalent to
  char-wise here.
- `normalized_distance` — a raw edit distance divided by the longer of the
  two compared strings' character counts, so short link text isn't swamped
  by (or doesn't swamp) full relative paths regardless of the weights.
- `combined_distance` — `PATH_WEIGHT * normalized(path_distance) +
TEXT_WEIGHT * normalized(text_distance)`, falling back to the path term
  alone when there's no text signal (empty link text, or text identical to
  its own target) — a link with no usable display text ranks exactly as it
  did before v0.16.
- `text_mismatch` — `true` when the top-`combined` candidate isn't also the
  top candidate by `text_distance` alone, i.e. the link's own visible text
  points somewhere the blended ranking didn't land. Always `false` when no
  candidate has a text signal — nothing to disagree with.

`rank_anchor_candidates(broken_slug, link_text, target_note) ->
Vec<RankedCandidate<&Heading>>` and `rank_link_candidates(broken_target,
link_text, source, index) -> Vec<RankedCandidate<String>>` (both private)
compute `path_distance` as before (slug-to-slug / target-to-relative-path)
and `text_distance` as the edit distance between the link text's GFM slug
and the candidate's own slug (heading text, or file stem for links), then
sort by `combined`. Both are used by `knap lint --suggest` (which wants the
whole ranked list, `text_distance` included, to show the agent).

### `compute_link_fix()` (v0.13, text-aware since v0.16)

The link-target counterpart to `compute_anchor_fix`,
added alongside `knap lint --suggest` (after the six functions
above) so a broken link can be repointed at an existing note the same way a
broken anchor is repointed at an existing heading, instead of always falling
back to creating a stub file.

```rust
pub(crate) fn compute_link_fix(source: &Path, target_range: Range, new_target: &str) -> WorkspaceEdit
```

- `compute_link_fix` — a single `TextEdit` on `target_range` replacing it
  with `escape_link_target(new_target)`. Mirrors `compute_anchor_fix`'s
  shape exactly, just for a link's `target_range` instead of `anchor_range`.

The execution side of `knap apply`'s `repoint-link` op
(`src/cli/apply.rs`) — the caller supplies the already-chosen `new_target`.

### `compute_diagnostics_with_suggestions()` (v0.13, `text_distance`/`text_mismatch` since v0.16)

```rust
pub(crate) fn compute_diagnostics_with_suggestions(
    path: &Path,
    index: &NoteIndex,
    config: &crate::config::Config,
    top_n: usize,
) -> Vec<Diagnostic>
```

Calls `compute_diagnostics` unchanged, then — when `top_n > 0` — attaches up
to `top_n` ranked candidates to each `broken-link`/`broken-anchor`
diagnostic's `data` field as `{ "suggestions": [{ "target", "distance",
"text_distance" }, ..] }`, plus `"text_mismatch": true` when the top-ranked
candidate disagrees with the link's own text (omitted when `false`), via
`rank_link_candidates`/`rank_anchor_candidates` re-run against the same
link/anchor the diagnostic's range identifies. `target` is a relative path
for a `broken-link` candidate, or `"#slug"` for a `broken-anchor` candidate;
`distance` is `path_distance`. `top_n == 0` returns `compute_diagnostics`'s
output verbatim, no `data` field added — this is what lets `knap lint`
without `--suggest` stay byte-for-byte identical to before this function
existed.

Used only by `knap lint --suggest` (`src/cli/lint.rs`) — the LSP server
keeps calling plain `compute_diagnostics` for `textDocument/publishDiagnostics`,
since editors don't consume `data` here and ranking every broken link/anchor
against the whole vault on every keystroke-triggered publish would be
wasted work the interactive session doesn't need.

---

## Code Lens (`textDocument/codeLens`)

```rust
pub(crate) fn handle_code_lens(params: CodeLensParams, index: &NoteIndex) -> Vec<CodeLens>
```

Returns two classes of lenses:

1. **Backlinks lens** — a single `↑ N backlink(s)` lens anchored at line 0,
   character 0. Omitted when the file has no incoming links. Uses
   `editor.action.showReferences` with the pre-computed `Location` list so VS
   Code opens the references panel on click without a second request.

2. **Heading anchor-link lenses** — one `↑ N anchor link(s)` lens per heading
   that is the target of one or more `#slug` anchor links. Includes same-file
   bare anchors (`[text](#slug)` in the current file) and cross-file anchors
   (`[text](this.md#slug)` from any note in the workspace). Headings with no
   incoming anchor links produce no lens. The lens `range.start` equals the
   heading's `range.start`.

---

## Folding Ranges (`textDocument/foldingRange`)

```rust
pub(crate) fn handle_folding_ranges(params: FoldingRangeParams, index: &NoteIndex) -> Vec<FoldingRange>
```

Returns fold regions for heading sections and fenced code blocks.

- **Heading sections** — one region per heading, from the heading's line to the
  line before the next peer-or-higher-level heading (or the last content line of
  the document). Single-line sections (end equals start) are omitted.
- **Code fences** — one `FoldingRangeKind::Region` per `CodeFence` in
  `note.code_fences`.

Private helper: `fn last_content_line(content: &str) -> u32` — returns the
zero-based line number of the last non-empty line in the document.

---

## Selection Range (`textDocument/selectionRange`)

```rust
pub(crate) fn handle_selection_range(params: SelectionRangeParams, index: &NoteIndex) -> Vec<SelectionRange>
```

Returns one `SelectionRange` per position in `params.positions`, each
describing a chain of nested ranges for smart expand/contract:

**word → link → paragraph → heading section → document**

Levels are deduplicated — if two consecutive levels would have the same range,
the inner one is omitted. The outermost range always covers the full document.

Private helpers:

- `fn word_range_at(line: &str, cursor_char: u32, line_num: u32) -> Option<Range>` —
  returns the UTF-16 range of the word under the cursor; `None` on whitespace.
- `fn paragraph_range(content: &str, cursor_line: u32) -> Range` — scans
  backward and forward from `cursor_line` to the nearest blank lines.
- `fn heading_section_range(content: &str, headings: &[Heading], cursor_line: u32) -> Option<Range>` —
  the section from the enclosing heading to just before the next peer-level
  heading.
- `fn build_selection_chain(pos: Position, note: &Note) -> SelectionRange` —
  assembles the full chain for one position.

---

## Inlay Hints (`textDocument/inlayHint`)

```rust
pub(crate) fn handle_inlay_hints(params: InlayHintParams, index: &NoteIndex) -> Vec<InlayHint>
```

For each Markdown link in the visible range (`params.range`), if the link
resolves to an indexed note with a `title:` frontmatter field, emits one inlay
hint positioned at the end of the link's `target_range`:

- `label`: `InlayHintLabel::String(format!("-> {title}"))`
- `kind`: `None` (neither TYPE nor PARAMETER fits a linked-note title)

External URL targets and broken links produce no hint. Links outside
`params.range` are excluded via `range_contains_position`.

Private helper: `fn range_contains_position(range: &Range, pos: Position) -> bool`.

---

## Utilities

```rust
/// Convert an LSP URI to an absolute filesystem path.
/// Returns `None` for non-`file://` URIs (e.g. `untitled:`, `vscode-notebook-cell:`).
pub(crate) fn uri_to_path(uri: &lsp_types::Uri) -> Option<PathBuf>

/// Convert an absolute filesystem path to an LSP URI.
/// Panics if `path` is not absolute.
pub(crate) fn path_to_uri(path: &Path) -> lsp_types::Uri
```
