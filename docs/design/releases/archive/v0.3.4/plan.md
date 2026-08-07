# v0.3.4 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoint where the server is manually verified against a real editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                           | Status | Notes |
| ------------------------------ | ------ | ----- |
| 1 — Fix placeholder in handler | Done   |       |

---

## Step 1 — Fix placeholder in handler

Replace `heading.text.clone()` in `handle_prepare_rename` with raw text extracted
from `note.content` at `heading.text_range` using the existing `utf16_to_byte_offset`.

**Deliverables:**

- `src/handlers.rs`: `handle_prepare_rename` — placeholder extracted from raw source
- One new unit test: `prepare_rename_placeholder_is_raw_text`

**Unit tests:**

| Test                                     | What it verifies                                                                               |
| ---------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `prepare_rename_placeholder_is_raw_text` | Heading with `_..._` italic → placeholder is raw source text, not pulldown-cmark rendered text |

> **Manual checkpoint:** Open `docs/ROADMAP.md` in the editor. Place the cursor on
> the heading `## v0.3 — Heading Navigation & Anchors _(released 2026-05-16)_` (line 50).
> Trigger rename (`F2`). The dialog should appear pre-filled with the raw heading text
> including the `_..._` markers. Confirm rename succeeds.

---

## Done — v0.3.4 complete

| Story | Feature                                                   | Delivered in step |
| ----- | --------------------------------------------------------- | ----------------- |
| #3    | Rename dialog appears for headings with inline formatting | Step 1            |
