# Multi-Feature Release Folders — Design

Changes how `knap-design` and `knap-release` lay out release design docs so a
single `vX.Y` release can bundle multiple features or bugfixes, each with its
own design doc and plan, instead of one release folder holding exactly one
`design.md`/`plan.md` pair.

---

## Goal

Today every knap release is one feature: `docs/design/releases/vX.Y/design.md`

- `plan.md`. That forces a version bump per feature even when several small
  features or bugfixes would ship together naturally. This change lets a
  release folder hold multiple feature subfolders, each independently designed
  and planned, while the release as a whole still ships and archives as one
  unit.

---

## Directory structure

```
docs/design/releases/vX.Y/
  <feature-slug>/
    design.md
    plan.md
  <another-feature-slug>/
    design.md
    plan.md
```

- Every release folder always uses this subfolder layout — even a
  single-feature release gets one subfolder. No conditional "flat vs nested"
  logic in either skill.
- `<feature-slug>` is a kebab-case slug derived from the feature name (e.g.
  "Batch Apply" → `batch-apply`, "Fix empty-file diff" → `fix-empty-file-diff`).
- A version ships as a whole: all feature subfolders under `vX.Y/` must be
  complete before that version releases. Features are not archived
  individually — `knap-release` archives the entire `vX.Y/` directory in one
  move.
- `docs/design/releases/templates/design.md` and `templates/plan.md` are
  unchanged; only the output path changes.

---

## `knap-design` changes

**Phase 1 — Understand the scope:**

- Ask the user for both the version (`vX.Y`) and the feature name.
- Slugify the feature name into `<feature-slug>`.
- If `docs/design/releases/vX.Y/` already exists with other feature
  subfolders (a prior `knap-design` invocation already planned another
  feature for this release), skim each sibling's `design.md` goal section for
  context — avoid duplicate slugs and overlapping scope. Don't re-derive or
  re-validate their content.

**Phase 3/4 — Output paths:**

- Design doc: `docs/design/releases/vX.Y/<feature-slug>/design.md`
- Plan: `docs/design/releases/vX.Y/<feature-slug>/plan.md`

No other phase changes — template guidance, cross-checks, and style rules
stay as they are, just scoped to one feature's files.

---

## `knap-release` changes

**Step 2 — Verify implementation is complete:**

- Glob `docs/design/releases/v{N}/*/plan.md` (every feature subfolder under
  the target version). Every step in every matched plan must show ✅ Done.
- Report which feature(s), if any, are incomplete and stop, same as today.

**Step 5 — Update version and release docs:**

- **CHANGELOG.md:** pull content from every feature's plan under `v{N}`,
  combined into one changelog entry for the release.
- **ROADMAP.md:** the version heading gets one subsection per feature
  (`### <feature title>`), each with its own short goal paragraph and stories
  table — replacing today's single combined goal + stories table per version.
  The version heading itself keeps the release date as today.

**Step 6 — Archive the release design docs:**

- Replace the two-file `git mv` with a single directory move:
  ```bash
  git mv docs/design/releases/v{N} docs/design/releases/archive/v{N}
  ```
- Link sweep: the existing grep for `design/releases/v{N}/` still works
  unchanged (it matches on prefix), but expect one match per feature
  subsection in ROADMAP.md now instead of one per version. Fix every match to
  point at `docs/design/releases/archive/v{N}/<feature-slug>/...`.

**Step 7 — Commit, tag, and push:**

- Stage `docs/design/releases/archive/v{N}/` (the whole directory) instead of
  the two individual files.

No other step changes — quality gates (Step 4), docs-sync (Step 3), and
post-release reminders (Step 8) are unaffected.

---

## Out of scope

- No change to `templates/design.md` / `templates/plan.md` content — they're
  already scoped to one feature.
- No migration of already-archived single-feature releases (v0.1–v0.14) to
  the new layout.
- No support for archiving individual features before the rest of their
  version is done (the "whole version ships at once" model, per user
  decision).
