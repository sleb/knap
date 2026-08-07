# v0.3.3 Design — Rename for Unindexed Files

## Stories

| Story | Type | Description                                           |
| ----- | ---- | ----------------------------------------------------- |
| #2    | Bug  | Rename silently fails for files absent from NoteIndex |

---

## Goal

When a user triggers heading rename (`F2`) on a file that is not yet in the
`NoteIndex`, the rename dialog silently never appears and no changes are made.
The fix makes `prepare_rename` and `rename` work on any readable `.md` file,
regardless of whether it was indexed at startup or on open.

---

## Bug

### Symptom

`handle_prepare_rename` and `handle_rename` both open with:

```rust
let note = index.get_note(&path)?;   // early exit when None
```

If `path` is absent from `NoteIndex.by_path`, both handlers return `None`. The
LSP client interprets `null` as "rename not available here" and shows nothing.
No error is surfaced.

### When a file can be absent from the index

At startup, `index::build()` crawls all workspace roots. Every `.md` file under
a non-hidden, non-`target`, non-`node_modules` directory is parsed and indexed.
Separately, `on_did_open` indexes any file the editor sends a `didOpen`
notification for.

A file ends up absent from the index when either path fails:

| Scenario                                     | Why the file is absent                                                                                                                                                                     |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Server restart / crash-reconnect             | Some editors do not re-send `didOpen` for already-open buffers after re-attaching. If `workspace_folders` is empty, the startup `build()` also produces an empty index.                    |
| No `workspace_folders` in `initializeParams` | `Config::from_params` collects an empty `index_roots`; `build(&[], …)` returns an empty index. A subsequent `didOpen` normally fixes this, but timing or client compliance can prevent it. |
| Single-file open without workspace           | Same as above — no workspace configured, and the editor may not follow up with `didOpen`.                                                                                                  |

The CLI diagnostic in the bug report (`cargo run -- index docs/ROADMAP.md`) is
misleading: the `index` subcommand expects a _directory_, not a file. Passing a
file causes `std::fs::read_dir` to fail silently and print "0 note(s) indexed".
The file is not actually excluded by the startup crawler.

---

## Fix

### Strategy: disk-parse fallback

Both handlers need a `Note` — specifically its `headings` (for cursor lookup)
and `md_links` (for self-anchor updates). Both values can be obtained by reading
the file from disk and calling `parser::parse`. This is safe because:

- `prepare_rename` only reads heading positions; no index data is needed.
- `rename` reads headings and self-anchor links from the note, then uses
  `index.links_to(&path)` for incoming-link updates. If the file is absent from
  the index, `links_to` returns `&[]` — heading text and self-anchors are still
  updated correctly; incoming links from other files are not (see Limitation below).

The fallback does not mutate the index, so it does not affect diagnostics or
link resolution for other files.

### `handle_prepare_rename`

Replace the early-exit `get_note?` with a two-branch lookup:

```rust
pub fn handle_prepare_rename(
    params: TextDocumentPositionParams,
    index: &NoteIndex,
) -> Option<PrepareRenameResponse> {
    let path = uri_to_path(&params.text_document.uri)?;

    let disk_note;
    let note: &parser::Note = match index.get_note(&path) {
        Some(n) => n,
        None => {
            let content = std::fs::read_to_string(&path).ok()?;
            disk_note = parser::parse(&path, &content);
            &disk_note
        }
    };

    let pos = params.position;
    let heading = note.headings.iter().find(|h| {
        h.range.start.line <= pos.line && pos.line <= h.range.end.line
    })?;
    Some(PrepareRenameResponse::RangeWithPlaceholder {
        range: heading.text_range,
        placeholder: heading.text.clone(),
    })
}
```

### `handle_rename`

Same two-branch lookup replacing the early-exit `get_note?`:

```rust
pub fn handle_rename(params: RenameParams, index: &NoteIndex) -> Option<WorkspaceEdit> {
    let path = uri_to_path(&params.text_document_position.text_document.uri)?;

    let disk_note;
    let note: &parser::Note = match index.get_note(&path) {
        Some(n) => n,
        None => {
            let content = std::fs::read_to_string(&path).ok()?;
            disk_note = parser::parse(&path, &content);
            &disk_note
        }
    };

    // remainder unchanged — heading text (a), self-anchors (b), incoming links (c)
    ...
}
```

### Limitation: incoming links for unindexed files

Step (c) of `handle_rename` iterates `index.links_to(&path)`. If the file is
not in the index, this returns an empty slice — incoming links from other files
are not updated. This is acceptable: the rename dialog appears, the heading text
and self-anchor links are rewritten correctly, and the user at least does not
lose data. A follow-up re-index (server restart or file save) resolves the
situation fully.

This limitation is inherent: correctly tracking `links_to` for a file requires
that all other workspace files have been indexed first. On-demand indexing within
a single handler cannot rebuild this retroactively.

---

## Implementation notes

`handle_prepare_rename` and `handle_rename` currently import only
`crate::index`. The fallback requires `crate::parser`, which must be added to
the `use` block in `src/handlers.rs`.

No changes to `NoteIndex`, `Config`, the server loop, or the parser.

---

## Alternatives considered

**On-demand indexing in `dispatch_request`.** Change `dispatch_request` to
`&mut NoteIndex` and index the file before delegating to the handler. Rejected:
`index.index(note)` does call `recheck_incoming`, which would populate `links_to`
from already-indexed notes — so in the normal case (workspace indexed at startup)
this would give correct incoming-link updates. But it adds mutable-borrow
complexity to the server's message loop and couples request dispatch to
side-effecting index mutations. The disk-parse fallback keeps the handlers
self-contained and the server loop unchanged.

**Error response instead of silent `null`.** Return a structured error when the
file is not indexed, so the editor can show a message. Rejected: the disk-parse
fallback means the file _can_ be handled — there is no reason to fail. Silent
null was the bug; a real response is the fix.

---

## Testing

The fallback path requires actual disk I/O. Tests that exercise it create a
temporary file with `std::env::temp_dir()`, write a heading, call the handler
with an empty `NoteIndex`, and assert a non-`None` response.

### Unit tests (new, in `src/handlers.rs`)

| Test                                       | What it verifies                                                             |
| ------------------------------------------ | ---------------------------------------------------------------------------- |
| `prepare_rename_disk_fallback`             | Empty index + real temp file with a heading → `Some(RangeWithPlaceholder)`   |
| `prepare_rename_disk_fallback_off_heading` | Empty index + real temp file, cursor on prose line → `None`                  |
| `rename_disk_fallback_edits_heading`       | Empty index + real temp file → workspace edit contains heading text rewrite  |
| `rename_disk_fallback_no_incoming_links`   | Empty index + real temp file → workspace edit has no entries for other files |

### Existing tests unaffected

All existing `prepare_rename_*` and `rename_heading_*` tests seed the index
and exercise the `get_note` path. They remain unchanged and continue to verify
indexed-file behaviour.
