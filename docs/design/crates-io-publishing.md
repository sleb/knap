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

   > yes, this should be part of the existing `/knap-release` flow

2. **Package trimming** — Do you want `tests/`, `docs/`, `examples/`
   excluded from the published package via `exclude` in `Cargo.toml`, or is
   crate size a non-issue here?

   > let's discuss. What is the trade-off here? When would I want to exclude these paths vs. include them?

   Discussion:
   - **Include everything (default)** — `cargo package` bundles whatever
     isn't gitignored, so the published tarball mirrors the repo exactly
     (tests, docs, fixtures included). Upside: anyone auditing the
     published source sees the same thing GitHub shows, and nothing breaks
     if a doc comment or README example ever references a path under those
     dirs. Downside: bigger download on every `cargo install`/`cargo add`,
     and crates.io enforces a hard size cap per crate (10 MB on the free
     tier) — the `tests/fixtures/exclude_*` dirs plus `docs/design/` add up
     to real, unnecessary weight for someone who only wants the binary.
   - **Exclude `tests/`, `examples/`, `docs/`** — smaller, faster-installing
     crate, no risk of tripping the size cap, and it draws a clear line
     between "distributable artifact" and "dev repo." Downside: nobody
     installing from crates.io can browse fixtures/design docs without
     going to GitHub (rarely a real use case), and you'd need to double
     check no doctest/example path depends on those dirs at build/test
     time from the packaged tarball.

   Recommendation: exclude `tests/`, `docs/`, `examples/` — none of it is
   needed to run the installed `knap` binary, and it keeps the published
   artifact lean and comfortably under the size cap. Worth confirming with
   `cargo package --list` before locking it in (see plan step 3).

   > (resolved — proceed with excluding `tests/`, `docs/`, `examples/`,
   > pending confirmation via `cargo package --list`)

3. **Repository URL** — What's the canonical GitHub remote to put in
   `Cargo.toml`'s `repository` field? Does it need creating/making public
   first, or does it already exist?

   > Let's discuss. What is the canonical GitHub remote to put in `Cargo.toml`'s `repository` field? The current repo is `sleb/knap`. Is that the right choice?

   Discussion: confirmed via `gh repo view sleb/knap` — the repo exists and
   is already **public**. `https://github.com/sleb/knap` is the correct
   value; no setup needed first.

   > (resolved — `repository = "https://github.com/sleb/knap"`)
