# v0.11.1 Implementation Plan

Describes the order in which changes are made, what is tested after each
step, and the checkpoints where the CLI output should be manually verified.

The guiding principle: each step produces something testable. No step lays
down untested code for the next step to build on.

---

## Status

| Step                                            | Status |
| ----------------------------------------------- | ------ |
| 1 — `resolve()` empty-target fix (#60)          | Done   |
| 2 — `walk_dir()` CurDir fix (#62)               | Done   |
| 3 — `find_fallback_links()` code-span fix (#63) | Done   |
| 4 — Integration tests                           | Done   |
| 5 — Docs: components, ROADMAP, CHANGELOG        | Done   |

---

## Step 1 — `resolve()` empty-target fix (#60), then delete the redundant callers

Fixes the narrower of the two `NoteIndex` bugs first, in isolation, before
touching `walk_dir()` in Step 2 — keeps each step's diff to one bug so a
regression is easy to bisect. This step's regression test is the actual bug
report: it must be written and confirmed failing against the unfixed code
before the fix lands.

Three sub-steps, in order: fix `resolve()`, pin the caller-side special
cases' current behavior with tests, then delete those special cases and
confirm the pinning tests still pass unchanged.

**TDD cycle:**

1. Write `resolve_empty_target_resolves_to_source` and
   `resolve_empty_target_with_anchor_resolves_to_source` first (see table
   below).
2. Run `cargo test` and confirm both **fail** (`resolve()` returns `Broken`
   for an empty target today).
3. Add the `if target.is_empty() { return ResolvedLink::Found(source.to_path_buf()); }`
   branch to `resolve()`, immediately after the `is_url_like` check and
   before `unescape_link_target`. Re-run `cargo test` and
   `cargo clippy -- -D warnings`.
4. Write the five pinning tests in the second table below, against the
   _current_ (still special-cased) `compute_diagnostics`, `handle_definition`,
   `handle_completion`, and `handle_references`. Run `cargo test` and confirm
   all five **pass** as-is — they're pinning today's behavior, not exercising
   new code yet.
5. Delete the `if link.target.is_empty() { ... }` branch from
   `compute_diagnostics` (`src/handlers.rs:63`, ~line 71-87), from
   `handle_definition` (`src/handlers.rs:790`, ~line 807-819), and from the
   anchor-completion trigger in `handle_completion` (`src/handlers.rs:513`,
   ~line 570-577). Leave the guards in code actions "Create note" (~line 1195) and inlay hints (~line 1707) untouched — those are deliberate
   feature exclusions, not duplicated anchor-matching logic.
6. Re-run all five pinning tests. They must pass **unchanged** — this is what
   proves the deletion is behavior-preserving. Then run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/index/mod.rs`: `resolve()` gains an empty-target branch returning
  `ResolvedLink::Found(source.to_path_buf())`
- `src/handlers.rs`: `compute_diagnostics`, `handle_definition`, and
  `handle_completion`'s anchor-trigger branch each lose their
  `if link.target.is_empty() { ... }` special case; every link now flows
  through the function's existing `match index.resolve(...)`
- `src/handlers.rs`: `handle_references` gains no new code, but its existing
  `resolve()` call now returns `Found(source)` instead of `Broken` for a
  same-file anchor link under the cursor, so it starts returning real
  backlinks for that case — a deliberate, in-scope behavior change (see
  design.md)

**Unit tests:**

| Test                                                  | What it verifies                                                                                |
| ----------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `resolve_empty_target_resolves_to_source`             | `resolve(source, "")` returns `Found(source)`                                                   |
| `resolve_empty_target_with_anchor_resolves_to_source` | A same-file anchor link's empty `target` (anchor carried separately) still resolves to `source` |

**Pinning tests (`src/handlers.rs`) — written before the deletions, still passing after:**

| Test                                                                 | What it verifies                                                                                                                                                                                                              |
| -------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `diagnostics_same_file_anchor_valid_no_warning`                      | A same-file anchor link whose anchor matches a heading produces no diagnostic, before and after the deletion                                                                                                                  |
| `diagnostics_same_file_anchor_missing_emits_heading_not_found`       | A same-file anchor link with no matching heading emits `"Heading not found: '#{anchor}'"`, before and after                                                                                                                   |
| `goto_definition_same_file_anchor_jumps_to_heading`                  | Go to Definition on a same-file anchor link returns the matching heading's range, before and after                                                                                                                            |
| `completion_same_file_anchor_trigger_lists_current_note_headings`    | `[text](#` returns completion items for the current note's own headings, before and after                                                                                                                                     |
| `references_same_file_anchor_link_returns_backlinks_to_current_file` | Find References on a same-file anchor link returns other notes' backlinks to the current file — **new** behavior, only true after the `resolve()` fix (this one is expected to go from failing to passing, not stay constant) |

> **Manual checkpoint:** In a note with at least one heading and a link like
> `[§1](#section-one)` pointing at it, place the cursor inside the link and
> trigger Go to Definition. Confirm it jumps to the heading. Then edit the
> link to point at a nonexistent anchor (`#nope`) and confirm a "Heading not
> found" diagnostic appears. Then open a second note that links to the first,
> place the cursor on the original `[§1](#section-one)` link, and trigger
> Find References — confirm the second note now appears in the results.

---

## Step 2 — `walk_dir()` CurDir fix (#62)

Fixes the path-normalization mismatch. This is the highest-impact bug (58 of
59 false positives on this repo trace back to it) and needs its own
regression test at the exact point of the fix, written and confirmed failing
first.

**TDD cycle:**

1. Write `walk_files_strips_leading_curdir_from_root` and
   `build_with_leading_curdir_root_resolves_relative_links` first, both
   pointed at `tests/fixtures/lint_clean` with a `./`-prefixed root.
2. Run `cargo test` and confirm both **fail** — `walk_files` returns
   `./tests/fixtures/lint_clean/note.md` (leading `CurDir`), and `build()`'s
   index reports the note's link to `target.md` as `Broken`.
3. Change the `ft.is_file()` arm in `walk_dir()` to
   `out.push(normalize_path(&entry.path()))`. Re-run `cargo test` and
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/index/mod.rs`: `walk_dir()`'s file arm normalizes each path before
  pushing to `out`

**Unit tests:**

| Test                                                     | What it verifies                                                                                                     |
| -------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `walk_files_strips_leading_curdir_from_root`             | `walk_files(Path::new("./tests/fixtures/lint_clean"))` returns no path with a leading `CurDir` component             |
| `build_with_leading_curdir_root_resolves_relative_links` | `build(&[PathBuf::from("./tests/fixtures/lint_clean")], &["md"])` resolves the note's link to `target.md` as `Found` |

> **Manual checkpoint:** From the repo root, run `knap lint .`. Confirm the
> warnings for `README.md`'s links to `docs/ARCHITECTURE.md`,
> `docs/ROADMAP.md`, `docs/GETTING_STARTED.md`, `docs/USER_STORIES.md` are
> gone. (`knap lint` still reports the remaining, expected findings —
> intentional placeholder links in `docs/design/**` and the deliberately
> broken links in `tests/fixtures/lint_basic`.)

---

## Step 3 — `find_fallback_links()` code-span fix (#63)

Independent of Steps 1–2 (different file, different function) — ordered
last among the three fixes since it's the lowest-impact (one false
positive on this repo) and has no interaction with the `NoteIndex` changes
above.

**TDD cycle:**

1. Write `md_link_fallback_skips_inline_code_span` and
   `md_link_fallback_recovers_link_outside_code_span_on_same_line` first.
2. Run `cargo test` and confirm the first **fails** (the code-span content
   is currently recovered as a broken link).
3. Add `code_span_ranges: Vec<Range<usize>>` alongside `link_byte_ranges` in
   `extract_body_elements()`, push `byte_range.clone()` for every
   `Event::Code(_)` event, and thread `&code_span_ranges` into
   `find_fallback_links()` as a new `code_spans` parameter, checked in
   `already_covered` the same way `existing` is checked. Re-run
   `cargo test` and `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/parser/mod.rs`: `extract_body_elements()` collects `code_span_ranges`
  from `Event::Code` events
- `src/parser/mod.rs`: `find_fallback_links()` gains a `code_spans: &[Range<usize>]`
  parameter, checked via `code_spans.iter().any(|r| r.contains(&span_start))`
  in the `already_covered` condition

**Unit tests:**

| Test                                                            | What it verifies                                                                                      |
| --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `md_link_fallback_skips_inline_code_span`                       | `` `[text](` path)` `` is not recovered as a link                                                     |
| `md_link_fallback_recovers_link_outside_code_span_on_same_line` | A real fallback-eligible link elsewhere on the same line as an unrelated code span is still recovered |

> **Manual checkpoint:** Run `knap lint docs/ARCHITECTURE.md`. Confirm the
> `Link target not found: '` path'` warning at line 312 is gone.

---

## Step 4 — Integration tests

End-to-end tests over the real CLI binary, exercising all three fixes
together the way a user actually invokes `knap`. Always last, per the
project's step-ordering convention — by this point every unit behind these
tests is already covered in isolation.

**Deliverables:**

- `tests/cli.rs`: `lint_relative_dot_root_does_not_false_positive_on_valid_links`
- `tests/cli.rs`: `index_text_output_resolves_same_file_anchor_link`
- `tests/fixtures/index_anchor/note.md`: new fixture — a note with a
  same-file anchor link (`[§1](#section-one)`) and a matching heading
  (`## Section One`)
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                                            | What it verifies                                                                                            |
| --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `lint_relative_dot_root_does_not_false_positive_on_valid_links` | `knap lint .` (via `Command::current_dir` on `tests/fixtures/lint_clean`) exits 0 with `problem_count: 0`   |
| `index_text_output_resolves_same_file_anchor_link`              | `knap index tests/fixtures/index_anchor` text output shows `→` (resolved) for the anchor link, not `broken` |

> **Manual checkpoint (full session):** From the repo root, run
> `knap lint .` and `knap index .`. Confirm `knap lint .`'s problem count
> drops from 59 to the small, expected set (placeholder links in
> `docs/design/**` plus `tests/fixtures/lint_basic`'s deliberate ones), and
> that `knap index .`'s same-file anchor links (e.g. in `docs/ROADMAP.md`)
> show `→` instead of `broken`.

---

## Step 5 — Docs: components, ROADMAP, CHANGELOG

No new code in this step — brings the documentation set back in sync with
the shipped behavior, per project convention.

**Deliverables:**

- `docs/design/components/note-index.md`:
  - `resolve()` section: document the new empty-target branch, and update
    the sentence "Empty targets... are resolved against the source file
    itself by the caller before invoking `resolve`" — that's no longer
    true only of callers, `resolve()` now does it directly too
  - `build()`/`walk_files` section: document that `walk_dir()` normalizes
    each collected path, and why (the CurDir mismatch this fixes)
- `docs/design/components/parser.md`:
  - "Fallback link scan" section: replace the "known limitation" paragraph
    (lines ~311–316) describing inline code spans as unhandled — it's now
    handled; document `code_spans` as a third exclusion alongside
    `existing` and `fence_lines`
- `docs/design/components/handlers.md`:
  - `compute_diagnostics`, `handle_definition`/Go to Definition, and the
    completion anchor-trigger sections: remove any description of a
    separate empty-target special case; document that same-file anchor
    links now flow through the same `resolve()`-based path as cross-file
    links
  - Find References section: note that a same-file anchor link under the
    cursor now returns backlinks to the current file, where it previously
    returned nothing
- `docs/USER_STORIES.md`: no change — bug fixes against existing behavior,
  not new stories
- `docs/ROADMAP.md`: add a `v0.11.1` row to the version table (title "Lint
  & Index False Positives", status "Released" once shipped) and a matching
  `## v0.11.1` section with the stories table, mirroring the v0.10.2 entry
  format
- `CHANGELOG.md`: add a `## [0.11.1]` entry under `### Fixed` referencing
  #60, #62, #63 — done at release time per the `/knap-release` skill
- `Cargo.toml`: version bump to `0.11.1` — also done at release time

> **Manual checkpoint:** No editor checkpoint — documentation only. Diff
> review: confirm `note-index.md` and `parser.md` describe the exact
> behavior implemented in Steps 1–3, not an idealized version of it.

---

## Done — v0.11.1 complete

| Story | Feature                                                                                                                                                                                     | Delivered in step |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------- |
| #60   | `resolve()` treats an empty target as a same-file reference                                                                                                                                 | Step 1            |
| #60   | Redundant empty-target special cases deleted from `compute_diagnostics`, `handle_definition`, and completion's anchor trigger — one implementation of anchor-matching per function, not two | Step 1            |
| —     | Find References on a same-file anchor link now returns real backlinks instead of nothing (side effect of the #60 fix, called out as in-scope)                                               | Step 1            |
| #62   | `walk_dir()` normalizes collected paths, fixing relative-root false positives                                                                                                               | Step 2            |
| #63   | `find_fallback_links()` excludes inline code spans                                                                                                                                          | Step 3            |
