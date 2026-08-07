# v0.3.2 Design — Global Jump in Completions

## Stories

| Story | Type    | Description                                               |
| ----- | ------- | --------------------------------------------------------- |
| US-47 | Feature | Global file list alongside directory items in completions |

---

## Goal

v0.3.1 (US-46) gave writers segment-by-segment directory drilling. That is
excellent when you don't know the full path and want to explore, but it adds
friction when you do know the file — or can recognise it by title — and just
want to jump there.

v0.3.2 augments completion with a third tier: every workspace file not already
shown as an immediate child, sorted below the directory items. The writer can
now type any segment of a deep path (e.g. `meeting`) and reach
`notes/work/2026/meeting-notes.md` in one step, while the directory items
remain at the top for when exploration is the right move.

---

## Design

### Completion tiers

When `check_dir_trigger` fires (cursor inside `](…)` with no `#`), the handler
builds items in three sorted tiers:

| Tier | `sort_text` prefix | Contents                                                           |
| ---- | ------------------ | ------------------------------------------------------------------ |
| 0    | `0_`               | FOLDER items — immediate subdirectories of `base_dir`              |
| 1    | `1_`               | FILE items — files directly inside `base_dir` (immediate children) |
| 2    | `2_`               | FILE items — every other workspace file (global jump targets)      |

`sort_text` uses a prefix so editors that respect the field keep the tiers
ordered even when their fuzzy scorer would otherwise rerank items.

### Global items

A global item is emitted for every note and attachment in the index **except**:

- The file currently being edited (self-exclusion, same as tier 1)
- Files already emitted as tier-1 items (deduplication — no duplicates)

Each global item:

| Field         | Value                                                                      |
| ------------- | -------------------------------------------------------------------------- |
| `label`       | Frontmatter `title` if present; otherwise the bare filename                |
| `kind`        | `FILE`                                                                     |
| `detail`      | Full relative path from the current note's directory                       |
| `filter_text` | Full relative path — editors match against this, so any path segment works |
| `sort_text`   | `"2_"` + full relative path                                                |
| `text_edit`   | Replaces from `](` to cursor with the full relative path                   |

Setting `filter_text` to the full relative path (rather than just the filename)
is the key decision: it lets the editor's fuzzy matcher surface a file like
`sub/b.md` when the user types `b`, `sub`, or `sub/b`, without the full path
needing to appear in the label.

### text_edit range

The `replace_range` is the same for all three tiers: it starts immediately after
`](` (the first character the user would type) and ends at the cursor. This means
selecting any item from any tier always replaces the entire partial path the user
has typed — no orphaned prefix is left behind.

This is important for global items: if the user has typed `sub/`, selecting the
global item for `other.md` replaces `sub/` with `other.md` cleanly.

### Interaction with directory drilling

Global items are present even when the user has typed a partial like `](sub/`.
At that point, tier 1 contains the children of `sub/`, and tier 2 contains
everything else — including files in sibling directories the user may have
decided they want instead. The user can drill further (tier 0 FOLDER items if
`sub/` has nested dirs) or abandon the partial and pick any global file.

### Anchor completions unaffected

`check_anchor_trigger` runs before `check_dir_trigger`. When the cursor is after
`#`, the anchor branch returns early and no file-listing logic runs. No change.

---

## Alternatives considered

**Show full path in the label for global items.** Rejected: long labels clutter
the picker. `detail` is the right place for the path — editors render it as
secondary text or a tooltip. The label stays short and readable.

**Separate global items into a second completion request.** Rejected: LSP has no
built-in concept of "sections" within a completion list, and triggering a second
request would require editor-side coordination. A single sorted list with
`sort_text` tiers achieves the same ordering with zero protocol complexity.

**Only show global items at the initial `](` trigger, not when drilling.**
Rejected: the user may start drilling and then realise they want a file in a
sibling directory. Showing global items at every trigger point keeps the escape
hatch always available.

**Deduplicate by hiding tier-1 items from tier 2, but not vice versa.** This is
exactly what we do — tier-1 files are excluded from tier 2. Tier-2 files are
never shown in tier 1 (by definition they are not immediate children). No
further deduplication is needed.
