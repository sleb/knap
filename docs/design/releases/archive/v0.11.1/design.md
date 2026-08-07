# v0.11.1 Design — Lint & Index False Positives

Covers three bugs fixed in v0.11.1, all in link resolution:

| Story | Type | Description                                                                                                 |
| ----- | ---- | ----------------------------------------------------------------------------------------------------------- |
| #60   | Bug  | `knap index` always reports same-file anchor links (`[text](#heading)`) broken                              |
| #62   | Bug  | `knap lint`/`knap index` falsely report valid links broken when run with a relative root (`.`, the default) |
| #63   | Bug  | Parser fallback link scan misparses example link syntax inside inline code spans as a broken link           |

---

## Goal

A writer running `knap lint` — with no arguments, the documented default — on
a real vault gets diagnostics that match reality: no warnings for links that
actually resolve, no warnings for prose that merely shows link syntax as an
example. Today all three fail in exactly this everyday case, discovered by
running `knap lint` on this repo itself: 59 reported problems, of which 58
are false positives and only the deliberately-broken test fixtures are real.
These three bugs ship together because they were found together during that
same audit, all three sit in link resolution (`NoteIndex::resolve()` and its
callers, plus the parser's fallback link scan that feeds it), and #60 shares
a function with #62 — fixing `resolve()` once for both avoids two separate
patch releases touching the same nine lines.

---

## Note Index Changes

### `resolve()` — same-file anchor links (#60)

`resolve()` (`src/index/mod.rs`) currently has no special case for an empty
target. A same-file anchor link (`[text](#heading)`) parses with
`link.target == ""`; joining `""` onto `source.parent()` normalizes to the
_parent directory_, which is never a member of `all_files` (a set of file
paths, not directories), so the lookup always misses and returns `Broken`.

The LSP-facing callers (`compute_diagnostics`, `handle_goto_definition`)
already special-case `link.target.is_empty()` themselves before ever calling
`resolve()`, so they're unaffected — this is why `knap lint` doesn't show the
bug. `cmd_index` (both the text and `--json` output, via `report()`) calls
`resolve()` directly with no such guard, so this is purely a `knap index`
bug.

Moving the special case into `resolve()` itself fixes the one broken caller
without duplicating logic, and is a no-op for callers that already guard for
it:

```rust
pub fn resolve(&self, source: &Path, target: &str) -> ResolvedLink {
    if is_url_like(target) {
        return ResolvedLink::Found(PathBuf::from(target));
    }
    if target.is_empty() {
        return ResolvedLink::Found(source.to_path_buf());
    }
    let target = unescape_link_target(target);
    let candidate = source
        .parent()
        .expect("note path must have a parent directory")
        .join(target.as_ref());
    let candidate = normalize_path(&candidate);
    if self.all_files.contains(&candidate) {
        ResolvedLink::Found(candidate)
    } else {
        ResolvedLink::Broken
    }
}
```

### Deleting the now-redundant caller-side special cases

`compute_diagnostics` (`src/handlers.rs:63`), `handle_definition`
(`src/handlers.rs:790`), and `handle_completion`'s anchor-trigger branch
(`src/handlers.rs:513`, ~line 570) each have an `if link.target.is_empty()`
branch that runs _before_ calling `resolve()` at all. Each branch does the
same lookup the function's own `ResolvedLink::Found` arm already does a few
lines below it — match the anchor against a target note's headings via
`slug()` — just written against the local `note` variable instead of going
through `index.get_note(&target_path)`.

Once `resolve()` returns `Found(source)` for an empty target, these aren't
merely similar to their `Found`-arm counterpart — they're the same
computation. `index.get_note(&target_path)` where `target_path == source`
returns the identical `Note` already held in the local `note` variable, so
the general path produces byte-identical output to the special case it
replaces. All three special-case branches are deleted; every link (empty
target or not) now flows through the one `match index.resolve(...)` each
function already had. This removes the comment-based "don't delete this,
it's not really redundant" contract entirely — there is now exactly one
implementation of anchor-matching per function, not two, so nothing can
silently diverge.

`resolve()`'s public signature and `ResolvedLink` enum are unchanged by this
— unification happens by deleting the caller-side duplicates, not by
teaching `resolve()` about anchors.

**Two call sites keep their own `if link.target.is_empty()` guards, correctly
— not affected by this deletion:**

- Code actions "Create note" (`src/handlers.rs:~1195`) — skips empty targets
  because there's no file to create for a same-file anchor link. Not
  duplicated anchor-matching logic; a deliberate feature exclusion.
- Inlay hints (`src/handlers.rs:~1707`) — skips empty targets because
  showing a note's own title next to a self-referential anchor link isn't a
  feature anyone asked for. Also a deliberate exclusion, not duplication.

**One real behavior change, not just a redundancy removal:** `handle_references`
(`src/handlers.rs:842`) has no empty-target guard today. "Find References" on
a same-file anchor link currently returns nothing, because `resolve()`
returns `Broken` for it. After this fix, `resolve()` returns `Found(source)`,
and `handle_references` will start returning real backlinks
(`index.links_to(&path)`) for such links. This is in scope for v0.11.1 —
arguably a bug fix in its own right (a same-file anchor link is still a link;
other notes that reference _this_ note should show up in Find References
regardless of what triggered the lookup) — but it's new behavior beyond what
#60/#62/#63 originally called for, so it's called out explicitly here rather
than landing as a silent side effect.

### `walk_dir()` — leading `./` mismatch (#62)

`walk_dir()` (`src/index/mod.rs`) builds file paths via
`std::fs::read_dir(dir)`. `DirEntry::path()` joins the literal `dir` string
onto each entry's filename, so a relative root with a leading `.` (`.`, the
default `PATH` for `knap lint`/`knap index`; also `./docs`, etc.) produces
paths like `./docs/ARCHITECTURE.md` — a literal leading `CurDir` component.

`resolve()`'s `normalize_path()` collapses `.`/`..` components in the
_candidate_ path it builds from `source.parent().join(target)`, including a
leading `CurDir`, producing `docs/ARCHITECTURE.md` with no `./`. `all_files`
(populated straight from `walk_dir`'s un-normalized output) still holds the
`./`-prefixed form. `Path`/`PathBuf` equality is component-wise and a leading
`.` is significant, so the two representations never compare equal and the
`all_files.contains(&candidate)` lookup fails for every valid link — even
though the file exists — whenever the root has a leading `.`.

Fix: normalize each file path at the point it's collected, not the root used
for `read_dir` (normalizing an all-`.` root like `.` down to an empty path
would break `read_dir` itself):

```rust
fn walk_dir(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            let name = entry.file_name();
            if !should_skip_dir(name.to_string_lossy().as_ref()) {
                walk_dir(&entry.path(), out);
            }
        } else if ft.is_file() {
            out.push(normalize_path(&entry.path()));
        }
        // symlinks: ft.is_symlink() → skip to prevent infinite loops
    }
}
```

Directory recursion keeps using the un-normalized `entry.path()` (still a
valid argument to `read_dir`); only the leaf file paths that become
`all_files`/`by_path` keys get normalized, matching what `resolve()` already
produces for its candidates.

---

## Parser Changes

### `find_fallback_links()` — skip inline code spans (#63)

`find_fallback_links()` (`src/parser/mod.rs`) already skips spans inside
fenced code blocks (`fence_lines`) but has no equivalent check for inline
`` `code spans` ``. Prose that shows link syntax as an example inside
backticks — e.g. `` `[text](` path`` — gets misparsed as a literal link with
a garbage target.

The main pulldown-cmark loop in `extract_body_elements()` already receives
`Event::Code(s)` for every inline code span, with a `byte_range` (the same
loop that already collects `link_byte_ranges` and `code_fences`), but
currently discards it. Collect it the same way:

```rust
let mut code_span_ranges: Vec<Range<usize>> = Vec::new();
// ...
Event::Text(s) | Event::Code(s) => {
    if let Event::Code(_) = event {
        code_span_ranges.push(byte_range.clone());
    }
    // existing heading/link text accumulation unchanged
}
```

Thread it into `find_fallback_links()` as a new parameter and check it the
same way `existing` is checked — by byte-range containment of the span
start, not by line range like `fence_lines`, since a code span can start and
end mid-line and a fallback link elsewhere on the same line must still be
reachable:

```rust
fn find_fallback_links(
    content: &str,
    offset: usize,
    line_index: &LineIndex,
    existing: &[Range<usize>],
    fence_lines: &[(u32, u32)],
    code_spans: &[Range<usize>],
) -> Vec<MarkdownLink> {
    // ...
    let already_covered = existing.iter().any(|r| r.contains(&span_start))
        || fence_lines.iter().any(|&(start, end)| {
            let line = line_index.position(span_start + offset).line;
            line >= start && line <= end
        })
        || code_spans.iter().any(|r| r.contains(&span_start));
    // ...
}
```

Edge cases:

- A code span entirely before or after a fallback-link-shaped span on the
  same line → unaffected, still recovered.
- A fallback-link-shaped span whose `[`/`![` starts inside a code span but
  whose `)` closes outside it (backtick doesn't balance around the whole
  span) → excluded, since only `span_start` (the `[`/`![` position) is
  checked — matches how `existing` and `fence_lines` are already checked
  (start-position only, not full-range overlap).

---

## Testing

### Unit tests (`src/index/tests.rs`)

| Test                                                     | What it verifies                                                                                                                   |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `resolve_empty_target_resolves_to_source`                | `resolve(source, "")` returns `Found(source)`                                                                                      |
| `resolve_empty_target_with_anchor_resolves_to_source`    | Same-file anchor link (`target: ""`, real-world shape) still resolves to `source`                                                  |
| `walk_files_strips_leading_curdir_from_root`             | `walk_files(Path::new("./tests/fixtures/lint_clean"))` returns no path with a leading `CurDir` component                           |
| `build_with_leading_curdir_root_resolves_relative_links` | `build(&[PathBuf::from("./tests/fixtures/lint_clean")], &["md"])` resolves the note's link to `target.md` as `Found`, not `Broken` |

### Unit tests (`src/handlers.rs`) — pinning tests, written before deleting the special cases

These assert the exact behavior of the branches being deleted, so the
deletion is provably behavior-preserving rather than "looks equivalent."
Written and passing against the code _before_ the caller-side branches are
removed; still passing, unchanged, after removal.

| Test                                                                 | What it verifies                                                                                                                               |
| -------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `diagnostics_same_file_anchor_valid_no_warning`                      | A same-file anchor link whose anchor matches a heading in the current note produces no diagnostic                                              |
| `diagnostics_same_file_anchor_missing_emits_heading_not_found`       | A same-file anchor link whose anchor matches no heading emits `"Heading not found: '#{anchor}'"`                                               |
| `goto_definition_same_file_anchor_jumps_to_heading`                  | Go to Definition on a same-file anchor link returns the matching heading's range in the current file                                           |
| `completion_same_file_anchor_trigger_lists_current_note_headings`    | `[text](#` in a note with headings returns completion items for that note's own headings                                                       |
| `references_same_file_anchor_link_returns_backlinks_to_current_file` | Find References on a same-file anchor link now returns other notes' backlinks to the current file (new behavior — previously returned nothing) |

### Unit tests (`src/parser/tests.rs`)

| Test                                                            | What it verifies                                                                                      |
| --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `md_link_fallback_skips_inline_code_span`                       | `` `[text](` path)` `` (link-shaped text inside a single backtick span) is not recovered as a link    |
| `md_link_fallback_recovers_link_outside_code_span_on_same_line` | A real fallback-eligible link elsewhere on the same line as an unrelated code span is still recovered |

### Integration tests (`tests/cli.rs`)

| Test                                                            | What it verifies                                                                                            |
| --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `lint_relative_dot_root_does_not_false_positive_on_valid_links` | `knap lint .` (via `current_dir` on the `lint_clean` fixture) exits 0 with `problem_count: 0`               |
| `index_text_output_resolves_same_file_anchor_link`              | `knap index` text output shows `→` (resolved), not `broken`, for a same-file anchor link in a small fixture |

No new fixture directories are needed for the CurDir regression test —
`tests/fixtures/lint_clean` (already used by `lint_clean_dir_exits_zero`)
has exactly the shape needed: one valid relative link. The anchor test needs
a new fixture (`tests/fixtures/index_anchor/note.md`) since no existing
fixture has a same-file anchor link.
