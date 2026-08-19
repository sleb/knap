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
- **Binary vs library shape**: `knap` publishes as a binary with only `cli`
  public — that's the one module `main.rs` needs across the crate boundary.
  Everything else (`config`, `edit`, `handlers`, `index`, `parser`, `server`)
  is `pub(crate)`; `server::run` is already called in-crate from
  `src/cli/lsp.rs`/`src/cli/check.rs`, so it never needed to be public for the
  binary itself. `tests/` and `examples/` (compiled as separate crates) still
  need `server::run` and `handlers::slug`, so those two are re-exported behind
  an opt-in `test-support` Cargo feature (`#[doc(hidden)]`, gated), enabled
  automatically for this crate's own dev builds via a self-referencing
  `[dev-dependencies]` entry — invisible to a normal `cargo add knap`.
  Implemented in `4a95d16`.

## Plan

1. ~~**Metadata pass on `Cargo.toml`**~~ — done. `description`, `license`,
   `repository`, `readme`, `keywords`, `categories` added; no `homepage` (no
   separate docs site). Implemented in `7623139`.

2. ~~**Decide binary vs library shape**~~ — done, see Decisions above.

3. ~~**Trim package contents**~~ — done. Added `exclude = ["tests/", "docs/",
"examples/"]` to `Cargo.toml`. Confirmed with `cargo package --list`: no
   test fixtures, scratch dirs, or secrets in the remaining 44 files (source,
   metadata, `schemas/`, `scripts/`, `skill/`, `.claude/skills/`,
   `.github/`). `cargo package` produces a 135 KB `.crate` (599.6 KiB
   uncompressed, 131.8 KiB compressed) and passes verification.
   Implemented in `4c9d23b`.

4. ~~**Local dry run**~~ — done. `cargo publish --dry-run` packaged and
   verified cleanly (only expected warnings: excluded `examples/`/`tests/`
   files ignored, plus one pre-existing unrelated dead-code warning);
   aborted the upload as intended. `cargo package` + `tar tzvf` on the
   `.crate` confirmed the same 44 files / ~135 KB seen in step 3, nothing
   unexpected.

5. ~~**crates.io account/auth**~~ — done. crates.io account confirmed
   (GitHub OAuth), scoped API token generated, `cargo login` run
   (`~/.cargo/credentials.toml` present, 0600). Additional owners: none for
   now — `cargo owner --add` can happen any time after the first publish if
   that changes later.

6. ~~**Fold into `/knap-release`**~~ — done. `.claude/skills/knap-release/SKILL.md`
   now has a "Publish to crates.io" step right after commit/tag/push:
   `cargo publish --dry-run` as a safety gate, then real `cargo publish`.
   Renumbered the old "Post-release" step to 9 and folded in crates.io/docs.rs
   verification and (first-publish-only) README badge reminders.

7. **Publish**
   - `cargo publish` (real). One-way door — can't delete a version, only
     yank.

8. **Post-publish**
   - Add crates.io badge to `README.md` (`docs.rs` badge too — docs.rs
     auto-builds from crates.io)
   - Verify `docs.rs/knap` builds cleanly
   - Tag the release / update `CHANGELOG.md` if not already part of the
     release flow
