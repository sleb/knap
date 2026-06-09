# Releasing knap

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

- [ ] All steps in `docs/design/releases/v{N}/plan.md` are marked ✅ Done
- [ ] All user stories for the milestone are implemented (cross-check
      `docs/ROADMAP.md` and `docs/USER_STORIES.md`)

### 2. Verify docs are in sync with the code

The long-lived architecture and component docs must accurately reflect the
current implementation before a release is tagged. Drift accumulates during
development; the release is the forcing function to clear it.

Check each of these against the source:

- [ ] **`docs/ARCHITECTURE.md`** — `Config` shape, Note Index method names,
      handler table, Debug CLI table, data-flow descriptions, invariants
- [ ] **`docs/GETTING_STARTED.md`** — CLI examples, configuration option table,
      troubleshooting commands
- [ ] **`docs/design/components/parser.md`** — dependency versions, all public
      types (`Note`, `WikiLink`, `Heading`, `Frontmatter`, `Tag`,
      `MarkdownLink`), `parse()` body, extraction function signatures
- [ ] **`docs/design/components/note-index.md`** — `NoteIndex` struct fields,
      `resolve()` lookup strategy, `index()`/`remove()` steps, all read methods,
      `build()` signature
- [ ] **`docs/design/components/handlers.md`** — handler signatures, return
      types, diagnostic message strings, all handlers present for shipped
      capabilities
- [ ] **`docs/design/components/protocol-handler.md`** — `Config` struct,
      capabilities block, notification routing table
- [ ] **`docs/design/components/transport.md`** — transport layer description,
      any public types or interfaces

Fix any drift found before continuing.

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
- [ ] **`docs/ROADMAP.md`** — add the release date to the completed milestone
      heading (e.g. `## v0.1 — MVP _(released YYYY-MM-DD)_`)
- [ ] **`docs/design/releases/v{N}/plan.md`** — confirm all steps show ✅ Done

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

### 5. Commit, tag, and push

```bash
git add CHANGELOG.md Cargo.toml Cargo.lock README.md docs/ROADMAP.md docs/design/releases/v{N}/plan.md
# plus any docs/ files changed in step 2
git commit -m "Release v{VERSION}"
git tag -a v{VERSION} -m "v{VERSION}"
git push && git push --tags
```

Pushing the tag triggers the release workflow, which:

1. Extracts the top entry from `CHANGELOG.md` and creates the GitHub release
2. Builds binaries for all platforms and attaches them to the release

---

## After the release

- [ ] Verify the GitHub release page looks correct (notes match CHANGELOG, all
      binaries attached)
- [ ] Open the next milestone in `docs/ROADMAP.md` — create
      `docs/design/releases/v{N+1}/plan.md` if it doesn't already exist
