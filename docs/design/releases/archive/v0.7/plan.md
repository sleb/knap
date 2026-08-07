# v0.7 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                        | Status | Notes |
| ------------------------------------------- | ------ | ----- |
| 1 — Same-file anchor completions (US-51)    | Done   |       |
| 2 — Go to Definition same-file (US-48)      | Done   |       |
| 3 — Broken bare anchor diagnostics (US-50)  | Done   |       |
| 4 — Find References on heading (US-49)      | Done   |       |
| 5 — Integration tests                       | Done   |       |

---

## Step 1 — Same-file anchor completions (US-51)

Two changes to `src/handlers.rs`:

1. Remove the `if path.is_empty() { return None; }` guard from
   `check_anchor_trigger` so it returns `Some("")` at `[text](#`.
2. In the anchor completion branch of `handle_completion`, check
   `target_rel.is_empty()` and source headings from `note` itself rather than
   resolving a target path.

This step is first because it has the narrowest blast radius — it only affects
the completion response and does not touch diagnostics, definition, or
references. A failing test confirms the guard removal was needed.

**TDD cycle:**

1. Write all unit tests for this step first — stub `check_anchor_trigger` return
   value changes via the test inputs if needed to compile.
2. Run `cargo test` and confirm the new tests fail.
3. Implement until tests pass, then run `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/handlers.rs`: remove `if path.is_empty() { return None; }` from
  `check_anchor_trigger` (line ~274)
- `src/handlers.rs`: add empty-path branch inside the anchor completion block in
  `handle_completion` (~line 331)

**Unit tests:**

| Test | What it verifies |
| ---- | ---------------- |
| `completion_bare_anchor_returns_current_file_headings` | `[text](#` in a note with two headings → items for each heading, `insert_text` is GFM slug |
| `completion_bare_anchor_empty_headings` | `[text](#` in a note with no headings → empty vec |
| `completion_bare_anchor_does_not_include_other_notes` | `[text](#` → completions come from current file only, not other indexed notes |

> **Manual checkpoint:** Open a Markdown note with two or more headings. Inside
> the note body, type `[see](#`. The completion menu should appear with one item
> per heading in the file (label = heading text, insert = GFM slug). No items
> from other files should appear.

---

## Step 2 — Go to Definition for same-file anchors (US-48)

Add a same-file anchor branch to `handle_definition` in `src/handlers.rs`,
inserted after the tag check and before the `index.resolve` call. When
`link.target.is_empty()`, resolve the anchor against `note.headings` and return
a `Location` in the same file.

**TDD cycle:**

1. Write unit tests first.
2. Run `cargo test` — new tests fail.
3. Implement; `cargo test` passes; `cargo clippy -- -D warnings` clean.

**Deliverables:**

- `src/handlers.rs`: add same-file anchor branch in `handle_definition`
  (~line 537), after the tag check, before `index.resolve`

**Unit tests:**

| Test | What it verifies |
| ---- | ---------------- |
| `definition_same_file_anchor_navigates_to_heading` | `[text](#section)` in note with `## Section` → `Location` in same file, range equals heading range |
| `definition_same_file_anchor_missing_falls_back_to_top` | `[text](#missing)` in note with `## Section` → `Location` in same file at `Range::default()` |

> **Manual checkpoint:** Open a Markdown note that contains `## My Section` and
> a link `[jump](#my-section)`. Place the cursor on the link and trigger Go to
> Definition. The cursor should jump to the `## My Section` line in the same
> file.

---

## Step 3 — Broken bare anchor diagnostics (US-50)

Replace the unconditional `continue` on `link.target.is_empty()` in
`compute_diagnostics` with anchor validation against `note.headings`. The
diagnostic message and severity are identical to the existing cross-file anchor
diagnostic so editors style them consistently.

**TDD cycle:**

1. Write unit tests first.
2. Run `cargo test` — `diagnostics_anchor_only_skipped` now fails for the broken
   case; confirm new tests fail.
3. Update `diagnostics_anchor_only_skipped` test to assert the bare anchor with
   no headings **does** produce a diagnostic; implement; `cargo test` passes;
   `cargo clippy -- -D warnings` clean.

Note: the existing `diagnostics_anchor_only_skipped` test asserts that anchor-
only links produce no diagnostic — that assertion was correct before this step.
It must be updated (split into a valid case and a broken case) before
implementing the change.

**Deliverables:**

- `src/handlers.rs`: replace `if link.target.is_empty() { continue; }` in
  `compute_diagnostics` (~line 44) with the anchor validation block from the
  design doc
- Update the existing `diagnostics_anchor_only_skipped` test to cover only the
  `[text](#)` (empty slug) case; add the two new tests below

**Unit tests:**

| Test | What it verifies |
| ---- | ---------------- |
| `diagnostics_bare_anchor_valid` | `[text](#existing)` in note with `## Existing` → no diagnostic |
| `diagnostics_bare_anchor_broken` | `[text](#missing)` in note with `## Existing` → one warning, message contains `#missing` |
| `diagnostics_bare_anchor_no_headings` | `[text](#anything)` in note with no headings → one warning |
| `diagnostics_bare_anchor_empty_slug_no_diagnostic` | `[text](#)` → no diagnostic (empty anchor, `link.anchor = None`) |

> **Manual checkpoint:** Open a Markdown note that has `## Real Heading` and two
> links: `[ok](#real-heading)` and `[broken](#fake-heading)`. Save the file. The
> `ok` link should have no diagnostic squiggle; `fake-heading` should show a
> warning squiggle. Hovering the squiggle should show `Heading not found:
> '#fake-heading'`.

---

## Step 4 — Find References on heading includes bare anchors (US-49)

Add `find_heading_at_position` helper and a new priority step in
`handle_references`. The new step sits between the existing link-at-cursor check
and the fallback-to-self branch. It gathers same-file bare anchors by scanning
`note.md_links` and cross-file anchors by filtering `index.links_to(path)` —
both filtered to the clicked heading's slug.

Link-before-heading order means a cursor on a link embedded in a heading line
(e.g. `## See [this](other.md)`) still returns backlinks to the link target,
not anchor references to the heading. Moving the cursor to the heading prefix or
plain text falls through to the heading branch.

**TDD cycle:**

1. Write unit tests first.
2. Run `cargo test` — new tests fail (cursor on heading currently falls through
   to the "no link → backlinks to self" branch).
3. Implement; `cargo test` passes; `cargo clippy -- -D warnings` clean.

**Deliverables:**

- `src/handlers.rs`: add `find_heading_at_position(note, pos) -> Option<&Heading>`
  helper (analogous to `find_md_link_at_position`)
- `src/handlers.rs`: add heading-at-cursor branch in `handle_references` (~line
  570), after the tag check, before the link check

**Unit tests:**

| Test | What it verifies |
| ---- | ---------------- |
| `references_heading_includes_same_file_bare_anchors` | Note with `## Section` and `[link](#section)` → results include that link's location |
| `references_heading_includes_cross_file_anchors` | Note A with `## Section`; note B with `[text](a.md#section)` → results include B's link location |
| `references_heading_excludes_non_matching_anchors` | Note A with `## Section`; note B links `[text](a.md#other)` → that link not in results |
| `references_heading_no_refs_returns_empty` | Heading with zero inbound anchor links (same-file or cross-file) → empty vec |

> **Manual checkpoint:** Open a Markdown note with `## Introduction` and a bare
> anchor link `[go to intro](#introduction)` in the same file. Also have a
> second note in the same vault that contains `[link](this-note.md#introduction)`.
> Place the cursor on the `## Introduction` heading line and trigger Find
> References. The references panel should show two results: the bare anchor link
> in the current file and the cross-file link from the second note.

---

## Step 5 — Integration tests

End-to-end tests over the full LSP message loop exercising all four stories
together. Always the last step.

**Deliverables:**

- New test cases added to `tests/lsp.rs`
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test | What it verifies |
| ---- | ---------------- |
| `test_same_file_anchor_definition` | `textDocument/definition` on `[text](#section)` in a note with `## Section` → location in same file at heading line |
| `test_same_file_anchor_definition_missing` | `textDocument/definition` on `[text](#missing)` → location at top of same file |
| `test_same_file_anchor_broken_diagnostic` | `textDocument/didOpen` with `[text](#missing)` → `textDocument/publishDiagnostics` contains one warning for that anchor |
| `test_same_file_anchor_valid_no_diagnostic` | `textDocument/didOpen` with `[text](#existing)` and matching heading → diagnostics list empty |
| `test_same_file_anchor_completion` | `textDocument/completion` at `[text](#` → completion items include current file's headings |
| `test_same_file_anchor_references_on_heading` | `textDocument/references` on heading line in note with a bare anchor `[link](#slug)` → returns that link's location |

> **Manual checkpoint (full session):** Open a real vault in your editor. In a
> single note: write two headings (`## Alpha` and `## Beta`) and two bare anchor
> links (`[go to alpha](#alpha)` and `[go to broken](#nope)`). Verify:
> (1) `#nope` shows a warning squiggle, `#alpha` does not;
> (2) Go to Definition on `[go to alpha](#alpha)` jumps to `## Alpha`;
> (3) Find References on `## Alpha` shows `[go to alpha](#alpha)` plus any
> cross-file links to that heading;
> (4) Typing `[new link](#` opens the completion menu with `Alpha` and `Beta`.
> Confirm cross-file anchor completion, definition, and diagnostics (v0.3
> features) are unaffected.

---

## Done — v0.7 complete

| Story | Feature | Delivered in step |
| ----- | ------- | ----------------- |
| US-51 | Anchor completions for `[text](#` — heading list scoped to the current file | Step 1 |
| US-48 | Go to Definition on `[text](#slug)` — navigates to matching heading in the current file | Step 2 |
| US-50 | Diagnostic when a bare anchor doesn't match any heading in the current file | Step 3 |
| US-49 | Find References on a heading — includes same-file bare anchor links | Step 4 |
