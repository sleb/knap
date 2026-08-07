# v0.3.1 Design — Smarter Path Completion + Code Review Fixes

## Stories

| Story | Type    | Description                                                           |
| ----- | ------- | --------------------------------------------------------------------- |
| BF-01 | Bug fix | Missing test for EOF-terminated frontmatter; silent arithmetic        |
| BF-02 | Bug fix | `recv_response` loop can hang forever with no timeout                 |
| BF-03 | Cleanup | Quote-stripping logic duplicated in parser                            |
| BF-04 | Cleanup | Skip-list duplicated across `index` and `server` modules              |
| BF-05 | Bug fix | Silent empty-parent fallback inconsistent with rest of codebase       |
| BF-06 | Bug fix | `path_to_uri` panic message omits the offending path                  |
| BF-07 | Bug fix | Extra content changes silently discarded in `on_did_change`           |
| BF-08 | Cleanup | `note()` test helper defined in three places                          |
| BF-09 | Doc     | README version badge and status stale (0.2 vs 0.3)                    |
| BF-10 | Doc     | `ARCHITECTURE.md` handler table missing all v0.3 entries              |
| BF-11 | Doc     | `ARCHITECTURE.md` frontmatter described as v0.4; shipped in v0.1/v0.3 |
| BF-12 | Doc     | Stale `</thinking>` tag at EOF of `ARCHITECTURE.md`                   |
| US-46 | Feature | Segment-by-segment directory completion with re-trigger on `/`        |

---

## Goal

Fix all correctness and quality issues surfaced by the v0.3 code review, then
deliver the v0.3.1 roadmap feature: path completions that let a writer drill
into vault folders one segment at a time, instead of presenting a flat list of
every file in the workspace.

---

## Part 1 — Bug Fixes

### BF-01: Missing test for EOF-terminated frontmatter (`src/parser/mod.rs:249`)

`frontmatter_block` matches two patterns (line 126–130):

- Mid-file: `"\n---\n"` — the body starts 5 bytes after the closing `---`
- EOF: `.strip_suffix("\n---")` — no trailing newline; the body is empty

`frontmatter_body_offset` (line 249) handles both via a bounds check, but
there is no test asserting that the EOF case produces an empty body rather
than silently including a garbage byte.

**Fix:** Add a unit test in `src/parser/mod.rs` (the existing `tests` module):

```rust
#[test]
fn frontmatter_body_offset_eof_terminated() {
    let content = "---\ntitle: T\n---";   // no trailing newline
    let note = parse(Path::new("x.md"), content);
    assert_eq!(note.headings.len(), 0);
    assert_eq!(note.md_links.len(), 0);
    let fm = note.frontmatter.expect("frontmatter present");
    assert_eq!(fm.title.as_deref(), Some("T"));
}
```

No implementation changes are needed — this is a test gap only.

---

### BF-02: `recv_response` unbounded loop (`src/cli.rs:330`)

The loop skips `Request` and `Notification` messages without bound. A broken
server that never sends a `Response` blocks the process forever.

**Fix:** Introduce a skip counter. After 32 skipped messages, return an error:

```rust
fn recv_response(conn: &lsp_server::Connection) -> anyhow::Result<lsp_server::Response> {
    let mut skipped = 0u32;
    loop {
        match conn.receiver.recv()? {
            lsp_server::Message::Response(r) => return Ok(r),
            lsp_server::Message::Request(_) | lsp_server::Message::Notification(_) => {
                skipped += 1;
                if skipped > 32 {
                    anyhow::bail!("recv_response: server sent >32 non-response messages");
                }
            }
        }
    }
}
```

The limit of 32 is generous enough for any known LSP exchange during `knap check`.

---

### BF-03: Quote-stripping duplication (`src/parser/mod.rs:150` and `204`)

`extract_frontmatter` (line 150–157) and `extract_frontmatter_fields`
(line 204–213) both contain identical quote-stripping logic.

**Fix:** Extract a private `strip_surrounding_quotes(s: &str) -> &str` helper
at module scope, and call it from both sites:

```rust
fn strip_surrounding_quotes(s: &str) -> &str {
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"'))
            || (s.starts_with('\'') && s.ends_with('\'')))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}
```

---

### BF-04: Skip-list duplicated (`src/index/mod.rs:360`, `src/server/mod.rs:357`)

`should_skip_dir` in `index/mod.rs` and `should_skip_path` in `server/mod.rs`
both hard-code the same skip list (`starts_with('.')`, `"node_modules"`,
`"target"`). Adding `.obsidian` to one won't update the other.

**Fix:** Make `should_skip_dir` pub(crate) in `src/index/mod.rs` and call it
from `src/server/mod.rs` instead of duplicating the logic. `should_skip_path`
can delegate:

```rust
// server/mod.rs
fn should_skip_path(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        let std::path::Component::Normal(name) = c else { return false };
        index::should_skip_dir(&name.to_string_lossy())
    })
}
```

---

### BF-05: Silent empty-parent fallback (`src/handlers.rs:196`)

```rust
let from_dir = path.parent().unwrap_or(Path::new(""));
```

An empty `from_dir` silently produces garbage relative paths. Every other
`parent()` call in the codebase uses `expect`. The inconsistency means this
path fails silently while others fail loudly.

**Fix:**

```rust
let from_dir = path.parent().expect("indexed path must have a parent");
```

---

### BF-06: `path_to_uri` panic omits the offending path (`src/handlers.rs:501`)

```rust
.expect("non-absolute path")
```

**Fix:**

```rust
.unwrap_or_else(|_| panic!("path_to_uri: path must be absolute, got: {}", path.display()))
```

---

### BF-07: Extra content changes silently discarded (`src/server/mod.rs:343`)

`on_did_change` takes only `content_changes[0]`. For `TextDocumentSyncKind::FULL`
this is always correct, but silent when wrong.

**Fix:** Warn when more than one change arrives:

```rust
let mut iter = params.content_changes.into_iter();
let content = match iter.next() {
    Some(c) => c.text,
    None => { warn!("didChange: no content changes"); return; }
};
if iter.next().is_some() {
    warn!("didChange: received >1 content changes; only the first is used (Full sync)");
}
```

---

### BF-08: `note()` test helper defined three times

The helper `fn note(path: &str, content: &str) -> parser::Note` appears
verbatim in:

- `src/handlers.rs:523` (inside `#[cfg(test)]`)
- `src/index/tests.rs:6`

**Fix:** Move the definition into a `src/test_helpers.rs` module with
`#[cfg(test)]` visibility, and replace both call sites with `use
crate::test_helpers::note`.

```rust
// src/test_helpers.rs
#[cfg(test)]
pub(crate) fn note(path: &str, content: &str) -> crate::parser::Note {
    crate::parser::parse(std::path::Path::new(path), content)
}
```

---

### BF-09 – BF-12: Documentation drift

| Location                   | Issue                                  | Fix                                                                      |
| -------------------------- | -------------------------------------- | ------------------------------------------------------------------------ |
| `README.md:3`              | Badge says `0.2.0`                     | Change to `0.3.0`                                                        |
| `README.md:48`             | Status says v0.2.0                     | Update to v0.3.0 + feature list                                          |
| `README.md:21–36`          | "What it does" omits all v0.3 features | Add DocumentSymbols, WorkspaceSymbols, heading Rename, Anchor completion |
| `docs/ARCHITECTURE.md:244` | Handler table stops at v0.2            | Add all four v0.3 handlers                                               |
| `docs/ARCHITECTURE.md:153` | Frontmatter listed as v0.4             | Change to v0.1/v0.3                                                      |
| `docs/ARCHITECTURE.md:292` | Stale `</thinking>` at EOF             | Delete the line                                                          |

---

## Part 2 — US-46: Segment-by-segment Directory Completion

### Current behavior

`textDocument/completion` triggered by `(` returns a flat list of **all**
indexed notes and attachments, each with its full relative path from the
current note. For a large vault with deep structure, this floods the picker
with hundreds of paths — `../../journal/2026/entries/jan.md` and its siblings
all appear at once.

### New behavior

Completion drills one directory level at a time, like a file browser:

1. `](` trigger → immediate children of the current note's directory:
   files show as themselves; subdirectories show as `dir/` (with trailing slash).
2. Selecting a directory (or typing its name and `/`) re-triggers completion
   with `/` as the trigger character.
3. `/` trigger inside a link destination → immediate children of the directory
   implied by the partial path already typed.

Example — editing `/vault/notes/intro.md`, vault structure:

```
vault/notes/  ← current dir
  intro.md    ← current file (excluded)
  a.md
  subdir/
    b.md
vault/journal/
  2026.md
```

| User types      | Trigger | Completions shown        |
| --------------- | ------- | ------------------------ |
| `](`            | `(`     | `a.md`, `subdir/`, `../` |
| `](subdir/`     | `/`     | `subdir/b.md`            |
| `](.` `../`     | `/`     | `../`, `../journal/`     |
| `](../`         | `/`     | `../journal/`            |
| `](../journal/` | `/`     | `../journal/2026.md`     |

"Stub new files by name" is naturally satisfied: the user can type any path
not in the index and the completion simply shows nothing (no completions from
unknown paths). The link will be broken but the picker does not interfere.

---

### Trigger mechanics

Add `/` to `completion_provider.trigger_characters`:

```rust
CompletionOptions {
    trigger_characters: Some(vec!["(".to_string(), "#".to_string(), "/".to_string()]),
    ..Default::default()
}
```

Every `/` typed anywhere in the document sends a completion request. The
handler returns `vec![]` immediately if no `](` context is found, so the
overhead is negligible.

---

### Replacing `check_link_trigger` with `check_dir_trigger`

The existing `check_link_trigger(content, pos) -> bool` only fires when the
cursor is immediately after `](`. It cannot serve the `/` re-trigger case
(where the cursor is inside `](subdir/`).

**Remove `check_link_trigger`.** Replace with:

```rust
/// Returns `Some(partial)` when the cursor is inside a Markdown link destination
/// — i.e. there is a `](` somewhere before the cursor on the same line and no
/// `#` between `](` and the cursor (that would be an anchor context).
/// `partial` is the text between `](` and the cursor (may be empty).
fn check_dir_trigger(content: &str, pos: Position) -> Option<String> {
    let line = content.lines().nth(pos.line as usize).unwrap_or("");
    let cursor = utf16_to_byte_offset(line, pos.character);
    let before = &line[..cursor];
    let open = before.rfind("](")?;
    let after_open = &before[open + 2..];
    if after_open.contains('#') {
        return None; // anchor context; handled by check_anchor_trigger
    }
    Some(after_open.to_string())
}
```

`check_anchor_trigger` is checked first in `handle_completion`; `check_dir_trigger`
is checked second.

---

### Computing the completion set

Given `partial` from `check_dir_trigger`:

1. **`base_dir`** — the directory we are currently browsing:

   ```rust
   let note_dir = note_path.parent().expect("indexed path must have a parent");
   // If partial ends with '/', resolve it directly.
   // Otherwise take its parent (we're mid-segment; re-trigger hasn't fired yet,
   // but this branch handles the initial `](` trigger where partial = "").
   let base_dir = if partial.ends_with('/') || partial.is_empty() {
       normalize_path(&note_dir.join(&*partial))
   } else {
       let p = Path::new(&partial);
       normalize_path(&note_dir.join(p.parent().unwrap_or(Path::new(""))))
   };
   ```

2. **Enumerate immediate children of `base_dir`** from all indexed paths:

   ```rust
   let mut dirs: BTreeSet<String> = BTreeSet::new();
   let mut files: Vec<PathBuf> = vec![];

   let all_paths = index.all_notes().map(|n| &n.path)
       .chain(index.all_attachment_paths());

   for file_path in all_paths {
       if file_path == note_path { continue; }
       let rel = relative_path(&base_dir, file_path);
       let first = rel.split('/').next().unwrap_or("");
       if first == rel {
           files.push(file_path.clone()); // directly in base_dir
       } else {
           dirs.insert(first.to_string()); // subdirectory or ".."
       }
   }
   ```

3. **TextEdit range** — replaces the entire `partial` already typed. Computing
   the character start position requires a new private helper:

   ```rust
   /// Convert a byte offset within `s` to a UTF-16 code unit offset.
   fn byte_to_utf16_offset(s: &str, byte_offset: usize) -> u32 {
       s[..byte_offset].chars().map(|c| c.len_utf16() as u32).sum()
   }
   ```

   The range:
   - `start`: `byte_to_utf16_offset(line, open + 2)` (right after `](`)
   - `end`: `pos.character` (cursor)

4. **Completion items:**

   For each **directory** `dir_name` in `dirs`:

   ```rust
   let abs_dir = normalize_path(&base_dir.join(&*dir_name));
   let full_rel = if dir_name == ".." {
       // relative_path(note_dir, abs_dir) already resolves to ".." or "../.."
       relative_path(note_dir, &abs_dir) + "/"
   } else {
       relative_path(note_dir, &abs_dir) + "/"
   };
   CompletionItem {
       label: format!("{}/", dir_name),
       kind: Some(CompletionItemKind::FOLDER),
       filter_text: Some(dir_name.clone()),
       text_edit: Some(CompletionTextEdit::Edit(TextEdit {
           range: replace_range,
           new_text: full_rel,
       })),
       ..Default::default()
   }
   ```

   For each **file** `file_path` in `files`:

   ```rust
   let full_rel = relative_path(note_dir, &file_path);
   let file_name = file_path.file_name()
       .map(|n| n.to_string_lossy().into_owned())
       .unwrap_or_else(|| full_rel.clone());
   // Use frontmatter title as label for notes, filename for attachments.
   let (label, detail) = if let Some(note) = index.get_note(&file_path) {
       let title = note.frontmatter.as_ref().and_then(|fm| fm.title.clone());
       (title.clone().unwrap_or_else(|| file_name.clone()), title.map(|_| file_name.clone()))
   } else {
       (file_name.clone(), None)
   };
   CompletionItem {
       label,
       kind: Some(CompletionItemKind::FILE),
       filter_text: Some(file_name),
       detail,
       text_edit: Some(CompletionTextEdit::Edit(TextEdit {
           range: replace_range,
           new_text: full_rel,
       })),
       ..Default::default()
   }
   ```

---

### `handle_completion` updated flow

```
1. check_anchor_trigger  → Some(path) → anchor completions (unchanged)
2. check_dir_trigger     → Some(partial) → directory completions (new)
3. else → vec![]
```

The old `check_link_trigger` branch is removed entirely; `check_dir_trigger`
with `partial = ""` replicates the initial-trigger case.

---

### What changes for existing completion tests

The existing unit tests for path completion (`completion_trigger_returns_notes`,
`completion_relative_path_subdirectory`, `completion_includes_attachments`, etc.)
assert that ALL notes appear after `](`. They will need to be updated: each test
should only list notes that are immediate children of the cursor note's directory.
Tests that cross directory boundaries should either be split or updated to cover
the directory-item behavior.

Specifically:

- `completion_relative_path_subdirectory` — currently asserts `subdir/note.md`
  appears from the top level. New: asserts `subdir/` (a directory item) appears,
  not the file inside it.
- `completion_trigger_returns_notes` — multi-note test; only notes in the same
  directory should appear; cross-directory notes should appear as `../` items.

---

### Index changes

None. `all_notes()`, `all_attachment_paths()`, and `get_note()` are sufficient.
The directory structure is computed on the fly from relative paths.

---

## Testing

### New unit tests (`src/handlers.rs` or `src/test_helpers.rs`)

| Test                                         | What it verifies                                                      |
| -------------------------------------------- | --------------------------------------------------------------------- |
| `frontmatter_body_offset_eof_terminated`     | EOF `---` with no newline → empty body, title parsed correctly        |
| `dir_completion_initial_shows_siblings`      | `](` in `/notes/a.md` → sibling files and `subdir/` dir item          |
| `dir_completion_initial_excludes_current`    | Current file not in completion list                                   |
| `dir_completion_subdir_shows_children`       | `](subdir/` → files inside `subdir/` only                             |
| `dir_completion_parent_shows_dotdot`         | Files above note dir → `../` dir item appears                         |
| `dir_completion_double_parent`               | `](../` from a subdir → shows parent's children                       |
| `dir_completion_empty_dir_shows_only_dotdot` | No files in current dir (only elsewhere) → only `../` appears         |
| `dir_completion_text_edit_replaces_partial`  | textEdit range covers partial text; new_text is full relative path    |
| `dir_completion_title_as_label`              | Note with frontmatter title → label is title, filter_text is filename |
| `dir_completion_attachment_filename_label`   | Attachment → label is filename                                        |
| `check_dir_trigger_empty_on_plain_slash`     | `/` typed outside link context → `None`                               |
| `check_dir_trigger_none_in_anchor_context`   | `](path#` → `None` (anchor takes over)                                |
| `check_dir_trigger_partial_path`             | `](subdir/` → `Some("subdir/")`                                       |

### Updated existing unit tests

- `completion_relative_path_subdirectory` — expect `subdir/` dir item, not `subdir/note.md`
- `completion_trigger_returns_notes` — only same-dir notes; cross-dir becomes `../`
- `completion_includes_attachments` — dir-level file items still appear

### Integration test (`tests/lsp.rs`)

| Test                               | What it verifies                                            |
| ---------------------------------- | ----------------------------------------------------------- |
| `test_dir_completion_initial`      | `](` in a note returns directory items and sibling files    |
| `test_dir_completion_retrigger`    | Simulating `](subdir/` returns children of `subdir/`        |
| `test_anchor_completion_unchanged` | `](file.md#` still works correctly after the trigger change |
