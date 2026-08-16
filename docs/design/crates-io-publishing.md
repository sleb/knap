# Publishing knap to crates.io

Status: draft — plan alignment in progress, not yet implemented.

## Current state

- `Cargo.toml` has only `name`, `version`, `edition` — missing the metadata
  crates.io needs/wants: `description`, `license`, `repository`, `readme`,
  `keywords`, `categories`, and probably `authors`/`homepage`.
- Crate name `knap` is unclaimed on crates.io (confirmed via API: 404).
- `LICENSE` (MIT) and a substantial `README.md` already exist in the repo.
- All dependencies are normal crates.io deps (no path/git deps) — good, since
  those block publishing.
- Current version in `Cargo.toml` is `0.15.0`; the active branch is
  `v0.16-exclude-paths`.

## Proposed plan

1. **Metadata pass on `Cargo.toml`**
   - Add `description` (one-liner, ~200 chars max for the crates.io listing)
   - Add `license = "MIT"` (matches `LICENSE` file)
   - Add `repository` (GitHub URL — confirm it's public before publishing)
   - Add `readme = "README.md"`
   - Add `keywords` (max 5, e.g. `lsp`, `markdown`, `linter`) and
     `categories` (from crates.io's fixed list, e.g.
     `command-line-utilities`, `development-tools`)
   - Add `homepage` if there's a docs site, otherwise skip

2. **Decide binary vs library shape**
   - Confirm `knap` is meant to publish as a binary crate (it's an LSP/CLI
     tool). Check `src/main.rs` vs `src/lib.rs` — if there's reusable lib
     code, `cargo publish` publishes both, so it's worth being intentional
     about what counts as public API.

3. **Sanity-check package contents**
   - `cargo package --list` to see exactly what gets included (verify no
     test fixtures, scratch dirs, or secrets sneak in — there are
     `tests/fixtures/...` dirs worth checking against `.gitignore` /
     `include` / `exclude` in `Cargo.toml`)
   - Consider an explicit `exclude` for `tests/`, `docs/`, `examples/` if
     they're large and not needed at install time (smaller crate, faster
     publish)

4. **Local dry run**
   - `cargo publish --dry-run` to catch metadata/lint errors before
     touching the real registry
   - `cargo package` + inspect the generated `.crate` tarball

5. **crates.io account/auth**
   - Confirm a crates.io account linked to GitHub, with an API token
     (`cargo login`)
   - Decide who else, if anyone, should be an owner (`cargo owner --add`)

6. **Versioning**
   - Current version is `0.15.0` in `Cargo.toml` but the branch is
     `v0.16-exclude-paths` — decide whether the first publish ships at
     `0.15.0`, or version bumps to `0.16.0` first as part of that release.
     Given the `/knap-release` skill already handles version bumps, this
     likely rides along with that flow rather than being a separate step.

7. **Publish**
   - `cargo publish` (real). One-way door — can't delete a version, only
     yank.

8. **Post-publish**
   - Add crates.io badge to `README.md` (`docs.rs` badge too — docs.rs
     auto-builds from crates.io)
   - Verify `docs.rs/knap` builds cleanly
   - Tag the release / update `CHANGELOG.md` if not already part of the
     release flow

## Open questions

Annotate inline below.

1. **Release flow integration** — Should this land as part of the existing
   `/knap-release` flow (version bump + tag + publish together), or as a
   standalone one-time "get set up on crates.io" branch first, with future
   versions using the normal release skill?

   >

2. **Package trimming** — Do you want `tests/`, `docs/`, `examples/`
   excluded from the published package via `exclude` in `Cargo.toml`, or is
   crate size a non-issue here?

   >

3. **Repository URL** — What's the canonical GitHub remote to put in
   `Cargo.toml`'s `repository` field? Does it need creating/making public
   first, or does it already exist?

   >
