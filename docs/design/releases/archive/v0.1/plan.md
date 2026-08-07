# v0.1 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                        | Status  | Notes |
| --------------------------- | ------- | ----- |
| 1 — Overhaul Parser         | ✅ Done |       |
| 2 — Overhaul Note Index     | ✅ Done |       |
| 3 — Rewrite Handlers        | ✅ Done |       |
| 4 — Update Protocol Handler | ✅ Done |       |
| 5 — Update CLI              | ✅ Done |       |
| 6 — Integration Tests       | ✅ Done |       |

---

## Step 1 — Overhaul Parser

Remove wiki-link infrastructure and expand `MarkdownLink` with the fields needed
for diagnostics and future rename support. This step is entirely self-contained —
the index and handlers don't compile yet, but the parser and its tests do.

**Deliverables:**

- Remove `WikiLink` struct and `scan_wiki_links` function from `src/parser/mod.rs`
- Remove `stem` and `wiki_links` fields from `Note`
- Add `anchor: Option<String>`, `target_range: LspRange`,
  `anchor_range: Option<LspRange>` to `MarkdownLink`
- Update `extract_body_elements` to return `(Vec<MarkdownLink>, Vec<Heading>)`
  and populate the new fields by scanning raw source bytes within each link
  event's span (locate `(`, optional `#`, and `)`)
- Update `parse()` to match the new signature

**Unit tests:**

| Test                                    | What it verifies                                                 |
| --------------------------------------- | ---------------------------------------------------------------- |
| `test_md_link_basic`                    | `[text](path.md)` extracts target and all ranges                 |
| `test_md_link_with_anchor`              | `[text](note.md#section)` splits target/anchor, ranges correct   |
| `test_md_link_anchor_only`              | `[text](#heading)` has empty target, anchor and anchor_range set |
| `test_md_link_image`                    | `![alt](img.png)` sets `is_image` and correct ranges             |
| `test_md_link_external_url`             | `[text](https://...)` captured; anchor is None                   |
| `test_md_link_in_code_block`            | Links inside fenced code blocks are not extracted                |
| `test_md_link_target_range_no_anchor`   | `target_range` covers full path when no anchor present           |
| `test_md_link_target_range_with_anchor` | `target_range` stops before `#`                                  |
| `test_md_link_anchor_range`             | `anchor_range` covers anchor text only, not the `#`              |

> **Manual checkpoint:** `cargo test -p knap -- parser` — all parser tests pass.
> `cargo build` compiles (index and handlers may need stubs for removed fields).

---

## Step 2 — Overhaul Note Index

Replace stem/filename resolution with path-relative resolution. After this step
the index is functional and its tests pass independently of the handlers.

**Deliverables:**

- Replace `by_stem` and `by_filename` with `all_files: HashSet<PathBuf>` in
  `src/index/mod.rs`
- Remove `ResolvedLink::Ambiguous`
- Change `resolve` signature to `pub fn resolve(&self, source: &Path, target: &str) -> ResolvedLink`
- Add `normalize_path(path: &Path) -> PathBuf` (lexical `..` / `.` collapsing,
  no syscalls)
- Add `looks_like_url(target: &str) -> bool` (matches `https://`, `http://`,
  `ftp://`, `mailto:`)
- Update `LocatedLink` to use `md_link: MarkdownLink` instead of
  `wiki_link: WikiLink`
- Rewrite `index()` to iterate `note.md_links`, skip empty/URL targets,
  store `LocatedLink { md_link }`
- Replace `recheck_links_to` with `recheck_incoming(new_path: &Path)` — scans
  `by_path` for notes whose `md_links` resolve to `new_path`, adds them to
  `links_to` if not yet tracked
- Rewrite `add_attachment` to insert into `all_files` and call `recheck_incoming`
- Rewrite `remove_attachment` to remove from `all_files`, drop the `links_to`
  entry, and return affected source files
- Update `build()` to match

**Unit tests:**

| Test                            | What it verifies                                              |
| ------------------------------- | ------------------------------------------------------------- |
| `test_resolve_relative`         | Sibling file at `./note.md` resolves `Found`                  |
| `test_resolve_parent_dir`       | `../other/note.md` resolves correctly via `normalize_path`    |
| `test_resolve_broken`           | File absent from `all_files` resolves `Broken`                |
| `test_resolve_url`              | `https://` target resolves `Found` without a filesystem check |
| `test_index_populates_links_to` | Indexing a linking note registers it in `links_to`            |
| `test_recheck_incoming`         | Adding the target file after the linker fixes `links_to`      |
| `test_remove_breaks_incoming`   | Removing a target marks all linking notes as affected         |
| `test_add_attachment_resolves`  | Non-note file in `all_files` resolves an attachment link      |

> **Manual checkpoint:** `cargo test -p knap -- index` — all index tests pass.
> `cargo clippy -- -D warnings` clean.

---

## Step 3 — Rewrite Handlers

Rewrite all four v0.1 handlers for standard Markdown link semantics; delete the
v0.2+ handlers that no longer compile.

**Deliverables:**

- Rewrite `compute_diagnostics` in `src/handlers.rs` to iterate `note.md_links`;
  use `link.target_range` for broken-link diagnostics and `link.anchor_range`
  for missing-anchor diagnostics
- Rewrite `handle_completion` to trigger on `](` immediately before the cursor;
  compute `insert_text` as a path relative to the source file's directory
- Rewrite `handle_definition` using `find_link_at_position`; resolve the target;
  navigate to `heading.range` when an anchor is present, `Range::default()` otherwise
- Rewrite `handle_references` returning `index.links_to(resolved_target)` for a
  link under the cursor, or `index.links_to(current_path)` as fallback
- Delete: `handle_hover`, `handle_document_symbols`, `handle_workspace_symbols`,
  `handle_code_action`, `handle_code_lens`, `handle_will_rename_files`,
  `handle_prepare_rename`, `handle_rename`
- Ensure `uri_to_path` and `path_to_uri` utilities are present

**Unit tests:**

| Test                              | What it verifies                                                |
| --------------------------------- | --------------------------------------------------------------- |
| `test_diagnostics_broken_link`    | Warning on `Broken` resolution at correct `target_range`        |
| `test_diagnostics_missing_anchor` | Warning when anchor text not in target headings                 |
| `test_diagnostics_external_url`   | No diagnostic for external URL links                            |
| `test_completion_trigger`         | Completions returned when line contains `](` before cursor      |
| `test_completion_no_trigger`      | No completions when cursor is not inside `[text](`              |
| `test_completion_relative_path`   | `insert_text` is path relative to the source file's directory   |
| `test_completion_title_label`     | Note with frontmatter `title` uses the title as the item label  |
| `test_definition_top_of_file`     | Go to Definition without anchor returns top of target file      |
| `test_definition_to_heading`      | Go to Definition with anchor returns the matching heading range |
| `test_references_from_link`       | References on a link target → all backlinks to that target      |
| `test_references_fallback`        | No link at cursor → backlinks to the current document           |

> **Manual checkpoint:** Open a real Markdown vault in an editor connected to
> the built server. Type `[text](` and confirm relative-path completions appear.
> `Go to Definition` on a link jumps to the target file. `Find References`
> on a file shows its backlinks. A broken link shows a warning diagnostic.

---

## Step 4 — Update Protocol Handler

Strip advertised capabilities to the v0.1 set and remove config fields that
aren't needed until later releases. After this step the server is shippable.

**Deliverables:**

- Update `ServerCapabilities` in `src/server/mod.rs`: keep `text_document_sync`,
  `completion_provider` (trigger char `"("`), `definition_provider`,
  `references_provider`; remove all others
- Remove `attachments_dir`, `new_note_dir`, and `frontmatter_schema` from
  `Config` and `InitOptions`
- Update `dispatch_request` to route only `Completion::METHOD`,
  `GotoDefinition::METHOD`, `References::METHOD`
- Remove all call sites for deleted handlers
- Remove now-unused imports

**Unit tests:**

_No new unit tests — the protocol handler is covered by the integration tests in Step 6._

> **Manual checkpoint:** Restart the server and check the editor's LSP client.
> Confirm completion, go-to-definition, and find-references capabilities are
> advertised. Confirm no hover, code action, rename, or code lens capabilities
> appear.

---

## Step 5 — Update CLI

Rewrite `cmd_parse` and `cmd_index` to reflect the new data model.

**Deliverables:**

- Update `cmd_parse` in `src/cli.rs`: remove wiki-link output; print each
  `MarkdownLink` with its target, anchor (if present), and byte-range-derived
  line/column positions
- Update `cmd_index` in `src/cli.rs`: adapt to the new index structure — no
  `by_stem`, no `Ambiguous`; print resolved and broken links for each note

**Unit tests:**

_No unit tests — CLI output verified manually._

> **Manual checkpoint:** `knap parse tests/fixtures/example.md` shows a clean
> `md_links` table with correct line numbers. `knap index tests/fixtures/` shows
> the full workspace link graph with resolved/broken labels.

---

## Step 6 — Integration Tests

End-to-end tests over the full LSP message loop. Always the last step.

**Deliverables:**

- `tests/lsp.rs` with all integration tests listed below
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                   | What it verifies                                                 |
| -------------------------------------- | ---------------------------------------------------------------- |
| `test_completion_returns_all_notes`    | Completion at `](` returns one item per indexed note             |
| `test_completion_relative_path`        | `insert_text` is relative to the requesting file                 |
| `test_definition_jumps_to_file`        | Go to Definition navigates to top of target file                 |
| `test_definition_jumps_to_heading`     | Go to Definition with `#anchor` navigates to heading line        |
| `test_references_backlinks`            | Find References returns all notes linking to the target          |
| `test_broken_link_diagnostic`          | Missing target produces a `WARNING` diagnostic                   |
| `test_file_created_clears_diagnostic`  | `didChangeWatchedFiles` Created event clears broken-link warning |
| `test_file_deleted_creates_diagnostic` | `didChangeWatchedFiles` Deleted event introduces a new warning   |

> **Manual checkpoint (full session):** Open a real vault. Walk the complete
> golden path: insert a link using completion, jump to the target with Go to
> Definition, run Find References to see backlinks, introduce a broken link and
> observe the diagnostic, create the missing file externally and confirm the
> diagnostic clears. Confirm that earlier sessions (v0.8.x) are not regressed in
> any capability that carries over.

---

## Done — v0.1 complete

| Story  | Feature                               | Delivered in step     |
| ------ | ------------------------------------- | --------------------- |
| US-01  | Path completions inside `[text](`     | Steps 3 + 4           |
| US-02  | Go to Definition on standard links    | Steps 3 + 4           |
| US-05  | Navigation regardless of display text | Step 3                |
| US-03  | Find References on a file             | Steps 3 + 4           |
| US-07  | Broken link diagnostics               | Step 3                |
| US-16  | Incremental file watching             | Step 4 (carried over) |
| US-D01 | `knap parse <file>`                   | Step 5                |
| US-D02 | `knap index <dir>`                    | Step 5                |
