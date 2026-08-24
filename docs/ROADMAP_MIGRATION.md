# Roadmap → GitHub Migration

`docs/ROADMAP.md` used to do three jobs at once: a changelog of every
released version, an unordered backlog of deferred work, and an ordered
"what's next" sequencing table. This migration (completed 2026-08-23) moved
each to a better home and retired the file. It's kept here only as a record
of the decisions made, since one of them (the `Next` milestone convention)
is still referenced by `.claude/skills/knap-release/SKILL.md`.

## Where things moved

| Was in `docs/ROADMAP.md`              | Now lives in                                                              |
| -------------------------------------- | -------------------------------------------------------------------------- |
| Released version history (v0.1–v0.23) | GitHub Releases (changelog) + `docs/design/releases/archive/vX.Y/**/design.md` (the "why") |
| Backlog section                        | GitHub Issues (nothing needed filing — see below)                        |
| Ordered "what's next" / status table   | GitHub Milestones — a single rolling `Next` milestone                    |
| Principles section                     | `docs/ARCHITECTURE.md` ("Release Principles")                            |

`docs/ROADMAP.md` was trimmed to a pointer file first, then deleted outright
once nothing depended on it as a distinct doc (2026-08-23). Its full
version-by-version history isn't duplicated anywhere; it's readable via
`git log`/`git show` on this repo's history, and the "why" for each release
lives in `docs/design/releases/archive/vX.Y/**/design.md`.

`docs/RELEASING.md` — an older, separate release checklist that had grown
stale references to the pre-trim `docs/ROADMAP.md` — was deleted in the same
pass, superseded by `.claude/skills/knap-release/SKILL.md`.

## Decisions worth keeping

- **Milestone naming: rolling `Next`.** Chosen over pre-numbered milestones
  to avoid committing to version numbers before scope is settled. A single
  open milestone titled `Next` holds near-term issues; `/knap-release` step
  10 renames it to the shipped version and closes it on release, then opens
  a fresh `Next`.
- **Backlog dedup found nothing to file.** `docs/ROADMAP.md`'s Backlog
  section had 7 bullets; all were either already shipped, already tracked as
  issues (closed `NOT_PLANNED`), or declared non-goals. The one net-new issue
  from this pass, [#81 — `[[wikilink]]` syntax](https://github.com/sleb/knap/issues/81),
  came up in discussion rather than from the Backlog section itself.
- **GitHub Projects (v2) was considered and rejected** for sequencing — more
  expressive (board/roadmap view with date fields) but heavier to maintain,
  and `gh` here lacks the `read:project` scope to manage it. Milestones were
  the lighter-weight match for "which issues ship in the next release."
- **Milestone-close automation was deferred, not skipped.** `/knap-release`
  step 10 already closes the shipped milestone and opens the next `Next` via
  `gh api`, run manually by whoever executes the skill. Whether that's worth
  scripting further has no signal yet — no release has exercised it. Revisit
  after the first one does.
