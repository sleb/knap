# Releasing knap

---

## Workflow (GitHub flow)

`main` is always the latest **released** state. Every commit on `main`
corresponds to a tagged release. This matters because `main` serves live
artifacts that users depend on (e.g. `schemas/v1/initialization_options.json`
at its raw GitHub URL) — if in-progress work lands on `main`, users get a
schema that doesn't match their installed binary.

### Branches

| Branch pattern      | Purpose                                                    |
| ------------------- | ---------------------------------------------------------- |
| `main`              | Released code only — never commit work-in-progress here    |
| `feat/<short-name>` | A feature, story, or doc change (e.g. `feat/us-31-schema`) |
| `fix/<short-name>`  | A bug fix or patch (e.g. `fix/anchor-range-off-by-one`)    |

Cut every branch from `main`:

```bash
git checkout main
git pull
git checkout -b feat/us-32-backlinks
```

### Merging

Merge to `main` as the **last step of releasing**, not before. The flow is:

1. Do all work on the feature branch.
2. Run the full release checklist (below) while still on the branch.
3. When everything passes, merge to `main` and tag immediately.

```bash
git checkout main
git merge --no-ff feat/us-32-backlinks -m "Release v0.7.0"
git tag -a v0.7.0 -m "v0.7.0"
git push && git push --tags
```

The `--no-ff` flag keeps the merge commit even if a fast-forward is possible,
so the graph shows clearly where each feature landed.

### Patches

For a patch release (bug fix against a shipped version), cut the branch from
the relevant tag rather than from the current tip of `main`:

```bash
git checkout -b fix/broken-anchor-crash v0.7.0
# fix, test, then merge back to main and tag v0.7.1
```

### What goes straight to `main`

Typo fixes in docs that don't touch any live artifact (the schema files, the
server binary) can go directly to `main`. When in doubt, use a branch.

---

## Versioning

knap follows [Semantic Versioning 2.0.0](https://semver.org/). Given a version
`MAJOR.MINOR.PATCH`:

| Increment | When                                                             |
| --------- | ---------------------------------------------------------------- |
| `PATCH`   | Bug fixes that don't add or change behaviour                     |
| `MINOR`   | A roadmap milestone is complete (new LSP capabilities shipped)   |
| `MAJOR`   | Breaking change to the server's public interface or config shape |

Before `1.0.0`, minor version bumps may include breaking changes — this is
standard pre-1.0 practice under semver. Each roadmap milestone maps to one minor
release (`v0.1`, `v0.2`, …). Patch releases may be cut between milestones for
critical bug fixes.

---

## Release checklist

Work through these in order. Every item must pass before tagging.

### 1. Verify the implementation is complete

- [ ] All steps in `docs/design/v{N}/plan.md` are marked ✅ Done
- [ ] All user stories for the milestone are implemented (cross-check
      `docs/ROADMAP.md`)

### 2. Verify docs are in sync with the code

The long-lived architecture and component docs must accurately reflect the
current implementation before a release is tagged. Drift accumulates during
development; the release is the forcing function to clear it.

Check each of these against the source:

- [ ] **`docs/ARCHITECTURE.md`** — `Config` shape, Note Index method names,
      handler table, Debug CLI table, data-flow descriptions, invariants
- [ ] **`docs/design/components/parser.md`** — dependency versions, all public
      types (`Note`, `WikiLink`, `Heading`, `Frontmatter`, `Tag`,
      `MarkdownLink`), `parse()` body, extraction function signatures
- [ ] **`docs/design/components/note-index.md`** — `NoteIndex` struct fields,
      `resolve()` lookup strategy, `index()`/`remove()` steps, all read methods,
      `build()` signature
- [ ] **`docs/design/components/handlers.md`** — handler signatures (no stale
      `_config` param), return types, diagnostic message strings, all handlers
      present for shipped capabilities
- [ ] **`docs/design/components/protocol-handler.md`** — `Config` struct,
      capabilities block, notification routing table
- [ ] **`docs/GETTING_STARTED.md`** — CLI examples, configuration option table,
      troubleshooting commands

Fix any drift found before continuing. These edits belong in their own commit
(or can be squashed into the release commit if they're trivial).

### 3. Quality gates

```bash
cargo test                    # all tests pass
cargo clippy -- -D warnings   # zero warnings
```

### 4. Update docs

- [ ] **`CHANGELOG.md`** — add an entry for the new version at the top (see
      format below); this becomes the GitHub release body automatically
- [ ] **`Cargo.toml`** — bump `version` to the new version string
- [ ] **`README.md`** — update the version badge; update the "What it does"
      feature list to reflect only what is actually shipped in this release
      (remove features that are still in future milestones)
- [ ] **`docs/ROADMAP.md`** — add the release date to the completed milestone
      heading (e.g. `## v0.1 — MVP _(released YYYY-MM-DD)_`)
- [ ] **`docs/design/v{N}/plan.md`** — confirm all steps show ✅ Done in the
      status table

**CHANGELOG entry format:**

```markdown
## [0.9.0] — YYYY-MM-DD

### Added

- Short user-facing description (US-XX)

### Fixed

- ...

### Changed

- ...
```

Use only the sections that apply. Write from the user's perspective — what
changed in their editor, not what changed in the code.

### 5. Commit

Stage only the files changed in steps 2–4. Include any doc files changed during
the sync check:

```bash
git add CHANGELOG.md Cargo.toml Cargo.lock README.md docs/ROADMAP.md docs/design/v{N}/plan.md
# plus any docs/ARCHITECTURE.md or docs/design/components/*.md changed in step 2
git commit -m "Release v{VERSION}"
```

### 6. Tag

```bash
git tag -a v{VERSION} -m "v{VERSION}"
```

### 7. Push

```bash
git push
git push --tags
```

Pushing the tag triggers the release workflow, which:

1. Extracts the top entry from `CHANGELOG.md` and creates the GitHub release
2. Builds binaries for all platforms and attaches them to the release

No manual `gh release create` needed.

---

## After the release

- [ ] Verify the GitHub release page looks correct (notes match CHANGELOG, all
      binaries attached)
- [ ] Open the next milestone in `docs/ROADMAP.md` — create
      `docs/design/v{N+1}/plan.md` if it doesn't already exist
