# Working notes for Claude

## Development workflow

Commit directly to `main`. Do not create feature branches.

## Release process

When the user asks to cut a release for version X.Y.Z:

1. `cargo test` — must be clean
2. `cargo clippy -- -D warnings` — must be clean
3. Bump `version` in `Cargo.toml` to X.Y.Z
4. Run `cargo build` to update `Cargo.lock`
5. Add `[X.Y.Z] — YYYY-MM-DD` section to `CHANGELOG.md`
6. Mark the release date in `docs/ROADMAP.md`
7. Commit everything: `git commit -m "chore(release): vX.Y.Z — <short description>"`
8. Push: `git push origin main`
9. Tell the user to run:
   ```
   git pull origin main
   git tag -a vX.Y.Z -m "vX.Y.Z"
   git push origin vX.Y.Z
   ```

Do NOT create the tag from this session — tag pushes via the local proxy return
403. The user pushes the tag directly from their terminal.
