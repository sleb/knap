# Roadmap → GitHub Migration Plan

`docs/ROADMAP.md` currently does three jobs at once: a changelog of every
released version, an unordered backlog of deferred work, and an ordered
"what's next" sequencing table. Each of those already has (or will get) a
better home on GitHub. This doc records the plan and its status; it is not
itself the roadmap.

## Target state

| Today (`docs/ROADMAP.md`)         | Moves to                                                                  |
| ---------------------------------- | -------------------------------------------------------------------------- |
| Released version history (v0.1–v0.23) | GitHub Releases (changelog) + `docs/design/releases/archive/vX.Y/**/design.md` (the "why") — retired from ROADMAP.md, not duplicated |
| Backlog section                    | GitHub Issues                                                              |
| Ordered "what's next" / status table | GitHub Milestones (one per upcoming release, closed on ship)             |
| Principles section                 | Stays in a repo doc (not GitHub) — doesn't belong on the issue tracker    |

## Steps

1. **File backlog items as Issues, deduped against existing ones.** — done
   (2026-08-23)
2. **Create milestones for sequencing**, attach relevant open issues, close
   each on release. Update `/knap-release` to close the milestone as part of
   the release checklist. — done (2026-08-23)
3. **Trim `docs/ROADMAP.md`**: delete the released-history entries and the
   (now-empty) Backlog section; keep or relocate Principles. — done
   (2026-08-23)
4. **Update cross-links**: README, `docs/GETTING_STARTED.md`, and the
   `/knap-release` skill currently point at `docs/ROADMAP.md` — repoint them
   at Issues/Milestones (and Releases for history) as appropriate. — done
   (2026-08-23)
5. **Automate milestone-close in `/knap-release`**, if step 2's manual
   process proves worth automating. — deferred (2026-08-23): no release has
   exercised step 10 yet, so there's no signal on whether the `gh api` steps
   already added there need further automation. Revisit after the first
   release that runs through it.

## Step 1 — backlog dedup, done

`docs/ROADMAP.md`'s Backlog section had 7 bullets. None required filing:

| Backlog item                                                  | Resolution                                                             |
| --------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `--fail-on <severity>` threshold on `knap lint`                 | Already shipped in v0.13 — stale entry, not a real gap                     |
| Hover Previews (US-09, US-10, US-23)                             | Already tracked as #11, #12, #22 — closed `NOT_PLANNED`; staying closed    |
| Diagnostics & Validation (US-32, US-34)                          | Already tracked as #30, #31 — closed `NOT_PLANNED`; staying closed         |
| Inline Tags (US-40)                                              | Already tracked as #36 — closed `NOT_PLANNED`; staying closed              |
| Orphan Doc Detection (US-38)                                     | Already tracked as #35 — closed `NOT_PLANNED`; staying closed              |
| Doc Templates (US-42)                                            | Already tracked as #37 — closed `NOT_PLANNED`; staying closed              |
| Full Markdown formatting / Git integration / Graph visualization / Sync-publishing | Declared non-goals, not deferred work — not filed as issues |

The one net-new issue from this pass was
[#81 — Support `[[wikilink]]` syntax](https://github.com/sleb/knap/issues/81),
filed separately (it wasn't in the Backlog section — it came up in
discussion of note-vault use cases). With nothing left to migrate, the
Backlog section in `docs/ROADMAP.md` can simply be deleted in step 3 rather
than converted.

## Step 2 — milestone sequencing, done

Decisions (2026-08-23):

- **Milestone naming: rolling `Next`.** Chosen over pre-numbered milestones
  to avoid committing to version numbers before scope is settled. A single
  open milestone titled `Next` ([#17](https://github.com/sleb/knap/milestone/17))
  holds near-term issues; `/knap-release` step 10 renames it to the shipped
  version and closes it on release, then opens a fresh `Next`.
- **First milestone contents:** [#77](https://github.com/sleb/knap/issues/77)
  (agent-facing "next step" hints), [#78](https://github.com/sleb/knap/issues/78)
  (`--verbose` flag), and [#80](https://github.com/sleb/knap/issues/80)
  (Homebrew/npm/PyPI packaging) were attached. [#76](https://github.com/sleb/knap/issues/76)
  (SKILL.md restructure) and [#81](https://github.com/sleb/knap/issues/81)
  (wikilink syntax) were left unscheduled.

`/knap-release` (`.claude/skills/knap-release/SKILL.md`) was updated: new
step 10 closes the release milestone and opens the next `Next` milestone;
the old "Open the next milestone in `docs/ROADMAP.md`" bullet in the
post-release step was removed (superseded).

## Step 3 — trim ROADMAP.md, done

`docs/ROADMAP.md` shrank from the full version-by-version history and the
(already-empty per step 1) Backlog section down to a short pointer file:
intro paragraph, links to GitHub Releases / design-doc archive / Issues /
the `Next` milestone, and the Principles section (kept as-is — it's a
standing repo convention, not tracker content). The old per-version sections
(v0.1–v0.23) aren't duplicated anywhere else; they remain readable via
`git log`/`git show` on this file's prior revisions, and the "why" for each
release is still in `docs/design/releases/archive/vX.Y/**/design.md`. No
in-repo links pointed at `ROADMAP.md#<anchor>`, so nothing else needed
updating as part of this step.

## Step 4 — cross-links, done

Found and repoint `docs/ROADMAP.md` references outside this file:

- `docs/GETTING_STARTED.md` — no reference found (already clean).
- `README.md` — the Documentation-links bullet described `docs/ROADMAP.md`
  as "features grouped into releases, starting with the MVP" (stale as of
  step 3). Reworded to describe the file's post-trim contents and added
  links to GitHub Releases, Issues, and the `Next` milestone alongside it.
- `docs/design/releases/archive/README.md` — pointed to `docs/ROADMAP.md`
  for "release history and what shipped in each version," which is no
  longer true post-trim. Repointed to GitHub Releases and `CHANGELOG.md`.
- `.claude/skills/knap-release/SKILL.md` — several steps assumed the old,
  pre-trim `docs/ROADMAP.md` (a version-by-version history with per-release
  story tables): the frontmatter description, step 2's doc-read list, step
  5's "add a dated version heading to ROADMAP.md" instruction, step 6's
  example stale-link line, and step 8's `git add` list. All updated to drop
  `docs/ROADMAP.md` as a file the release process edits. Step 10's
  `docs/ROADMAP_MIGRATION.md` reference (for the `Next` milestone
  convention) was already correct and left as-is.

## Not chosen

- **GitHub Projects (v2)** for sequencing — more expressive (board/roadmap
  view with date fields) but heavier to maintain, and `gh` here lacks the
  `read:project` scope to manage it. Milestones were chosen instead as the
  lighter-weight match for "which issues ship in the next release."
