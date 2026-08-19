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

- Find every feature subfolder for this release:
  `docs/design/releases/v{N}/*/plan.md`. A release can bundle multiple
  features or bugfixes, each in its own subfolder.
- Read each matched `plan.md`. Every step in every one must show ✅ Done.
- Read `docs/ROADMAP.md` and `docs/USER_STORIES.md`. Confirm all user
  stories from every feature subfolder are implemented.

Report any incomplete items — naming which feature subfolder they're in —
and stop if any are found; do not proceed to step 3 until the user confirms
they are resolved.

## Step 3 — Verify docs are in sync with the code

For each doc below, read the doc and the relevant source files side-by-side
(use LSP `hover`/`goToDefinition` to resolve types as needed). List every
discrepancy found, then fix each one.

| Doc                                          | What to check                                                                                                                                               |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `docs/ARCHITECTURE.md`                       | `Config` shape, Note Index method names, handler table, Debug CLI table, data-flow descriptions, invariants                                                 |
| `docs/GETTING_STARTED.md`                    | CLI examples, configuration option table, troubleshooting commands                                                                                          |
| `docs/design/components/parser.md`           | dependency versions, all public types (`Note`, `WikiLink`, `Heading`, `Frontmatter`, `Tag`, `MarkdownLink`), `parse()` body, extraction function signatures |
| `docs/design/components/note-index.md`       | `NoteIndex` struct fields, `resolve()` lookup strategy, `index()`/`remove()` steps, all read methods, `build()` signature                                   |
| `docs/design/components/handlers.md`         | handler signatures, return types, diagnostic message strings, all handlers present for shipped capabilities                                                 |
| `docs/design/components/protocol-handler.md` | `Config` struct, capabilities block, notification routing table                                                                                             |
| `docs/design/components/transport.md`        | transport layer description, public types or interfaces                                                                                                     |

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
   Pull the content from every feature subfolder's plan under `v{N}` and the
   git log since the last tag. Use only the sections that apply. Write from
   the user's perspective.
3. **`README.md`** — update the version badge; update the "What it does"
   feature list to reflect only shipped features (remove future-milestone items)
4. **`docs/ROADMAP.md`** — add the release date to the completed version
   heading: `## v{MINOR} — <name> _(released {TODAY})_`. Give the version one
   subsection per feature subfolder (`### <feature title>`), each with its
   own short goal paragraph and stories table, in place of a single combined
   table.
5. **`docs/design/releases/v{N}/*/plan.md`** — confirm all steps in every
   feature subfolder show ✅ Done (no edit needed if already done in step 2)

## Step 6 — Archive the release design docs

Step 3 already reconciled anything from each feature's `design.md` that
belongs in `docs/ARCHITECTURE.md` or `docs/design/components/*.md` — every
release design doc's job is done. Move the whole release folder (with all
its feature subfolders) into the archive in one step, so it's preserved for
historical context but stops being read as a source of truth:

```bash
mkdir -p docs/design/releases/archive
git mv docs/design/releases/v{N} docs/design/releases/archive/v{N}
```

Then fix any doc that still links to the pre-archive path — most commonly
`docs/ROADMAP.md`'s "See `docs/design/releases/v{N}/<feature-slug>/design.md`..."
lines, one per feature subsection for this version:

```bash
grep -rn "design/releases/v{N}/" docs/ --include="*.md" | grep -v /archive/
```

Update every match to point at
`docs/design/releases/archive/v{N}/<feature-slug>/...`. While here, it costs
nothing to sweep for archive links a _previous_ release missed too — fix any
that turn up:

```bash
grep -rn "design/releases/v0\." docs/ --include="*.md" | grep -v /archive/
```

## Step 7 — Commit, tag, and push

Stage the files changed in steps 3, 5, and 6:

```bash
git add CHANGELOG.md Cargo.toml Cargo.lock README.md docs/ROADMAP.md \
  docs/design/releases/archive/v{N}/
# plus any docs/ files updated in steps 3 or 6
git commit -m "Release v{VERSION}"
git tag -a v{VERSION} -m "v{VERSION}"
git push && git push --tags
```

Report the commit hash and confirm the tag was pushed.

## Step 8 — Publish to crates.io

```bash
cargo publish --dry-run
```

Confirm it packages and verifies cleanly (warnings about `examples/`/`tests/`
being excluded are expected — ignore them). Stop and ask the user to fix the
issue before continuing if the dry run fails for any other reason.

Then publish for real — this is a one-way door, the version can never be
deleted, only yanked:

```bash
cargo publish
```

Report the confirmation and the crate's URL: `https://crates.io/crates/knap`.

## Step 9 — Post-release

- Verify `https://crates.io/crates/knap` shows `{VERSION}` as the latest
  version.
- Verify `https://docs.rs/knap` builds cleanly for `{VERSION}` (docs.rs
  starts the build automatically on publish; it can take a few minutes —
  check back rather than blocking on it).
- If `README.md` doesn't yet have crates.io/docs.rs badges (only true before
  the first-ever publish), add them now.
- Verify the GitHub release page (notes match CHANGELOG, all binaries attached)
- Open the next milestone in `docs/ROADMAP.md`
- Run `knap-design` for the first feature of `v{N+1}` if no feature subfolder
  exists under `docs/design/releases/v{N+1}/` yet
