# v0.1 Design — MVP: Navigate your workspace

Covers the stories in the v0.1 release:

| Story  | Feature                                                        |
| ------ | -------------------------------------------------------------- |
| US-01  | Path completions inside `[text](` — all notes in the workspace |
| US-02  | Go to Definition on `[text](path/to/note.md)`                  |
| US-05  | Navigation works regardless of link display text               |
| US-03  | Find References on a file                                      |
| US-07  | Broken link diagnostics                                        |
| US-16  | Incremental file watching                                      |
| US-D01 | `knap parse <file>` — inspect parser output                    |
| US-D02 | `knap index <dir>` — inspect index output                      |

---

## Goal

v0.1 delivers the minimum useful knowledge-base tool built entirely on standard
Markdown links. The codebase is rewritten from the wiki-link (`[[...]]`) model to
path-relative standard Markdown links (`[text](path/to/note.md)`). A writer can
link to notes with completions, jump between them with Go to Definition, find what
links back with Find References, and catch broken links via diagnostics — all
without any non-standard syntax.

This is a ground-up rewrite of the parser, index, and handlers. The transport,
protocol lifecycle, and file-watching infrastructure carry over unchanged.

---

## Parser Changes

`Note` loses `stem` and `wiki_links`. `MarkdownLink` gains `anchor`,
`target_range`, and `anchor_range` to support diagnostics and future rename
support. `scan_wiki_links` and exclusion-zone tracking are removed.

### Updated types

```rust
pub struct Note {
    pub path: PathBuf,
    pub md_links: Vec<MarkdownLink>,
    pub content: String,
    pub headings: Vec<Heading>,
    pub frontmatter: Option<Frontmatter>,
    // removed: stem, wiki_links
}

pub struct MarkdownLink {
    pub text: String,
    pub target: String,                  // relative path or URL, raw; empty for anchor-only links
    pub anchor: Option<String>,          // text after `#`, trimmed; None when absent
    pub is_image: bool,
    pub range: LspRange,                 // full `[text](url)` span
    pub target_range: LspRange,          // path inside `()`, excluding `#anchor`
    pub anchor_range: Option<LspRange>,  // anchor text only (None when absent)
}
```

### Updated `extract_body_elements`

No longer tracks exclusion zones or calls `scan_wiki_links`. Returns
`(Vec<MarkdownLink>, Vec<Heading>)`.

```rust
fn extract_body_elements(
    content: &str,
    offset: usize,
    line_index: &LineIndex,
) -> (Vec<MarkdownLink>, Vec<Heading>)
```

For each link/image event, `target_range` and `anchor_range` are derived by
scanning the raw source bytes within the event's span:

1. Scan right from the span start for `(` — marks the start of the URL portion.
2. Record `target_range` from `(` + 1 to the first `#` or `)`.
3. If `#` is present, record `anchor` (trimmed) and `anchor_range` (just the
   anchor text, not the `#`) up to `)`.
4. Split `dest_url` on the first `#` to set `target` and `anchor`.

Anchor-only links (`[text](#heading)`) have `target = ""` and
`anchor = Some("heading")`.

External URLs (`https://`, `http://`, etc.) are captured with `anchor: None`
and `target_range` covering the full URL.

### Updated `parse()`

```rust
pub fn parse(path: &Path, content: &str) -> Note {
    let line_index = LineIndex::new(content);
    let frontmatter = extract_frontmatter(content).map(|mut fm| {
        fm.tags = extract_tags(content, &line_index);
        if let Some(block) = frontmatter_block(content) {
            fm.fields = extract_frontmatter_fields(block, 4, &line_index);
        }
        fm
    });
    let body_offset = frontmatter_body_offset(content);
    let body = &content[body_offset..];
    let (md_links, headings) = extract_body_elements(body, body_offset, &line_index);
    Note { path: path.to_path_buf(), md_links, content: content.to_string(),
           headings, frontmatter }
}
```

---

## Note Index Changes

The index switches from stem/filename resolution to path-relative resolution.
`by_stem` and `by_filename` are replaced by `all_files: HashSet<PathBuf>` — every
file in the workspace, notes and attachments alike. `LocatedLink` stores
`md_link: MarkdownLink` instead of `wiki_link: WikiLink`. `ResolvedLink::Ambiguous`
is removed — standard relative paths are unambiguous by definition.

### Updated types

```rust
pub struct NoteIndex {
    by_path: HashMap<PathBuf, Note>,
    all_files: HashSet<PathBuf>,          // replaces by_stem + by_filename
    links_to: HashMap<PathBuf, Vec<LocatedLink>>,
    by_tag: HashMap<String, Vec<PathBuf>>,
}

pub struct LocatedLink {
    pub source_path: PathBuf,
    pub md_link: MarkdownLink,            // was wiki_link: WikiLink
}

pub enum ResolvedLink {
    Found(PathBuf),
    Broken,
    // removed: Ambiguous
}
```

### Updated `resolve()`

```rust
pub fn resolve(&self, source: &Path, target: &str) -> ResolvedLink {
    if looks_like_url(target) {
        return ResolvedLink::Found(PathBuf::from(target));
    }
    let candidate = source
        .parent()
        .expect("note path must have a parent directory")
        .join(target);
    let candidate = normalize_path(&candidate);
    if self.all_files.contains(&candidate) {
        ResolvedLink::Found(candidate)
    } else {
        ResolvedLink::Broken
    }
}
```

`normalize_path` collapses `.` and `..` components lexically (no syscalls). This
means it works correctly for paths that don't yet exist on disk (e.g. during a
Quick Fix preview in a future release).

Empty targets (anchor-only links like `[text](#heading)`) are resolved against
the source file itself by the caller before invoking `resolve`.

### Updated `index()`

Step 3 iterates `note.md_links` instead of `note.wiki_links`, skipping empty
targets and external URLs. `LocatedLink` stores `md_link` instead of `wiki_link`.
Step 4 calls `recheck_incoming(&note.path)` instead of
`recheck_links_to(&note.stem)`.

### Updated `add_attachment` / `remove_attachment`

`add_attachment` inserts the absolute path into `all_files` and calls
`recheck_incoming`. `remove_attachment` removes from `all_files`, drops the
`links_to` entry for that path, and returns the affected source files.

### `recheck_incoming()`

Replaces `recheck_links_to`. Scans `by_path` for notes with an `md_link` whose
resolved target (path-relative from that note) is `new_path`, adding them to
`links_to` if not already tracked.

```rust
fn recheck_incoming(&mut self, new_path: &Path) -> AffectedPaths
```

---

## Handler Changes

Only the four v0.1 handlers exist in `dispatch_request`. All v0.2+ handlers
(`handle_hover`, `handle_document_symbols`, `handle_workspace_symbols`,
`handle_code_action`, `handle_code_lens`, `handle_will_rename_files`,
`handle_prepare_rename`, `handle_rename`) are removed.

### `compute_diagnostics`

Iterates `note.md_links`. For each local link (non-URL, non-empty target):

| Resolution                              | Diagnostic                                                                                 |
| --------------------------------------- | ------------------------------------------------------------------------------------------ |
| `Broken`                                | Warning: `Link target not found: 'path/to/note.md'` at `link.target_range`                 |
| `Found` + anchor not in target headings | Warning: `Heading not found: '#anchor' in 'path/to/note.md#anchor'` at `link.anchor_range` |
| `Found` + no anchor                     | No diagnostic                                                                              |

Anchor-only links (`target == ""`) are resolved against the source file itself.

### `handle_completion`

Trigger: the text on the cursor's line immediately before the cursor contains
`](` (the cursor is inside the `()` of a Markdown link).

```rust
pub fn handle_completion(
    params: CompletionParams,
    index: &NoteIndex,
) -> Vec<CompletionItem>
```

Response: one `CompletionItem` per note in the index. `insert_text` is the path
of the target note relative to the source note's directory (computed with
`pathdiff` or manual `Path` arithmetic). When the target has a frontmatter
`title`, the item label is that title; otherwise label equals the relative path.
`filter_text` always equals the relative path so the editor filters by path as
the user types.

Frontmatter completions and tag completions are deferred to later releases.

### `handle_definition`

Finds the `MarkdownLink` at the cursor position using `find_link_at_position`.
Resolves the target. On `Found`:

- If `link.anchor` is `Some(anchor)`, looks up the heading in the target note
  with matching text and returns `Location { uri, range: heading.range }`.
- If `link.anchor` is `None`, returns `Location { uri, range: Range::default() }`
  (top of file).

Returns `None` for broken links, non-link cursor positions, and anchor-only links.

```rust
pub fn handle_definition(
    params: GotoDefinitionParams,
    index: &NoteIndex,
) -> Option<GotoDefinitionResponse>
```

### `handle_references`

1. If cursor is on a `MarkdownLink` with a resolvable target: returns all
   `LocatedLink`s from `index.links_to(resolved_target)`.
2. Otherwise: returns all `LocatedLink`s from `index.links_to(current_path)` —
   backlinks to the current document.

```rust
pub fn handle_references(
    params: ReferenceParams,
    index: &NoteIndex,
) -> Vec<Location>
```

---

## Protocol Handler Changes

`initialize` advertises only v0.1 capabilities:

```rust
ServerCapabilities {
    text_document_sync: Some(TextDocumentSyncCapability::Kind(
        TextDocumentSyncKind::FULL,
    )),
    completion_provider: Some(CompletionOptions {
        trigger_characters: Some(vec!["(".to_string()]),
        ..Default::default()
    }),
    definition_provider: Some(OneOf::Left(true)),
    references_provider: Some(OneOf::Left(true)),
    ..Default::default()
}
```

The completion trigger character changes from `"["` to `"("` — completion fires
as the cursor enters the path portion of a link (after `](`).

`dispatch_request` routes only `Completion::METHOD`, `GotoDefinition::METHOD`,
`References::METHOD`. Unknown methods return a null result as before.

`Config` retains only `index_roots` and `extensions`. `attachments_dir`,
`new_note_dir`, and `frontmatter_schema` are removed until the releases that
introduce them.

`InitOptions` retains only `extensions: Option<Vec<String>>`.

---

## CLI Changes

`cmd_parse` prints standard Markdown links instead of wiki-links:

```
path:  /path/to/note.md
title: My Note

links (2):
  [My Note](../other/note.md)     line 5, cols 0–30
  [Section](./same.md#intro)      line 12, cols 0–35  anchor: intro
```

`cmd_index` prints the workspace link graph using path-relative resolution,
marking each link as resolved or broken.

---

## Testing

### Unit tests

| File              | Test                           | What it verifies                                             |
| ----------------- | ------------------------------ | ------------------------------------------------------------ |
| `parser/tests.rs` | `test_md_link_basic`           | `[text](path.md)` extracts target and ranges                 |
| `parser/tests.rs` | `test_md_link_with_anchor`     | `[text](note.md#section)` splits target/anchor correctly     |
| `parser/tests.rs` | `test_md_link_anchor_only`     | `[text](#heading)` has empty target, anchor/anchor_range set |
| `parser/tests.rs` | `test_md_link_image`           | `![alt](img.png)` sets `is_image` and correct ranges         |
| `parser/tests.rs` | `test_md_link_external_url`    | `[text](https://...)` captured, anchor is None               |
| `parser/tests.rs` | `test_md_link_in_code_block`   | Links inside fenced code blocks are not extracted            |
| `parser/tests.rs` | `test_md_link_target_range`    | `target_range` excludes the `#anchor` portion                |
| `parser/tests.rs` | `test_md_link_anchor_range`    | `anchor_range` covers anchor text, not the `#`               |
| `index/tests.rs`  | `test_resolve_relative`        | Sibling file resolves `Found`                                |
| `index/tests.rs`  | `test_resolve_parent_dir`      | `../other/note.md` resolves correctly                        |
| `index/tests.rs`  | `test_resolve_broken`          | File not in `all_files` resolves `Broken`                    |
| `index/tests.rs`  | `test_resolve_url`             | External URL resolves `Found` without lookup                 |
| `index/tests.rs`  | `test_recheck_incoming`        | Adding the target file after linker clears broken state      |
| `index/tests.rs`  | `test_remove_breaks_links`     | Removing a target marks all linking notes as affected        |
| `index/tests.rs`  | `test_add_attachment_resolves` | Non-note file in `all_files` resolves an attachment link     |

### Integration tests (`tests/lsp.rs`)

| Test                                   | What it verifies                                           |
| -------------------------------------- | ---------------------------------------------------------- |
| `test_completion_returns_all_notes`    | Completion returns one item per indexed note               |
| `test_completion_relative_path`        | `insert_text` is relative to the requesting file           |
| `test_definition_jumps_to_file`        | Go to Definition returns top of target file                |
| `test_definition_jumps_to_heading`     | Go to Definition with `#anchor` navigates to heading line  |
| `test_references_backlinks`            | Find References returns all notes linking to target        |
| `test_broken_link_diagnostic`          | Missing target produces a `WARNING` diagnostic             |
| `test_file_created_clears_diagnostic`  | `didChangeWatchedFiles` Created clears broken-link warning |
| `test_file_deleted_creates_diagnostic` | `didChangeWatchedFiles` Deleted introduces a new warning   |
