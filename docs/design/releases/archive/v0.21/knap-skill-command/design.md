# v0.21 Design — `knap skill` Command

Covers the stories in the v0.21 release:

| Story  | Feature                                                          |
| ------ | ----------------------------------------------------------------- |
| US-D22 | `knap skill --global \| --path <dir>` installs/updates the shipped SKILL.md |

Delivers GH issue #71.

---

## Goal

An agent or developer working from a `cargo install knap`-only setup can run
`knap skill --global` (or `knap skill --path <dir>` for a project-scoped
install) to place the shipped `SKILL.md` where their coding agent looks for
it, with no source checkout required. Because the file is embedded in the
binary at compile time, the installed copy can never drift from the running
`knap` version the way a hand-maintained `cp` from a cloned repo can — running
the same command again after a `knap` upgrade re-syncs it. `knap skill install`
and `knap skill update` collapse into one idempotent command per the issue's
bikeshed resolution: the operation is write-if-different either way, so there
is no "nothing installed yet" or "already installed" failure mode to design
around.

---

## CLI Changes

### `knap skill` (new subcommand)

```rust
#[command(group(clap::ArgGroup::new("target").required(true).args(["global", "path"])))]
Skill {
    /// Install to `~/.claude/skills/knap/SKILL.md`.
    #[arg(long)]
    global: bool,
    /// Install to `<dir>/SKILL.md`, creating `<dir>` if needed.
    #[arg(long, value_name = "DIR")]
    path: Option<PathBuf>,
},
```

`--global` and `--path <dir>` are mutually exclusive and one is required —
enforced by a clap `ArgGroup`, not manual validation, so `clap::Parser`
rejects `knap skill` (no flags) and `knap skill --global --path x` with its
own usage message before `skill::run` is ever called. This mirrors the issue's
resolution to skip a bare-default target: a silent write to `~/.claude/skills/`
on an unqualified `knap skill` would surprise a caller who meant the project's
`.claude/skills/`.

`--path <dir>` takes the exact skill directory to populate (e.g.
`<vault>/.claude/skills/knap`), not a vault root — `SKILL.md` is written
directly inside `<dir>`. This matches how the issue's example
(`--path <vault>/.claude/skills/knap/`) already spells out the full path.

New module `src/cli/skill.rs`:

```rust
/// The shipped agent skill, embedded at compile time so an installed copy
/// can never drift from the running `knap` version.
const SKILL_MD: &str = include_str!("../../skill/knap/SKILL.md");

/// `knap skill --global | --path <dir>`: writes `SKILL_MD` into the target
/// directory as `SKILL.md`, creating parent directories as needed.
/// Write-if-different: a byte-identical target is left untouched (and
/// reported as already up to date) so re-running after a `knap` upgrade
/// with no SKILL.md changes doesn't touch the file's mtime.
pub fn run(global: bool, path: Option<PathBuf>) -> anyhow::Result<()>
```

Algorithm:

1. Resolve the target directory: `~/.claude/skills/knap` for `--global`
   (via the `dirs` crate's `home_dir()`), or the absolute form of `--path
   <dir>` (relative paths resolved against `std::env::current_dir()`,
   matching the resolution style already used by `rename::run_file`).
2. `std::fs::create_dir_all(&target_dir)`.
3. Read the existing `<target_dir>/SKILL.md` if present. If its contents
   equal `SKILL_MD` byte-for-byte, print `knap skill: <path> already up to
   date` and return — no write.
4. Otherwise `std::fs::write(<target_dir>/SKILL.md, SKILL_MD)` and print
   `knap skill: installed <path>` (first write) or `knap skill: updated
   <path>` (a prior differing file existed), so the same message
   distinguishes a fresh install from a version bump without the caller
   needing two subcommands.

Edge cases:

- `dirs::home_dir()` returns `None` (no resolvable home directory) → error
  out with a message telling the caller to use `--path` instead, rather than
  panicking or writing to a bogus relative path.
- `--global`'s resolved home directory has no `.claude/skills/` yet → created
  by `create_dir_all`, same as any other missing parent.
- `<target_dir>/SKILL.md` exists but is a directory, or `<target_dir>` itself
  is an existing regular file (not a directory) → the underlying
  `create_dir_all`/`write` I/O error propagates with `anyhow::Context`
  naming the path, not a generic OS error.
- Trailing content differences (e.g. a hand-edited installed copy with an
  extra trailing newline) count as "different" — byte comparison, not a
  semantic diff — so any manual edit to an installed `SKILL.md` is
  overwritten on the next `knap skill` run. This is intentional: the
  installed copy is meant to always match the running binary.

---

## Config Changes

None.

## Parser Changes

None.

## Note Index Changes

None.

## Handler Changes

None — this is a CLI-only addition; no LSP request handler is touched, and
`knap skill` does not need to be advertised as a protocol capability since
it isn't reachable from `knap lsp`.

---

## Dependency Changes

Add `dirs = "6"` to `[dependencies]` in `Cargo.toml` for cross-platform home
directory resolution (`--global`'s `~/.claude/skills/knap`). No existing
dependency in the tree provides this; `std` has no portable home-dir API.

`skill/knap/` (currently just `SKILL.md`) must not be added to `Cargo.toml`'s
`exclude = ["tests/", "docs/", "examples/"]` list — it already isn't, but the
`include_str!` at compile time means a future edit to that exclude list
would silently break `cargo publish` builds (the embedded path wouldn't
exist in the packaged crate). Worth a one-line comment in `Cargo.toml`
next to `exclude` calling this out.

---

## Testing

### Unit tests (`src/cli/skill.rs`)

| Test                                              | What it verifies                                                          |
| -------------------------------------------------- | --------------------------------------------------------------------------- |
| `writes_skill_md_into_new_target_dir`              | fresh `--path <dir>` with no prior `.claude/skills/knap` creates parents and writes `SKILL.md` matching the embedded constant |
| `rerun_with_unchanged_content_is_a_no_op`           | running twice in a row with no upstream change leaves the file's write untouched (reported "already up to date") |
| `rerun_over_a_stale_copy_overwrites_it`             | a target `SKILL.md` with different bytes is overwritten to match `SKILL_MD`, reported "updated" |
| `path_and_global_are_mutually_exclusive`            | `Cli::try_parse` on `["knap", "skill", "--global", "--path", "x"]` returns a clap error |
| `neither_path_nor_global_is_required`               | `Cli::try_parse` on `["knap", "skill"]` returns a clap error |

### Integration tests (`tests/cli.rs`)

| Test                                                 | What it verifies                                                             |
| ----------------------------------------------------- | -------------------------------------------------------------------------- |
| `skill_path_subcommand_writes_skill_md`               | `knap skill --path <tempdir>` exits 0 and `<tempdir>/SKILL.md` matches `skill/knap/SKILL.md` from the source tree |
| `skill_path_subcommand_is_idempotent`                 | running the same command twice produces byte-identical output file and the second run's stdout says "already up to date" |
