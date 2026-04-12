# v0.1 Design — MVP: Navigate your workspace

Covers the implementation of the five stories in the v0.1 release:

| Story | Feature                                        |
| ----- | ---------------------------------------------- |
| US-01 | `[[` completion for all notes in the workspace |
| US-02 | Go to Definition on `[[wiki-link]]`            |
| US-03 | Find References on a file                      |
| US-07 | Broken link diagnostics                        |
| US-16 | Incremental index updates                      |

**Out of scope for v0.1:** aliased links (`[[Note|alias]]`), heading anchors (`[[Note#Heading]]`), tag support, hover previews, rename refactoring, frontmatter parsing.

---

## Wiki-link syntax

v0.1 supports the basic form only:

```
[[stem]]
```

Where `stem` is the filename without extension (e.g. `[[my-note]]` targets `my-note.md`). The `[[` sequence is the completion trigger. Any `[[...]]` where the content contains a `|` or `#` is left unparsed — those forms are introduced in later releases.

A valid wiki-link:

- Opens with `[[`
- Closes with `]]` on the same line
- Contains only the stem: no `|`, no `#`, no whitespace-only content
- Is not inside a fenced code block or inline code span

---

## Data structures

### Note

The parsed representation of a single file. Produced by the Parser; stored in the index.

```
Note {
  path: string          // absolute path
  stem: string          // filename without extension, e.g. "my-note"
  wikiLinks: WikiLink[]
  content: string       // raw source text, retained for completion trigger checking
}

WikiLink {
  stem: string          // the target stem as written, e.g. "other-note"
  range: Range          // full range of [[...]] including brackets
  innerRange: Range     // range of stem text only, for diagnostics
}
```

Frontmatter and headings are not parsed in v0.1.

### Note Index internals

Three maps maintained in sync:

```
byPath:  Map<path, Note>        // primary store
byStem:  Map<stem, path[]>      // stem → all files with that stem (for resolution)
linksTo: Map<path, WikiLink[]>  // target path → all links pointing to it (reverse index)
```

`byStem` values are arrays because two files can share a stem (e.g. `notes/foo.md` and `archive/foo.md`). A stem that maps to exactly one path is resolved; more than one is ambiguous.

`linksTo` is built by resolving each `WikiLink.stem` at index time and recording the result. Unresolvable links are not entered into `linksTo` (they become diagnostics instead).

---

## Startup sequence

```
initialize received
  → parse workspaceFolders + initializationOptions into Config
  → respond to initialize with server capabilities

initialized received
  → register file watcher: { globPattern: "**/*.md" } (adjusted by Config.extensions)
  → crawl all files matching Config across workspace folders
  → for each file: parse → index
  → publish diagnostics for all broken links found
```

The initial crawl is the only time the server reads from disk in bulk. After that, all updates are incremental.

---

## Dependencies

| Crate            | Purpose                                           |
| ---------------- | ------------------------------------------------- |
| `pulldown-cmark` | Markdown parsing — event stream with source spans |

```toml
[dependencies]
pulldown-cmark = "0.12"
```

YAML frontmatter parsing is not needed in v0.1 and will be added in v0.4.

---

## Parser

Parses a file's text content into a `Note` using `pulldown-cmark`'s event stream. Runs on a single file; has no access to the index.

`pulldown-cmark` does not know about `[[wiki-link]]` syntax (it's not standard Markdown), so links are extracted by scanning within `Text` events. What the parser gives us is correct context: we know exactly when we're inside a code block or inline code span, so we never scan for wiki-links in those positions.

**Algorithm:**

1. Create a `pulldown-cmark` parser with `into_offset_iter()` to get `(Event, Range<usize>)` pairs
2. Walk the event stream, tracking a `in_code` boolean:
   - `Start(CodeBlock)` / `End(CodeBlock)` → flip `in_code`
   - `Code(_)` (inline code) → emit nothing, skip
   - `Text(text)` when `in_code` is false → scan `text` for wiki-links (see below)
3. For each `Text` event, scan for `[[` / `]]` pairs on the same line:
   - Skip if inner content contains `|` or `#`
   - Use the event's byte offset + inner offset to compute the link's byte range
   - Convert byte range to LSP `Range` (line/column) using a newline index built from the source

**Byte offset → LSP position conversion:**

Build a `Vec<usize>` of newline byte positions once per file. A byte offset maps to a line by binary search, and a column by subtracting the line's start offset. This is done once after parsing, not per-link.

---

## Note Index operations

### `index(note: Note)`

1. If a `Note` already exists for `note.path`, call `remove(note.path)` first
2. Store `note` in `byPath`
3. Add `note.path` to `byStem[note.stem]`
4. For each `WikiLink` in `note.wikiLinks`:
   - Resolve the stem (see below)
   - If resolved to exactly one path, append the link to `linksTo[resolvedPath]`

### `remove(path: string)`

1. Look up the existing `Note` in `byPath`
2. Remove `path` from `byStem[note.stem]`
3. For each `WikiLink` in the note: remove from `linksTo[resolvedPath]`
4. Delete from `byPath`

### `resolve(stem: string) → ResolvedLink`

```
ResolvedLink =
  | { status: "found",     path: string }
  | { status: "ambiguous", paths: string[] }
  | { status: "broken" }
```

Look up `byStem[stem]`:

- Length 0 or missing → `broken`
- Length 1 → `found`
- Length > 1 → `ambiguous`

---

## LSP handlers

### Completion (`textDocument/completion`)

**Trigger:** the client sends a completion request when the user types `[` (registered as a trigger character). The handler checks whether the cursor is preceded by `[[` on the same line; if not, returns no results.

**Response:** one `CompletionItem` per note in the index.

```
CompletionItem {
  label:      note.stem
  kind:       File (17)
  insertText: note.stem   // just the stem; the closing ]] is not inserted
}
```

No filtering — the editor handles fuzzy matching against what the user has typed so far.

---

### Go to Definition (`textDocument/definition`)

1. Find the `WikiLink` in the current file whose range contains the cursor position
2. If none, return null
3. Resolve `link.stem` via the index
4. If `found`: return `Location { uri: resolvedPath, range: start of file (0,0)-(0,0) }`
5. If `broken` or `ambiguous`: return null (the diagnostic already flags this)

---

### Find References (`textDocument/references`)

1. Find the `WikiLink` in the current file whose range contains the cursor position
2. If none, return null — the editor does nothing
3. Resolve `link.stem` via the index
4. If `found`: return `index.linksTo[resolvedPath]` as a list of `Location` (one per link, using `link.range`)
5. If `broken` or `ambiguous`: return null

---

### Diagnostics (`textDocument/publishDiagnostics`)

Diagnostics are published for a file whenever that file is indexed (opened, changed, or updated via file watcher). They are never published for files that are not currently in the index.

For each `WikiLink` in the file:

| Resolution  | Severity | Message                                           |
| ----------- | -------- | ------------------------------------------------- |
| `broken`    | Warning  | `No note found for '[[stem]]'`                    |
| `ambiguous` | Warning  | `'[[stem]]' matches multiple notes: path1, path2` |
| `found`     | —        | no diagnostic                                     |

Diagnostic range: `link.innerRange` (the stem text only, not the brackets).

**Cascading updates:** when a file is added, removed, or renamed, links in _other_ files that reference it may change state (e.g. a previously broken link becomes valid). After any index update, the server republishes diagnostics for all files whose link resolutions changed.

---

## Incremental update flows

### File opened in editor (`textDocument/didOpen`)

1. Parse the document content from the notification (do not read from disk)
2. `index(note)` → updates `byPath`, `byStem`, `linksTo`
3. Publish diagnostics for the file
4. Republish diagnostics for any other files affected by resolution changes

### File changed in editor (`textDocument/didChange`)

Same as `didOpen` — re-parse the full document content and re-index.

### File closed in editor (`textDocument/didClose`)

No index update. The on-disk version (already indexed via `didOpen` or file watcher) remains current.

### External file change (`workspace/didChangeWatchedFiles`)

Each event in the notification:

| Event type | Action                                                        |
| ---------- | ------------------------------------------------------------- |
| `created`  | read file from disk, parse, `index(note)`                     |
| `changed`  | read file from disk, parse, `index(note)` (replaces existing) |
| `deleted`  | `remove(path)`                                                |

After processing all events, republish diagnostics for affected files.
