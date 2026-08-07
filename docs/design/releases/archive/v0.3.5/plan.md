# v0.3.5 Implementation Plan

---

## Status

| Step                                      | Status | Notes |
| ----------------------------------------- | ------ | ----- |
| 1 — Fix LineIndex UTF-16 + text_range end | Done   |       |

---

## Step 1 — Fix LineIndex UTF-16 + text_range end

**Deliverables:**

- `src/parser/mod.rs`: `LineIndex` stores content; `position()` computes UTF-16
- `src/parser/mod.rs`: End(Heading) handler uses `byte_range.end - 1` for text_range end
- `src/parser/tests.rs`: `line_index_utf16_multibyte`
- `src/parser/tests.rs`: `heading_text_range_trailing_italic`
- `src/parser/tests.rs`: `heading_text_range_with_em_dash_utf16`

> **Manual checkpoint:** Open `docs/ROADMAP.md`. Place cursor on
> `## v0.3 — Heading Navigation & Anchors _(released 2026-05-16)_`. Trigger
> rename (`F2`). Dialog appears pre-filled with the full raw heading text
> including `_(released 2026-05-16)_`. Confirm rename succeeds.

---

## Done — v0.3.5 complete

| Story | Feature                                              | Delivered in step |
| ----- | ---------------------------------------------------- | ----------------- |
| #4    | LSP ranges use UTF-16; text_range covers full markup | Step 1            |
