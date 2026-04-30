---
name: release
description: >
  Interactive release checklist for knap. Use this skill whenever the user
  mentions cutting a release, tagging a version, shipping a milestone, or
  anything like "let's release", "time to tag", "ready to ship v0.x", or "how do
  I release". Walk through the full docs/RELEASING.md checklist step-by-step,
  surfacing current state, proposing each action, and waiting for explicit
  confirmation before doing anything irreversible.
---

# Release Skill

You are acting as a release co-pilot for the knap project. Your job is to walk
through the `docs/RELEASING.md` checklist interactively — one step at a time —
surfacing the current state of the repo, proposing each action clearly, and
waiting for the user to confirm before proceeding. Think of yourself as a
careful pairing partner who has read the guide so the user doesn't have to
re-read it each time.

**Never run destructive or externally-visible commands without explicit
confirmation.** This includes: `git tag`, `git push`, and `git push --tags`.
For file edits, show the proposed diff first and ask.

---

## Step 1 — Determine the version

Read `Cargo.toml` (the `version` field) and `docs/ROADMAP.md` to understand
where the project currently stands.

Identify:

- The current version string
- The next milestone that appears complete (all user stories listed under it are
  implemented, based on the roadmap and the plan status table)
- Whether the bump should be **major**, **minor**, or **patch** per the
  versioning table in `docs/RELEASING.md`

For knap pre-1.0: each roadmap milestone (`v0.1`, `v0.2`, …) maps to one
**minor** bump. A patch release is only appropriate for a critical bug fix
between milestones. A major bump would require a breaking public interface
change — very unlikely pre-1.0.

Propose the new version to the user and ask them to confirm before continuing:

> "Based on `Cargo.toml` (currently `{current}`) and the completed v{N}
> milestone in the roadmap, this looks like a **{major|minor|patch}** release.
> Proposed new version: **v{new}**. Does that look right?"

---

## Step 2 — Verify docs are in sync with the code

Before running quality gates, confirm that the long-lived architecture and
component docs accurately reflect the current implementation. Drift accumulates
during a milestone; the release is the forcing function to clear it.

Read each of the following and cross-check it against the source:

| Doc                                          | What to check                                                                                                   |
| -------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `docs/ARCHITECTURE.md`                       | `Config` shape, Note Index method names, handler table, Debug CLI table, data-flow call-sites, invariants       |
| `docs/design/components/parser.md`           | dependency version, all public types, `parse()` body, extraction function signatures                            |
| `docs/design/components/note-index.md`       | struct fields, `resolve()` strategy, `index()`/`remove()` steps, all read methods, `build()` signature          |
| `docs/design/components/handlers.md`         | handler signatures (no stale `_config`), return types, diagnostic message strings, all shipped handlers present |
| `docs/design/components/protocol-handler.md` | `Config` struct, capabilities block, notification routing table                                                 |
| `docs/GETTING_STARTED.md`                    | CLI examples, config option table, troubleshooting commands                                                     |

For each doc, briefly state whether it's clean or has drift. If drift is found:

> "I found the following inconsistencies in `{doc}`: {list}. I can fix these now
> before we continue, or you can fix them manually. Which do you prefer?"

Fix or wait as the user directs. Only proceed to Step 3 once all docs are
confirmed in sync. Note any files you changed — they'll be included in the
release commit in Step 5.

---

## Step 3 — Verify implementation completeness

Read `docs/design/v{N}/plan.md` for the milestone being released (substitute the
milestone number from Step 1).

Scan the Status table for any step **not** marked `✅ Done`.

- If all steps are ✅ Done: tell the user and move on.
- If any steps are incomplete: list them clearly and ask:

  > "The following steps in the v{N} plan aren't marked done yet: {list}. Would
  > you like to complete them before releasing, or update the plan to defer them
  > to the next milestone?"

  If the user wants to defer, help them edit the plan (remove those steps from
  the v{N} plan and note them in the roadmap's next milestone). If they want to
  complete them first, pause and tell them to come back when done.

Also cross-check `docs/ROADMAP.md`: every user story listed under the milestone
should be implemented. Call out any mismatch.

---

## Step 4 — Quality gates

Run both checks and show the output:

```bash
cargo test
cargo clippy -- -D warnings
```

Run `cargo test` first, then `cargo clippy -- -D warnings`.

- If both pass: confirm to the user and continue.
- If either fails: stop and show the failures. Ask:
  > "Tests/clippy didn't pass. Do you want to fix these before continuing, or is
  > there something here you'd like to skip or investigate?"

Do not proceed to Step 5 until both pass (or the user explicitly says to
continue anyway — their project, their call).

---

## Step 5 — Update docs

Walk through each update **one at a time**. Show the proposed change and ask for
confirmation before applying it.

### 5a. CHANGELOG.md — add release entry

Read the top of `CHANGELOG.md` to see the current latest entry. Draft a new
entry for the version being released and show it:

> "Here's the CHANGELOG entry I'd add at the top. Does this capture everything,
> or would you like to adjust anything?"

The entry format is:

```markdown
## [0.9.0] — YYYY-MM-DD

### Added

- Short user-facing description (US-XX)

### Fixed

- ...
```

Use only the sections that apply (`Added`, `Fixed`, `Changed`). Write from the
user's perspective — what changed in their editor, not what changed in the code.

This entry becomes the GitHub release body automatically when the tag is pushed.

Apply on confirmation.

### 5b. Cargo.toml — bump version

Show:

> "I'll change `version = \"{current}\"` → `version = \"{new}\"` in
> `Cargo.toml`. OK to apply?"

Apply on confirmation.

### 5c. README.md — version badge and feature list

Read `README.md`. Find the version badge and the "What it does" feature list (or
equivalent). Show what you propose to change:

- Badge URL: old version → new version
- Feature list: remove any items that are still in future milestones; keep only
  what is actually shipped

Show the proposed diff, then ask:

> "Here's what I'd change in README.md. Does this look right, or do you want to
> adjust anything?"

Apply on confirmation.

### 5d. docs/ROADMAP.md — add release date

Find the heading for the milestone being released. Propose changing it to
include the release date, e.g.:

`## v0.1 — MVP: Navigate your workspace` →
`## v0.1 — MVP: Navigate your workspace _(released {YYYY-MM-DD})_`

Show the change and confirm before applying.

### 5e. docs/design/v{N}/plan.md — final confirmation

Re-read the plan and confirm all steps show ✅ Done. No edits needed if Step 3
already verified this — just tell the user it's clean.

---

## Step 6 — Commit

Run `git diff` to show all pending changes. Then propose the commit:

> "I'll commit these files with the message:
>
> ```
> Release v{VERSION}
> ```
>
> Staged:
> `CHANGELOG.md Cargo.toml Cargo.lock README.md docs/ROADMAP.md docs/design/v{N}/plan.md`
> (plus any doc files changed during the Step 2 sync check)
>
> OK to commit?"

On confirmation, stage everything that was changed — including any architecture
or component docs updated in Step 2:

```bash
git add CHANGELOG.md Cargo.toml Cargo.lock README.md docs/ROADMAP.md docs/design/v{N}/plan.md
# also add any docs/ARCHITECTURE.md or docs/design/components/*.md changed in Step 2
git commit -m "Release v{VERSION}"
```

If `Cargo.lock` wasn't changed (no new deps), don't stage it.

---

## Step 7 — Tag

Show the exact command you'll run, then confirm:

> "I'll create an annotated tag:
>
> ```
> git tag -a v{VERSION} -m "v{VERSION}"
> ```
>
> OK?"

Run on confirmation.

---

## Step 8 — Push

Show both push commands and confirm together (they're a natural pair):

> "I'll push the commit and the tag:
>
> ```
> git push
> git push --tags
> ```
>
> OK to push?"

Run both on confirmation.

---

## Step 9 — Verify the GitHub release

The release workflow creates the GitHub release automatically when the tag is
pushed. It extracts the top entry from `CHANGELOG.md` and attaches all platform
binaries. Tell the user:

> "The release workflow should be running now. Once it completes, check:
>
> - The release body matches the CHANGELOG entry you just wrote
> - All platform binaries are attached
> - The release is not marked as a draft
>
> You can monitor it at: https://github.com/sleb/knap/actions"

---

## Step 10 — Post-release

Remind the user:

> "A few things to check off:
>
> - [ ] Open the GitHub release page and verify the tag, notes, and that it's
>       not a draft
> - [ ] Open `docs/ROADMAP.md` and start the next milestone section if it's
>       not already there
> - [ ] Create `docs/design/v{N+1}/plan.md` if it doesn't exist yet"

Offer to help with any of these if they'd like.

---

## General guidance

- Keep the conversation focused: one step at a time, no jumping ahead.
- When showing proposed changes, be concrete — show the actual before/after,
  not a vague description.
- If the user wants to skip a step or do something differently, respect that —
  this is their project. Just note any risk briefly if it's relevant.
- If you're unsure what the current state is, read the file before proposing
  anything.
- The full guide is at `docs/RELEASING.md` — refer the user there if they want
  more context on any decision.

```

```
