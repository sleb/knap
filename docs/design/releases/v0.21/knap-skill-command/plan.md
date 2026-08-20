# v0.21 Implementation Plan — `knap skill` Command

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the CLI should be manually verified against a real
shell.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                       | Status | Notes |
| ------------------------------------------- | ------ | ----- |
| 1 — CLI wiring: `knap skill` arg parsing    | Done   |       |
| 2 — `skill::run` write-if-different logic   | Done   |       |
| 3 — Integration tests                       | Done   |       |
| 4 — Docs: story, roadmap, README            | Done   |       |

---

## Step 1 — CLI wiring: `knap skill` arg parsing

Add the `Skill` subcommand variant and its `ArgGroup` to `src/cli/mod.rs` so
`--global`/`--path` mutual-exclusion and required-ness are enforced by clap
before any file I/O exists to test. This is the smallest testable unit: arg
parsing behavior is observable via `Cli::try_parse` without touching disk.

This step uses TDD:

1. Write the unit tests below first, calling `Cli::try_parse_from` directly
   (stub `Commands::Skill { global: bool, path: Option<PathBuf> }` and a
   `mod skill;` with a `pub fn run(_global: bool, _path: Option<PathBuf>) ->
   anyhow::Result<()> { Ok(()) }` placeholder so the crate compiles).
2. Run `cargo test` and confirm the new tests **fail** (they'll fail to
   compile or fail assertions until the `ArgGroup` is added).
3. Implement the `#[command(group(...))]` attribute until tests pass, then
   run `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/cli/mod.rs`: `mod skill;` declaration
- `src/cli/mod.rs`: `Commands::Skill { global: bool, path: Option<PathBuf> }`
  variant with `#[command(group(clap::ArgGroup::new("target").required(true).args(["global", "path"])))]`
- `src/cli/mod.rs`: dispatch arm `Commands::Skill { global, path } =>
  skill::run(global, path)` in `run()`
- `src/cli/skill.rs`: placeholder `pub fn run(global: bool, path:
  Option<PathBuf>) -> anyhow::Result<()>` returning `Ok(())` (real logic
  lands in Step 2)

**Unit tests:**

| Test                                       | What it verifies                                                    |
| -------------------------------------------- | ----------------------------------------------------------------- |
| `path_and_global_are_mutually_exclusive`     | `Cli::try_parse_from(["knap", "skill", "--global", "--path", "x"])` returns `Err` |
| `neither_path_nor_global_is_required`        | `Cli::try_parse_from(["knap", "skill"])` returns `Err`             |
| `global_alone_parses`                        | `Cli::try_parse_from(["knap", "skill", "--global"])` returns `Ok` with `global: true, path: None` |
| `path_alone_parses`                          | `Cli::try_parse_from(["knap", "skill", "--path", "x"])` returns `Ok` with `global: false, path: Some("x")` |

> **Manual checkpoint:** Run `cargo run -- skill` in a terminal — clap
> prints a usage error naming `--global`/`--path` as required, not a Rust
> panic. Run `cargo run -- skill --global --path /tmp/x` and confirm clap
> reports the conflict instead of silently picking one.

---

## Step 2 — `skill::run` write-if-different logic

Implement the actual embed-and-write behavior described in the design doc.
This step depends on Step 1's compiling `Commands::Skill` variant.

This step uses TDD:

1. Write all unit tests below first, against the real `skill::run` signature
   (delete the Step 1 placeholder body, keep the signature).
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement until tests pass, then run `cargo clippy -- -D warnings`.

**Deliverables:**

- `Cargo.toml`: add `dirs = "6"` to `[dependencies]`
- `Cargo.toml`: one-line comment above `exclude = [...]` noting
  `skill/knap/` must stay out of that list because `skill::SKILL_MD` embeds
  it at compile time
- `src/cli/skill.rs`: `const SKILL_MD: &str = include_str!("../../skill/knap/SKILL.md");`
- `src/cli/skill.rs`: `fn resolve_target_dir(global: bool, path: Option<PathBuf>) -> anyhow::Result<PathBuf>` —
  `~/.claude/skills/knap` via `dirs::home_dir()` for `--global` (erroring
  with a `--path`-suggesting message if `home_dir()` is `None`), or the
  absolute form of `path` (relative paths resolved against
  `std::env::current_dir()`)
- `src/cli/skill.rs`: `pub fn run(global: bool, path: Option<PathBuf>) -> anyhow::Result<()>` —
  `create_dir_all`, read-compare-write-if-different against
  `<target_dir>/SKILL.md`, prints `installed`/`updated`/`already up to date`

**Unit tests:**

| Test                                       | What it verifies                                                          |
| --------------------------------------------- | --------------------------------------------------------------------- |
| `writes_skill_md_into_new_target_dir`         | fresh `--path <dir>` with no prior `.claude/skills/knap` creates parents and writes `SKILL.md` matching `SKILL_MD` |
| `rerun_with_unchanged_content_is_a_no_op`      | running twice in a row with no upstream change reports "already up to date" and leaves the file's mtime untouched |
| `rerun_over_a_stale_copy_overwrites_it`        | a target `SKILL.md` with different bytes is overwritten to match `SKILL_MD`, reported "updated" |
| `relative_path_resolves_against_cwd`           | `--path relative/dir` writes to `<cwd>/relative/dir/SKILL.md`, not literally `./relative/dir` |

> **Manual checkpoint:** Run `cargo run -- skill --path /tmp/knap-skill-test`
> in a terminal, then `cat /tmp/knap-skill-test/SKILL.md` and confirm it
> matches `skill/knap/SKILL.md` byte-for-byte (`diff` the two). Run the same
> command again and confirm the second run prints "already up to date"
> instead of "installed".

---

## Step 3 — Integration tests

End-to-end tests over the built `knap` binary, exercising the real
`skill/knap/SKILL.md` from the source tree rather than an in-process stub.
Always the last code step.

**Deliverables:**

- `tests/cli.rs`: `skill_path_subcommand_writes_skill_md` and
  `skill_path_subcommand_is_idempotent`
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                       | What it verifies                                                             |
| --------------------------------------------- | ---------------------------------------------------------------------- |
| `skill_path_subcommand_writes_skill_md`       | `knap skill --path <tempdir>` exits 0 and `<tempdir>/SKILL.md` matches `skill/knap/SKILL.md` read straight from the source tree |
| `skill_path_subcommand_is_idempotent`         | running the same command twice produces a byte-identical file and the second run's stdout contains "already up to date" |

> **Manual checkpoint (full session):** From a clean checkout, run `cargo
> install --path .` to build the real release binary, then run `knap skill
> --global` from an arbitrary directory (not the knap checkout) and confirm
> `~/.claude/skills/knap/SKILL.md` is created — this is the actual
> `cargo install knap`-only path the issue is about, not just `cargo run`
> from inside the source tree.

---

## Step 4 — Docs: story, roadmap, README

Record the story this release delivers and replace the manual `cp`
instructions with the new command. No code changes; doc-only, so no unit or
integration tests — verified by reading the diff.

**Deliverables:**

- `docs/USER_STORIES.md`: add **US-D22** under "The Skill" section, next to
  US-D15 — "As an agent or developer, I can run `knap skill --global` or
  `knap skill --path <dir>` to install or update the shipped `SKILL.md`
  without a source checkout, so a `cargo install knap`-only setup still gets
  the skill and it never drifts from the running binary's version."
- `docs/ROADMAP.md`: add a `v0.21` entry following the existing per-release
  format (goal paragraph, stories table, "LSP capabilities delivered" line
  omitted since none are added — CLI-only release, matching e.g. v0.18's
  "Publish to crates.io" entry which also added no LSP capability)
- `README.md`: replace the `cp -r skill/knap ~/.claude/skills/` /
  `cp -r skill/knap <workspace>/.claude/skills/` block (around the "Coding
  agents" section) with `knap skill --global` / `knap skill --path
  <workspace>/.claude/skills/knap`, keeping the surrounding prose about what
  `SKILL.md` teaches

> **Manual checkpoint:** Read the rendered `README.md` "Coding agents"
> section top to bottom — confirm it reads as a coherent instruction (no
> leftover reference to copying from a checkout) and that the `--path`
> example matches the exact directory shape `skill::resolve_target_dir`
> expects (ends in `.claude/skills/knap`, not the workspace root).

---

## Done — v0.21 complete

| Story  | Feature                                                        | Delivered in step |
| ------ | --------------------------------------------------------------- | ------------------ |
| US-D22 | `knap skill --global \| --path <dir>` installs/updates SKILL.md | Step 3              |
