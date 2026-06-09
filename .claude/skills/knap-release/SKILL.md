---
name: knap-release
description: >
  Walk through the knap release checklist interactively. Verifies docs are in
  sync with code, runs quality gates, updates version/CHANGELOG/README/ROADMAP,
  commits, tags, and pushes. Invoke with /knap-release.
---

# knap Release

You are executing the knap release process. Work through every step below in
order, performing each check yourself rather than asking the user to do it.
Report clearly what passed, what drifted, and what you fixed.

## Step 1 — Confirm the target version

Ask the user: "What version are we releasing?" (e.g. `0.6.0`). Use that
version string as `{VERSION}` throughout. Derive `v{N}` (e.g. `v0.6`) for
the release plan path.

## Step 2 — Verify implementation is complete

- Read `docs/design/releases/v{N}/plan.md`. Every step must show ✅ Done.
- Read `docs/ROADMAP.md` and `docs/USER_STORIES.md`. Confirm all milestone
  user stories are implemented.

Report any incomplete items and stop if any are found — do not proceed to step
3 until the user confirms they are resolved.

## Step 3 — Verify docs are in sync with the code

For each doc below, read the doc and the relevant source files side-by-side
(use LSP `hover`/`goToDefinition` to resolve types as needed). List every
discrepancy found, then fix each one.

| Doc | What to check |
|-----|---------------|
| `docs/ARCHITECTURE.md` | `Config` shape, Note Index method names, handler table, Debug CLI table, data-flow descriptions, invariants |
| `docs/GETTING_STARTED.md` | CLI examples, configuration option table, troubleshooting commands |
| `docs/design/components/parser.md` | dependency versions, all public types (`Note`, `WikiLink`, `Heading`, `Frontmatter`, `Tag`, `MarkdownLink`), `parse()` body, extraction function signatures |
| `docs/design/components/note-index.md` | `NoteIndex` struct fields, `resolve()` lookup strategy, `index()`/`remove()` steps, all read methods, `build()` signature |
| `docs/design/components/handlers.md` | handler signatures, return types, diagnostic message strings, all handlers present for shipped capabilities |
| `docs/design/components/protocol-handler.md` | `Config` struct, capabilities block, notification routing table |
| `docs/design/components/transport.md` | transport layer description, public types or interfaces |

After fixing all drift, summarise: "Docs sync: N files updated, M files
already correct."

## Step 4 — Quality gates

Run both commands and report results:

```bash
cargo test
cargo clippy -- -D warnings
```

Stop if either fails and ask the user to fix the issues before continuing.

## Step 5 — Update version and release docs

Make all of these changes:

1. **`Cargo.toml`** — bump `version` to `{VERSION}`
2. **`CHANGELOG.md`** — prepend a new entry at the top:
   ```
   ## [{VERSION}] — {TODAY}

   ### Added / Fixed / Changed
   - ...
   ```
   Pull the content from the release plan and git log since the last tag.
   Use only the sections that apply. Write from the user's perspective.
3. **`README.md`** — update the version badge; update the "What it does"
   feature list to reflect only shipped features (remove future-milestone items)
4. **`docs/ROADMAP.md`** — add the release date to the completed milestone:
   `## v{MINOR} — <name> _(released {TODAY})_`
5. **`docs/design/releases/v{N}/plan.md`** — confirm all steps show ✅ Done
   (no edit needed if already done in step 2)

## Step 6 — Commit, tag, and push

Stage only the files changed in steps 3–5:

```bash
git add CHANGELOG.md Cargo.toml Cargo.lock README.md docs/ROADMAP.md \
  docs/design/releases/v{N}/plan.md
# plus any docs/ files updated in step 3
git commit -m "Release v{VERSION}"
git tag -a v{VERSION} -m "v{VERSION}"
git push && git push --tags
```

Report the commit hash and confirm the tag was pushed.

## Step 7 — Post-release

Remind the user to:
- Verify the GitHub release page (notes match CHANGELOG, all binaries attached)
- Open the next milestone in `docs/ROADMAP.md`
- Create `docs/design/releases/v{N+1}/plan.md` if it doesn't exist yet
