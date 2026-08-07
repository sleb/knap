# v0.10.2 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a
real editor.

The guiding principle: each step produces something testable. No step lays
down untested code for the next step to build on.

---

## Status

| Step                                                   | Status | Notes                                                                          |
| ------------------------------------------------------ | ------ | ------------------------------------------------------------------------------ |
| 1 — `escape_link_target()` helper                      | Done   |                                                                                |
| 2 — Escape terminal completion insertions              | Done   |                                                                                |
| 3 — Escape "Create note" quick-action link text        | Done   | Required two unplanned fixes below — see Step 3.5                              |
| 3.5 — Parser fallback + unescape/extension gaps        | Done   | Discovered while implementing Step 3; not in the original design               |
| 4 — Docs: components, USER_STORIES, ROADMAP, CHANGELOG | Done   | CHANGELOG entry + Cargo.toml version bump deferred to `/knap-release` per plan |

---

## Step 1 — `escape_link_target()` helper

Lays down the escaping logic as a standalone, fully-tested function before
either call site uses it. Nothing calls it yet, so this step can't regress
existing completion/code-action behavior.

**TDD cycle:**

1. Write all unit tests for this step first (see table below) against a stub
   `fn escape_link_target(target: &str) -> String { target.to_string() }`.
2. Run `cargo test` and confirm the new tests **fail** (the stub never wraps
   anything).
3. Implement the real body (wrap in `<...>` and backslash-escape inner
   `<`/`>`/`\` when the target contains whitespace, control characters, or
   parentheses) until tests pass, then run `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/handlers.rs`: `fn escape_link_target(target: &str) -> String`, placed
  next to `slug()`

**Unit tests:**

| Test                                                            | What it verifies                                                                 |
| --------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `escape_link_target_no_special_chars_unchanged`                 | A plain relative path is returned unchanged                                      |
| `escape_link_target_space_wraps_in_angle_brackets`              | `"My File.md"` → `"<My File.md>"`                                                |
| `escape_link_target_parens_wrap_in_angle_brackets`              | `"file (1).md"` → `"<file (1).md>"`                                              |
| `escape_link_target_escapes_inner_angle_brackets_when_wrapping` | `"My <File>.md"` → inner `<`/`>` are backslash-escaped inside the wrapped result |

> **Manual checkpoint:** No editor checkpoint — this step is a pure function
> with no call site yet, covered entirely by the unit tests above.

---

## Step 2 — Escape terminal completion insertions

Wires the helper into `handle_completion` at the two call sites that insert a
_complete_ link destination (tier-1 immediate-child files and tier-2 global
files). The tier-0 folder items are deliberately left unwrapped — wrapping a
partial destination like `"sub/"` would break `check_dir_trigger`'s raw-text
re-read on the next keystroke, since the trigger detector doesn't know how to
resume inside an already-opened `<`.

**TDD cycle:**

1. Write all unit tests for this step first, seeding notes/attachments whose
   filenames or directory names contain spaces.
2. Run `cargo test` and confirm the new tests **fail** against the current
   (unescaped) `new_text` values.
3. Change the three `new_text: full_rel` sites for tier-1 and tier-2 items
   (not tier-0) to `new_text: escape_link_target(&full_rel)`, then re-run
   `cargo test` and `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/handlers.rs`: `handle_completion` — tier-1 file item `text_edit`
  (~line 650) and tier-2 global item `text_edit` (~line 686) call
  `escape_link_target(&full_rel)` for `new_text`
- `src/handlers.rs`: tier-0 folder item `text_edit` (~line 620) left
  unchanged (raw `full_rel`)

**Unit tests:**

| Test                                             | What it verifies                                                               |
| ------------------------------------------------ | ------------------------------------------------------------------------------ |
| `completion_file_item_with_space_wraps_target`   | Tier-1 file completion for a filename with a space wraps `new_text` in `<...>` |
| `completion_global_item_with_space_wraps_target` | Tier-2 global completion for a filename with a space wraps `new_text`          |
| `completion_folder_item_with_space_not_wrapped`  | Tier-0 folder completion for a directory with a space is **not** wrapped       |
| `completion_file_item_without_space_unchanged`   | Regression: a filename with no special characters still inserts the raw path   |

> **Manual checkpoint:** In a vault with a file named `My File.md`, open a
> note in the same directory, type `[link](`, and select `My File.md` from
> the completion list. Confirm the inserted text reads
> `[link](<My File.md>)` and that Go to Definition on the link (place cursor
> inside it, trigger "Go to Definition") jumps to `My File.md`.

---

## Step 3 — Escape "Create note" quick-action link text

Fixes the second half of the issue: today "Create note" creates the file on
disk but never touches the still-broken link text. This step must include a
regression test as its first deliverable — written and confirmed failing
against the unfixed code before the fix lands — since this is the actual bug
report and needs direct coverage.

**TDD cycle:**

1. Write `code_actions_create_note_target_with_space_adds_text_edit` (and the
   no-op regression test) first, asserting the `WorkspaceEdit` contains a
   `TextDocumentEdit` that rewrites `link.target_range` to
   `<My File>` in addition to the existing `CreateFile` op.
2. Run `cargo test` and confirm `code_actions_create_note_target_with_space_adds_text_edit`
   **fails** (no such edit exists yet).
3. Implement: import `OneOf` and `TextDocumentEdit`/
   `OptionalVersionedTextDocumentIdentifier` from `lsp_types`; in the
   `ResolvedLink::Broken` arm of `handle_code_actions`, build `ops` as a
   `Vec<DocumentChangeOperation>` starting with the existing `CreateFile` op,
   then push a `TextDocumentEdit` op (using `link.target_range`) only when
   `escape_link_target(&link.target) != link.target`. Re-run `cargo test` and
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/handlers.rs`: `handle_code_actions`, `ResolvedLink::Broken` arm —
  builds a multi-operation `document_changes` list (`CreateFile` +
  conditional `TextDocumentEdit`)
- `src/handlers.rs`: new `lsp_types` imports (`OneOf`, `TextDocumentEdit`,
  `OptionalVersionedTextDocumentIdentifier`)
- Test helper: `extract_text_edit(action) -> Option<&TextEdit>` alongside the
  existing `extract_create_file` helper in the test module

**Unit tests:**

| Test                                                         | What it verifies                                                                                       |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------ |
| `code_actions_create_note_target_with_space_adds_text_edit`  | Broken link `[link](My File)` produces both the `CreateFile` op and a `TextDocumentEdit` → `<My File>` |
| `code_actions_create_note_target_without_space_no_text_edit` | Regression: a broken link with no special characters produces only the `CreateFile` op, no extra edit  |

> **Manual checkpoint:** In an empty note, type `[notes](My New Note)` where
> `My New Note.md` doesn't exist yet. Trigger the code action menu, select
> "Create note". Confirm `My New Note.md` is created on disk **and** the
> document now reads `[notes](<My New Note>)`. Trigger Go to Definition on
> the link and confirm it navigates to the new file.

**What actually happened:** `code_actions_create_note_target_with_space_adds_text_edit`
initially failed with `actions.len() == 0`, not a missing `TextDocumentEdit`.
Investigation (documented fully in `design.md`'s "Correction" note) found
`[link](My File)` was invisible to `handle_code_actions` altogether —
pulldown-cmark doesn't emit a link event for a bare space-containing
destination, so `note.md_links` was empty for that input. Confirmed against
issue #57 via `gh issue view 57`: the issue's own repro assumes the quick
action fires for exactly this input, so the gap was real, not a bad test.
Fixing it required the unplanned Step 3.5 below; only after that did this
step's implementation (import `OneOf`/`TextDocumentEdit`/
`OptionalVersionedTextDocumentIdentifier`, build a multi-op `document_changes`
list in the `ResolvedLink::Broken` arm) become testable as originally
described.

---

## Step 3.5 — Parser fallback for unparseable links, plus two reuse bugs (unplanned)

Not in the original design. Surfaced by Step 3's regression test failing for
a reason unrelated to the `TextDocumentEdit` logic itself. Three fixes,
found and fixed in this order:

1. **Parser never sees bare space/paren-containing destinations.**
   `[link](My File)` and `[link](file (1).md)` are not recognized as link
   events by pulldown-cmark at all (verified directly against
   `pulldown_cmark::Parser` — confirmed via a throwaway `examples/dbg.rs`,
   not committed). Added `find_fallback_links()` to `src/parser/mod.rs`: a
   post-pass over the raw body text that recovers `[text](target)` /
   `![alt](target)` spans pulldown-cmark skipped, gated on the same
   whitespace/control/paren predicate `escape_link_target()` uses (now
   shared as `link_destination_needs_wrapping()` in `src/parser/mod.rs`, so
   there's one definition of "needs wrapping" for both the read and write
   sides). Skips spans already found by the main pulldown-cmark pass and
   spans inside fenced code blocks. The link/anchor-splitting logic shared
   with the main pulldown-cmark handler was factored out into
   `split_link_destination()` to avoid duplicating it.
2. **`link.target` can already be wrapped.** For a link that _is_ valid
   CommonMark because it's already wrapped (`[link](<My File>)`, broken only
   because the file doesn't exist), knap's own destination extraction
   re-slices raw text between `(` and `)` rather than using pulldown-cmark's
   unwrapped `dest_url`, so `link.target` is the literal `"<My File>"`,
   brackets included. Reusing that raw value in `new_note_path` or
   `escape_link_target` double-wraps. Fixed by promoting
   `index::unescape_link_target()` from a private helper (already used
   inside `index::resolve()`) to `pub(crate)`, and calling it on
   `link.target` before either use in `handle_code_actions`.
3. **`new_note_path` never appended `.md`.** Pre-existing gap, invisible
   until fix (1) made extension-less broken links (`My File`, no
   `.md`) reachable at all. `new_note_path` now calls `.with_extension("md")`
   when the computed path has no extension.

**Deliverables:**

- `src/parser/mod.rs`: `link_destination_needs_wrapping()`, `find_fallback_links()`,
  `split_link_destination()` (refactored out of the pulldown-cmark `Link`/`Image`
  end-event handler)
- `src/handlers.rs`: `escape_link_target()` now calls
  `crate::parser::link_destination_needs_wrapping()` instead of duplicating
  the predicate; `new_note_path()` appends `.md` when the target has no
  extension; `ResolvedLink::Broken` arm unescapes `link.target` via
  `index::unescape_link_target()` before deriving the new file path or the
  re-escaped link text
- `src/index/mod.rs`: `unescape_link_target()` changed from private to
  `pub(crate)`

**Unit tests:**

| Test (`src/parser/tests.rs`)                      | What it verifies                                                      |
| ------------------------------------------------- | --------------------------------------------------------------------- |
| `md_link_fallback_bare_space_target`              | `[link](My File)` recovered with `target: "My File"`                  |
| `md_link_fallback_bare_paren_target`              | `[link](file (1).md)` recovered — balanced-but-unescaped parens       |
| `md_link_fallback_image_with_space`               | `![alt](My Image.png)` recovered as an image link                     |
| `md_link_fallback_with_anchor`                    | `[link](My File#section)` splits into target/anchor correctly         |
| `md_link_fallback_does_not_duplicate_valid_links` | A normal valid link isn't double-counted                              |
| `md_link_fallback_skips_fenced_code`              | Fenced code block contents are not recovered as links                 |
| `md_link_fallback_no_bracket_no_link`             | Stray `[...]`/`(...)` prose without a `](` boundary isn't matched     |
| `md_link_fallback_multiple_on_one_line`           | Two fallback links on one line are both recovered with correct ranges |

| Test (`src/handlers.rs`)                                             | What it verifies                                                              |
| -------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `code_actions_create_note_already_wrapped_target_not_double_escaped` | `[link](<My File>)` creates `My%20File.md` and does not re-wrap the link text |

> **Manual checkpoint:** Same as Step 3's — this step is what makes that
> checkpoint's literal repro (`[notes](My New Note)`, typed without angle
> brackets) actually reachable.

---

## Step 4 — Docs: components, USER_STORIES, ROADMAP, CHANGELOG

No new code in this step — brings the documentation set back in sync with the
shipped behavior, per project convention (every release updates docs before
being considered done).

**Deliverables:**

- `docs/design/components/handlers.md`:
  - "Completion" → "Directory completion" subsection: note that tier-1/tier-2
    `new_text` is escaped via `escape_link_target()` when the path contains
    whitespace, control characters, or parentheses; tier-0 folder items stay
    raw
  - "Code Actions" section: update the **Create note** row of the action
    table to note it now also emits a `TextDocumentEdit` fixing the link text
    when escaping changes it, unescapes an already-wrapped `link.target`
    before reuse, and that `new_note_path` now appends `.md` when missing
  - New "Shared helpers" entry for `escape_link_target()`, alongside
    `find_md_link_at_position()`
- `docs/design/components/parser.md` (or equivalent): document
  `find_fallback_links()` as the read-side counterpart to
  `escape_link_target()` — bare, unwrapped destinations containing
  whitespace/control chars/parens are recovered as links via a raw-text scan
  after the pulldown-cmark pass, since pulldown-cmark itself won't parse them
- `docs/USER_STORIES.md`: no change — this is a bug fix against existing
  stories (US-01/US-44/US-46 completion, US-19-adjacent quick action), not a
  new story; the fix restores behavior those stories already promise
- `docs/ROADMAP.md`: add a row for v0.10.2 to the version table (title
  "Escaped Link Targets for Paths with Spaces", status "Released" once
  shipped) and a matching `## v0.10.2` section with the stories table,
  mirroring the v0.3.5 entry format
- `CHANGELOG.md`: add a `## [0.10.2]` entry under `### Fixed` describing the
  bug and referencing #57 — done at release time per the `/knap-release`
  skill, not authored ahead of the fix landing
- `Cargo.toml`: version bump to `0.10.2` — also done at release time per
  `/knap-release`, not part of this step

> **Manual checkpoint:** No editor checkpoint — documentation only. Diff
> review: confirm `handlers.md` describes the exact escaping condition
> implemented in Steps 1–3, not an idealized version of it.

---

## Done — v0.10.2 complete

| Story | Feature                                                                                                                                  | Delivered in step |
| ----- | ---------------------------------------------------------------------------------------------------------------------------------------- | ----------------- |
| #57   | Completion inserts escaped link targets for paths with spaces                                                                            | Step 2            |
| #57   | "Create note" quick action rewrites the link text, not just the file on disk                                                             | Step 3            |
| #57   | Bare, unwrapped space/paren-containing link destinations are now recognized as (broken) links at all, not silently dropped as plain text | Step 3.5          |
| —     | `new_note_path` appends `.md` when the target has no extension (pre-existing gap, unrelated to escaping)                                 | Step 3.5          |
