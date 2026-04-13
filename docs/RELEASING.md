# Releasing knap

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

### 2. Quality gates

```bash
cargo test                    # all tests pass
cargo clippy -- -D warnings   # zero warnings
```

### 3. Update docs

- [ ] **`Cargo.toml`** — bump `version` to the new version string
- [ ] **`README.md`** — update the version badge; update the "What it does"
      feature list to reflect only what is actually shipped in this release
      (remove features that are still in future milestones)
- [ ] **`docs/ROADMAP.md`** — add the release date to the completed milestone
      heading (e.g. `## v0.1 — MVP _(released YYYY-MM-DD)_`)
- [ ] **`docs/design/v{N}/plan.md`** — confirm all steps show ✅ Done in the
      status table

### 4. Commit

Stage only the files changed above:

```bash
git add Cargo.toml Cargo.lock README.md docs/ROADMAP.md docs/design/v{N}/plan.md
git commit -m "Release v{VERSION}"
```

### 5. Tag

```bash
git tag -a v{VERSION} -m "v{VERSION}"
```

### 6. Push

```bash
git push
git push --tags
```

### 7. Create the GitHub release

Write the release notes in a temporary file, then:

```bash
gh release create v{VERSION} \
  --title "v{VERSION}" \
  --notes-file /tmp/release-notes.md
```

**Release notes format:** use the roadmap milestone table as the starting point
— list the user stories shipped, note any design decisions or caveats worth
calling out, and include the `cargo install` command (once the crate is
published to crates.io) or build-from-source instructions.

---

## After the release

- [ ] Verify the GitHub release page looks correct (tag, notes, no draft)
- [ ] Open the next milestone in `docs/ROADMAP.md` — create
      `docs/design/v{N+1}/plan.md` if it doesn't already exist
