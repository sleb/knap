# Publishing knap to crates.io

Status: plan finalized, not yet implemented.

## Current state

- `Cargo.toml` has only `name`, `version`, `edition` — missing the metadata
  crates.io needs/wants: `description`, `license`, `repository`, `readme`,
  `keywords`, `categories`, and probably `authors`/`homepage`.
- Crate name `knap` is unclaimed on crates.io (confirmed via API: 404).
- `LICENSE` (MIT) and a substantial `README.md` already exist in the repo.
- All dependencies are normal crates.io deps (no path/git deps) — good, since
  those block publishing.
- The GitHub repo `sleb/knap` exists and is already public (confirmed via
  `gh repo view`) — `https://github.com/sleb/knap` is the `repository` value
  to use, no setup needed first.

## Decisions

- **Release flow**: publishing folds into the existing `/knap-release` flow
  (version bump + tag + publish together) rather than a standalone one-time
  branch. Future versions ship the same way.
- **Package contents**: exclude `tests/`, `docs/`, `examples/` from the
  published crate via `exclude` in `Cargo.toml`. None of it is needed to run
  the installed `knap` binary, and it keeps the package comfortably under
  crates.io's 10 MB size cap. Confirm the final file list with
  `cargo package --list` before locking it in.
- **Repository field**: `repository = "https://github.com/sleb/knap"`.

## Plan

1. **Metadata pass on `Cargo.toml`**
   - Add `description` (one-liner, ~200 chars max for the crates.io listing)
   - Add `license = "MIT"` (matches `LICENSE` file)
   - Add `repository = "https://github.com/sleb/knap"`
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

3. **Trim package contents**
   - Add an explicit `exclude` for `tests/`, `docs/`, `examples/` in
     `Cargo.toml`
   - `cargo package --list` to confirm exactly what's included (verify no
     test fixtures, scratch dirs, or secrets sneak in)

4. **Local dry run**
   - `cargo publish --dry-run` to catch metadata/lint errors before
     touching the real registry
   - `cargo package` + inspect the generated `.crate` tarball

5. **crates.io account/auth**
   - Confirm a crates.io account linked to GitHub, with an API token
     (`cargo login`)
   - Decide who else, if anyone, should be an owner (`cargo owner --add`)

6. **Fold into `/knap-release`**
   - Wire `cargo publish` into the existing release skill/flow so version
     bump, tag, and publish happen together for this and future releases.

7. **Publish**
   - `cargo publish` (real). One-way door — can't delete a version, only
     yank.

8. **Post-publish**
   - Add crates.io badge to `README.md` (`docs.rs` badge too — docs.rs
     auto-builds from crates.io)
   - Verify `docs.rs/knap` builds cleanly
   - Tag the release / update `CHANGELOG.md` if not already part of the
     release flow
