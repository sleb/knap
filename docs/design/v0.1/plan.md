# v0.1 Implementation Plan

Describes the order in which components are built, what is tested after each
step, and the checkpoints where the server should be manually verified against a
real editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                 | Status  | Notes                                                                                       |
| -------------------- | ------- | ------------------------------------------------------------------------------------------- |
| 1 — Project scaffold | ✅ Done | `src/lib.rs` + binary split; all deps locked                                                |
| 2 — Parser           | ✅ Done | 18/18 tests passing; design doc updated to reflect exclusion-zone approach                  |
| 3 — Note Index       | ✅ Done | 11/11 tests passing; `remove_internal` drops `links_to[path]` explicitly                    |
| 4 — Server skeleton  | ✅ Done | 3/3 tests passing; workspace folder parsing deferred to Step 5                              |
| 5 — Document sync    | ✅ Done | 5/5 tests passing; `url` crate added for URI↔PathBuf conversion                             |
| 6 — Diagnostics      | ✅ Done | 6/6 tests passing; `crossbeam-channel` added as direct dep for sender type                  |
| 7 — Completion       | ✅ Done | 4/4 tests passing; `dispatch_request` now takes `&NoteIndex` for routing                    |
| 8 — Go to Definition | ✅ Done | 6/6 tests passing; `[[stem\|alias]]` and `[[stem#anchor]]` links navigate to file top (0,0) |
| 9 — Find References  | ✅ Done | 4/4 tests passing; returns all `LocatedLink` entries from `linksTo` for the resolved target |

### Implementation notes

**Step 2:** pulldown-cmark fragments `[[note]]` into individual character `Text`
events, making it impossible to scan within text events directly. The parser
instead uses pulldown-cmark only to collect exclusion zones (code blocks, inline
code spans) by byte range, then does a raw scan of the full content string.
Design doc updated to match.

**Step 2:** Switched from a single-binary crate to a `lib.rs` + thin `main.rs`
split. This avoids false dead_code warnings from clippy (all `pub` items in the
library are treated as part of the public API) and makes integration tests
straightforward — they can `use knap::parser` directly.

**Step 3:** The design doc's `remove_internal` pseudo-code collects incoming
links into `affected` but never removes the `links_to[path]` entry itself. Fixed
by using `links_to.remove(path)` to collect and drop in one step — without this,
`links_to(path)` still returns stale entries after the note is removed.

**Step 3:** `build()` takes `extensions: &[&str]` (string slices) rather than
`&[String]` — sufficient for v0.1 where extensions are compile-time constants,
and avoids allocation at call sites.

**Step 4:** lsp-types 0.97 uses its own `Uri` struct (not `url::Url`) that lacks
`to_file_path()`. Rather than pulling in the `url` crate as a direct dependency,
`Config::from_params` leaves `index_roots` empty for now — the conversion is
deferred to Step 5 where it is actually needed. LSP method name constants
(`METHOD`) are defined on traits (`lsp_types::request::Request`,
`lsp_types::notification::Notification`) that conflict with the same-named types
from `lsp_server`; string literals are used instead to avoid the ambiguity.

**Step 5:** Added `url = "2"` as a direct dependency. `lsp_types::Uri` (backed
by `fluent_uri::Uri<String>`) has `as_str()` via `Deref` and `FromStr`, which
lets `uri_to_path` round-trip through `url::Url`. `FileChangeType` is a newtype
struct with associated constants (`CREATED`, `CHANGED`, `DELETED`), not an enum,
so pattern matching uses `if`/`else if` equality checks rather than match arms.
`DidCloseTextDocument` is a no-op (the on-disk version was already indexed);
`didChangeWatchedFiles` for created/changed files reads from disk and skips with
a warning if the file cannot be read.

**Step 6:** Added `crossbeam-channel = "0.5"` as a direct dependency so
`publish_diagnostics` can take `&crossbeam_channel::Sender<Message>` — the type
of `Connection.sender` from `lsp-server`. `DiagnosticSeverity` is also a newtype
struct (like `FileChangeType`) with associated constants, not an enum. Tests use
a synchronising-request pattern: after sending a notification, a dummy
completion request is sent; all `textDocument/publishDiagnostics` notifications
collected before the completion response are known to be causally ordered after
the notification.

---

## Step 1 — Project scaffold

Create the Cargo workspace, add all v0.1 dependencies, and establish the module
structure.

**Deliverables:**

- `Cargo.toml` with `lsp-server`, `lsp-types`, `pulldown-cmark`, `serde`,
  `serde_json`, `anyhow`
- Empty modules: `parser`, `index`, `handlers`, `server`
- `main.rs` that compiles and exits cleanly

**Tests:** none yet — nothing to test.

---

## Step 2 — Parser

Implement the parser in isolation: `LineIndex`, the `Note` and `WikiLink` types,
and `parse(path, content) → Note`.

Reference: `docs/design/components/parser.md`

**Deliverables:**

- `parser::parse()` — full implementation including `extract_wiki_links` and
  `scan_wiki_links`
- `parser::LineIndex` — byte offset → LSP position conversion
- `cli::cmd_parse` + `knap parse <file>` subcommand (US-D01)

**Unit tests** (`src/parser/tests.rs`):

| Test                        | What it verifies                                                               |
| --------------------------- | ------------------------------------------------------------------------------ |
| `basic_link`                | `[[note]]` produces one `WikiLink` with correct stem and ranges                |
| `multiple_links`            | Two links on one line both captured                                            |
| `link_in_fenced_code_block` | No links extracted from ` ``` ` blocks                                         |
| `link_in_inline_code`       | No links extracted from `` `[[note]]` ``                                       |
| `aliased_link_ignored`      | `[[note\|alias]]` produces no `WikiLink` in v0.1                               |
| `heading_anchor_ignored`    | `[[note#heading]]` produces no `WikiLink` in v0.1                              |
| `empty_link_ignored`        | `[[]]` produces nothing                                                        |
| `unclosed_link_ignored`     | `[[note` with no `]]` produces nothing                                         |
| `link_ranges`               | `range` covers `[[note]]`, `inner_range` covers `note` only                    |
| `line_index_positions`      | Byte offsets map to correct line/character positions across multi-line content |
| `stem_from_path`            | `my-note.md` → stem `"my-note"`                                                |

---

## Step 3 — Note Index

Implement the `NoteIndex` with all three internal maps and the full `index()`,
`remove()`, `resolve()` surface. The index is pure data — no LSP, no filesystem.

Reference: `docs/design/components/note-index.md`

**Deliverables:**

- `index::NoteIndex` with `index()`, `remove()`, `resolve()`, `get_note()`,
  `all_notes()`, `links_to()`
- `index::IndexDelta` and `index::LocatedLink`
- `index::build()` — initial crawl from a list of roots
- `cli::cmd_index` + `knap index <dir>` subcommand (US-D02)

**Unit tests** (`src/index/tests.rs`):

| Test                             | What it verifies                                                                  |
| -------------------------------- | --------------------------------------------------------------------------------- |
| `resolve_found`                  | Single file with stem `"foo"` resolves to `Found`                                 |
| `resolve_broken`                 | Stem with no matching file resolves to `Broken`                                   |
| `resolve_ambiguous`              | Two files with same stem resolve to `Ambiguous`                                   |
| `index_replaces_existing`        | Re-indexing a path updates links and stem map cleanly                             |
| `remove_clears_all_maps`         | Removed note disappears from all three maps                                       |
| `links_to_populated`             | After indexing `a.md` linking to `b.md`, `links_to("b.md")` returns the link      |
| `broken_link_heals_on_add`       | Index `a.md` with `[[b]]`, then index `b.md` — `links_to("b.md")` now populated   |
| `link_breaks_on_remove`          | Remove `b.md` — `links_to("b.md")` is cleared                                     |
| `delta_includes_affected`        | `index()` delta includes the indexed file and any files whose resolutions changed |
| `remove_delta_includes_incoming` | Removing `b.md` delta includes `a.md` (which linked to it)                        |
| `ambiguous_becomes_found`        | Two `foo.md` files; remove one → resolves to `Found`                              |

---

## Step 4 — Server skeleton

Wire up `lsp-server`, implement the lifecycle (`initialize` → `initialized` →
loop → `shutdown`/`exit`), and stub all v0.1 request handlers to return
empty/null results. The server starts, handshakes, and shuts down cleanly — but
does nothing useful yet.

Reference: `docs/design/components/transport.md`,
`docs/design/components/protocol-handler.md`

**Deliverables:**

- `main.rs` — `Connection::stdio()` + `server::run()`
- `server::run()` — full lifecycle and message loop
- `Config` resolution from `initializationOptions`
- Stub handlers returning null for all v0.1 methods
- File watcher registration on `initialized`
- Stderr logging via `log` + `env_logger` (add both to `Cargo.toml`)

**Logging:**

All log output goes to stderr — stdout is owned by the LSP JSON-RPC framing and
must not be touched. The `log` facade + `env_logger` backend is the
implementation:

```rust
// main.rs, before Connection::stdio()
env_logger::Builder::from_env(
    env_logger::Env::default().filter_or("KNAP_LOG", "info")
).init();
```

Using `KNAP_LOG` rather than `RUST_LOG` avoids conflicts when the editor or its
plugins also use `env_logger`.

Log the following lifecycle events (at `info` level unless noted):

| Event                         | Message                                                     |
| ----------------------------- | ----------------------------------------------------------- |
| Server start                  | `knap starting`                                             |
| `initialize` received         | `initialize: client={name} version={version}`               |
| `initialized` notification    | `initialized: registering file watcher, crawling {n} roots` |
| Every request dispatched      | `request: method={method} id={id}` (`debug`)                |
| Every notification dispatched | `notification: method={method}` (`debug`)                   |
| `shutdown` received           | `shutdown requested`                                        |
| Main loop exit                | `exiting`                                                   |

Logging at `debug` is verbose enough to reconstruct the full message sequence
after a crash, which makes it the right level for diagnosing startup failures or
hangs without flooding the editor's log in normal use (where `KNAP_LOG=info` is
the default).

**Integration tests** (`tests/lifecycle.rs`):

| Test                           | What it verifies                                                                          |
| ------------------------------ | ----------------------------------------------------------------------------------------- |
| `initialize_shutdown`          | Server responds to `initialize` with correct capabilities, then `shutdown`/`exit` cleanly |
| `capabilities_advertised`      | `InitializeResult` includes completion (trigger `[`), definition, references              |
| `unknown_request_returns_null` | An unrecognised method gets a null result, not an error                                   |

> **Manual checkpoint — in-process (no editor needed):**
>
> ```
> cargo run -- check          # protocol output only
> KNAP_LOG=debug cargo run -- check  # with lifecycle log interleaved
> ```
>
> Expected output: 11 checks, all `[ok]`. With `KNAP_LOG=debug` you see the
> server's lifecycle log lines interleaved with the check results —
> `initialize`, `initialized`, each request dispatched, `shutdown requested`,
> `exiting` — confirming both the protocol behaviour and the log output in one
> pass. Exit code is non-zero if any check fails.
>
> **Manual checkpoint — Zed (real editor):**
>
> In `~/.config/zed/settings.json` configure the binary:
>
> ```json
> "lsp": {
>   "knap": {
>     "binary": { "path": "/path/to/knap", "arguments": [] }
>   }
> }
> ```
>
> Set `KNAP_LOG=debug` before launching (`KNAP_LOG=debug zed` from a terminal,
> or `launchctl setenv KNAP_LOG debug` on macOS). Zed pipes language server
> stderr to its own log — run **Zed: Open Log** or
> `tail -f ~/Library/Logs/Zed/Zed.log`. Open a `.md` file; the log should
> contain `knap starting`, `initialize: client=Zed`, and
> `initialized: registering file watcher`. No features work yet, but no panics
> and the full lifecycle sequence visible in the log.

---

## Step 5 — Document sync + index wiring

Connect `textDocument/didOpen`, `textDocument/didChange`,
`textDocument/didClose`, and `workspace/didChangeWatchedFiles` to the
`NoteIndex`. After this step the index is populated and live; diagnostics are
still not published.

**Deliverables:**

- `on_did_open`, `on_did_change`, `on_did_close`, `on_did_change_watched_files`
  in the server
- Initial crawl on `initialized` using `index::build()`

**Integration tests** (`tests/sync.rs`):

| Test                      | What it verifies                                                     |
| ------------------------- | -------------------------------------------------------------------- |
| `did_open_indexes_note`   | After `didOpen`, `get_note()` returns the parsed note                |
| `did_change_updates_note` | After `didChange` with new content, the index reflects the new links |
| `did_close_retains_note`  | After `didClose`, the note is still in the index                     |
| `watched_file_created`    | `didChangeWatchedFiles` created event adds note to index             |
| `watched_file_deleted`    | `didChangeWatchedFiles` deleted event removes note from index        |

---

## Step 6 — Diagnostics

Implement `publish_diagnostics` and `compute_diagnostics`. Hook them into the
document sync handlers via `IndexDelta`. After this step broken links surface as
warnings in the editor.

Reference: `docs/design/components/handlers.md` (Diagnostics section)

**Deliverables:**

- `handlers::publish_diagnostics()` and `handlers::compute_diagnostics()`
- Called from all document sync handlers with the `IndexDelta` they return

**Integration tests** (`tests/diagnostics.rs`):

| Test                              | What it verifies                                                            |
| --------------------------------- | --------------------------------------------------------------------------- |
| `broken_link_produces_warning`    | Opening a file with `[[missing]]` publishes a warning diagnostic            |
| `valid_link_no_diagnostic`        | Opening a file with a valid link publishes no diagnostic                    |
| `ambiguous_link_produces_warning` | Two files with same stem produces an ambiguous warning                      |
| `diagnostic_clears_on_fix`        | Creating the missing file clears the diagnostic                             |
| `diagnostic_range_is_stem_only`   | Warning range covers the stem text, not the `[[` `]]` brackets              |
| `cascade_on_delete`               | Deleting a linked file publishes a diagnostic in the file that linked to it |

> **Manual checkpoint:** open a `.md` file with `[[broken-link]]`. The editor
> should show a warning. Create `broken-link.md`. The warning should clear.

---

## Step 7 — Completion

Implement the completion handler and wire it into the request router.

Reference: `docs/design/components/handlers.md` (Completion section)

**Deliverables:**

- `handlers::handle_completion()`
- Trigger check: returns empty unless cursor is preceded by `[[`

**Integration tests** (`tests/completion.rs`):

| Test                              | What it verifies                              |
| --------------------------------- | --------------------------------------------- |
| `completion_after_double_bracket` | Typing `[[` returns one item per indexed note |
| `completion_after_single_bracket` | Typing `[` returns no items                   |
| `completion_includes_all_notes`   | Three notes in index → three completion items |
| `completion_item_is_file_kind`    | Each item has `kind: File`                    |

> **Manual checkpoint:** type `[[` in a `.md` file. A completion menu should
> appear listing the other notes in the workspace.

---

## Step 8 — Go to Definition

Implement the definition handler.

Reference: `docs/design/components/handlers.md` (Go to Definition section)

**Deliverables:**

- `handlers::handle_definition()`

**Integration tests** (`tests/definition.rs`):

| Test                           | What it verifies                                                              |
| ------------------------------ | ----------------------------------------------------------------------------- |
| `definition_on_valid_link`     | Cursor on `[[note]]` returns `Location` pointing to `note.md` at position 0,0 |
| `definition_on_broken_link`    | Cursor on `[[missing]]` returns null                                          |
| `definition_off_link`          | Cursor on plain text returns null                                             |
| `definition_on_ambiguous_link` | Returns null (diagnostic already flags it)                                    |

> **Manual checkpoint:** `gd` (or equivalent) on a `[[link]]` navigates to the
> target file.

---

## Step 9 — Find References

Implement the references handler.

Reference: `docs/design/components/handlers.md` (Find References section)

**Deliverables:**

- `handlers::handle_references()`

**Integration tests** (`tests/references.rs`):

| Test                                | What it verifies                                                      |
| ----------------------------------- | --------------------------------------------------------------------- |
| `references_on_link`                | Cursor on `[[b]]` in `a.md` returns all locations that link to `b.md` |
| `references_returns_correct_ranges` | Returned ranges point to the `[[...]]` range, not line 0              |
| `references_off_link`               | Cursor on plain text returns empty list                               |
| `references_multiple_sources`       | Two files link to same target — both locations returned               |

> **Manual checkpoint:** with multiple notes linking to a common note, `gr` (or
> equivalent) on any `[[link]]` shows all the linking locations.

---

## Done — v0.1 complete

At this point all five user stories are implemented and tested:

| Story                           | Delivered in step |
| ------------------------------- | ----------------- |
| US-01 `[[` completion           | Step 7            |
| US-02 Go to Definition          | Step 8            |
| US-03 Find References           | Step 9            |
| US-07 Broken link diagnostics   | Step 6            |
| US-16 Incremental index updates | Step 5            |

Final check before tagging: run the full test suite, then do a manual end-to-end
session — open the workspace, verify all four features work together, introduce
and fix a broken link, confirm diagnostics and navigation stay consistent
throughout.
