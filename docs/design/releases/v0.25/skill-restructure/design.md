# v0.25 Design — Skill Restructure & npm Discoverability

Covers the stories in the v0.25 release:

| Story  | Feature                                                                                          |
| ------ | ------------------------------------------------------------------------------------------------- |
| US-D15 | `skills/knap/SKILL.md` (amended) — split into a short entrypoint + `reference/*.md` per subcommand |
| US-D22 | (removed) — replaced by a new story: install via `npx skills add sleb/knap`, no `knap` CLI involved |

Delivers GH issue #76.

---

## Goal

An agent picking up knap in a new workspace should get the skill the same
way it gets any other agent skill — `npx skills add sleb/knap` (project
scope) or `npx skills add sleb/knap -g` (global) — instead of via a
knap-specific `knap skill` subcommand that only this one project ships.
That only works if the skill lives where the `skills` package's discovery
convention looks: a top-level `skills/<name>/SKILL.md`, not knap's current
`skill/knap/SKILL.md` (singular `skill/`, which `npx skills` never scans).
Moving the directory and dropping the bespoke installer are one change, not
two: once `npx skills` can fetch the skill straight from GitHub, `knap
skill`'s only job — getting `SKILL.md` onto disk without a source checkout —
is redundant, and its compile-time `include_str!` embedding becomes a
liability the moment `SKILL.md` stops being the only file the skill ships.

One capability is genuinely lost, not just relocated: `knap skill --path
<dir>` could target *any* directory (e.g. a workspace's own
`.claude/skills/knap`, or a non-standard location). `skills add` only has
two scopes — project (auto-detected, installs under the current project's
agent directories, e.g. `.claude/skills/`) and `-g` global
(`~/.claude/skills/`) — there's no arbitrary-path equivalent. This is
acceptable because `--path` was always used for exactly one of those two
cases in practice (README/GETTING_STARTED both only ever showed `--global`
or a project's `.claude/skills/knap`, never an arbitrary third location),
but it's a real, user-visible narrowing worth calling out in the
CHANGELOG's breaking-change note, not just an implementation detail.

That "stops being the only file" is issue #76's original ask: today
`skill/knap/SKILL.md` is a single ~300-line file that documents `lint` and
`apply` in depth (diagnostic codes, `--suggest` ranking/tie-break, the
repoint round trip) while `rename-*` gets one bullet and `index` gets a
couple of paragraphs. Splitting it into a short entrypoint plus
per-subcommand `reference/*.md` files, each fleshed out to the depth
`lint`/`apply` already have, means an agent working a `rename-*`-only task
loads only `reference/rename.md`, not the full `lint --suggest`
ranking/tie-break write-up. This was deliberately deferred behind issue #77
(CLI-emitted `hint` fields, shipped in v0.24) so the rewritten reference
files describe the CLI's post-hints contract rather than decision rules
`hint` has since taken over.

---

## Skill Directory Move (`skill/` → `skills/`)

`git mv skill/knap skills/knap` — the `skills` package's discovery convention
(https://github.com/vercel-labs/skills) scans a top-level `skills/` directory
for `skills/<name>/SKILL.md` (up to two category levels deep), plus
agent-specific paths like `.claude/skills/`. It never scans `skill/`
(singular), so today's layout is invisible to `npx skills add sleb/knap`
regardless of any CLI change. No file inside `skill/knap/` changes as part
of the move itself — content changes are the restructure described below.

This repo's own `.claude/skills/` (containing knap's *contributor*-facing
skills — `knap-release`, `knap-design`, `knap-implementor`) is a distinct
concern: those are skills for working **on** the knap project, not the
product skill `skills/knap/` ships **for** consumers of the CLI. But because
`.claude/skills/` is itself one of the paths `npx skills` scans, doing
nothing here means `npx skills add sleb/knap` (or `-l`) would surface all
three contributor skills alongside the real one. Mark each with
`metadata.internal: true` in its frontmatter — the package's own mechanism
for exactly this ("only visible/installable when `INSTALL_INTERNAL_SKILLS=1`
is set; useful for skills meant only for internal tooling") — so discovery
against this repo shows only `skills/knap/`:

```yaml
---
name: knap-design
description: >
  ...
metadata:
  internal: true
---
```

Applies to all three: `.claude/skills/knap-design/SKILL.md`,
`.claude/skills/knap-implementor/SKILL.md`,
`.claude/skills/knap-release/SKILL.md`.

`skills/knap/SKILL.md` frontmatter already satisfies the package's
requirements (`name: knap`, lowercase-hyphenated; `description` present) —
no frontmatter change needed for discovery, only for trimming per the
restructure below.

---

## Skill Documentation Changes (`skills/knap/`)

Split into:

```
skills/knap/
  SKILL.md              # entrypoint: edit-verify loop, when to reach for
                         # the skill, a subcommand map, links to reference/
  reference/
    lint.md              # diagnostic codes, --suggest ranking/tie-break,
                          # examples (moved from today's SKILL.md near-verbatim)
    apply.md              # repoint-link/repoint-anchor, batching, the
                          # round-trip guard and its error, range-reuse caveat
    rename.md             # rename-file/-heading/-tag: argument shapes, what
                          # "atomic rewrite" covers, failure modes, aliases
    index.md               # single-doc vs. workspace-wide output shapes
```

**`SKILL.md`** keeps: the frontmatter, "When to reach for this", the
edit → verify loop's decision table (which `code`/situation routes to which
subcommand), and a one-line pointer to each `reference/*.md` file next to
the row it's relevant to. It drops the deep `--json` examples, the
`--suggest` ranking prose, and the round-trip example — those move to
`reference/lint.md` and `reference/apply.md` respectively. Target: short
enough that an agent orienting itself reads the whole thing before its first
tool call, the way the file reads today up through "The edit → verify loop"
(README.md's frontmatter-length precedent).

**`reference/lint.md`**: the diagnostic-code table, the "How to pick, not
just that you must" ranking guidance (`combined`/`distance`/`text_distance`,
`text_mismatch`), the `--json` diagnostics example, the `--suggest`
candidates example, the `text_mismatch` example, and the
`blocking_count`/`problem_count` example — all lifted from today's
`SKILL.md` with no content change beyond folding in the `hint` field each
example's JSON now carries (v0.24).

**`reference/apply.md`**: the `repoint-link`/`repoint-anchor` round-trip
example, the "never re-derive `range` by hand" warning and its
"unparseable markdown" error example, the note that `rename-*` ops compose
with repoint ops in the same batch, and `apply`'s `hint` field (points at
`knap lint` to confirm cleanliness).

**`reference/rename.md`** (new depth, per issue #76 request #1): argument
shapes for `rename-file <old> <new> [--json]` (alias `move-file`),
`rename-heading <file> <old> <new> [--json]`, `rename-tag <old> <new>
[--json]`; what "atomic rewrite" covers (every affected link/heading-anchor
rewritten in one pass, all-or-nothing); the fact that none of the three take
a dedicated JSON *report* shape of their own beyond the plain-text
`"old → new (N file(s) touched)"` summary — the only JSON connected to a
rename is `apply`'s `ApplyReport` when a rename op rides in an `apply`
batch (already covered in `reference/apply.md`), so this file states that
honestly instead of inventing a `--json` shape that doesn't exist; and
failure-mode examples drawn from `rename.rs`'s own error strings: `<path>:
no such file`, `<path>: already exists`, `<file>: no heading matching
<old>`, `no note uses tag <old>`.

**`reference/index.md`**: the `knap index <file> --json` single-doc shape
(`headings`/`links`/`backlinks`/`tags`) vs. the `knap index <dir> --json`
workspace shape (`notes`/`tags`), and `index`'s `hint` field (flags a broken
link, points at `knap lint --suggest`).

---

## CLI Changes

### Remove `Commands::Skill`

Delete the `Skill` variant from the `Commands` enum in `src/cli/mod.rs`
(including its `clap::ArgGroup` on `global`/`path`), its dispatch arm, and
its four unit tests (`path_and_global_are_mutually_exclusive`,
`neither_path_nor_global_is_required`, `global_alone_parses`,
`path_alone_parses`). Delete `src/cli/skill.rs` entirely and its `mod skill;`
declaration in `src/cli/mod.rs`. Delete the two integration tests in
`tests/cli.rs` that exercise `knap skill --path` (`skill_path_subcommand_...`)
and any shared setup only they use.

### Dependency Changes

Remove `dirs = "6"` from `Cargo.toml` — it was added solely for
`skill::resolve_target_dir`'s `--global` home-directory resolution
(confirmed: no other module references `dirs::`). Remove the `Cargo.toml`
comment noting `skill/knap/` must stay out of `exclude` — that constraint
disappears once nothing `include_str!`s it. `skills/knap/` (the whole
directory, now multiple files) still ships in the published crate the same
way `skill/knap/SKILL.md` did (it was never in `exclude`), which matters
only for anyone reading the crate off crates.io/`cargo doc`, not for
`npx skills`, which fetches from GitHub directly.

Handler Changes, Config Changes, Parser Changes, Note Index Changes: none —
this release touches no LSP-reachable code; `knap skill` was never advertised
as a protocol capability.

---

## Documentation Changes

- **`README.md`** — "Coding agents" section: replace the `knap skill
  --global` / `knap skill --path <dir>` block with `npx skills add
  sleb/knap` (project scope, default) and `npx skills add sleb/knap -g`
  (global); mention `-a <agent>` to target a specific agent when more than
  one is installed. Update the `skill/knap/` path references to
  `skills/knap/`.
- **`docs/GETTING_STARTED.md`** — "5. Coding agents" section: same
  replacement, same path update.
- **`docs/ARCHITECTURE.md`** — drop the `skill` row from the CLI subcommand
  table (it's no longer a subcommand); the `skill/knap/SKILL.md` doc-loop
  description one paragraph up in the same doc updates its path too.
- **`docs/USER_STORIES.md`** — reword US-D15's `skill/knap/SKILL.md` path
  reference to `skills/knap/SKILL.md`; replace US-D22's body (keep the ID —
  same precedent as v0.17's US-D14 removal note) with: "As an agent or
  developer, I can run `npx skills add sleb/knap` (project scope) or `npx
  skills add sleb/knap -g` (global) to install the shipped skill directly
  from GitHub, with no knap-specific CLI command and no risk of the
  installed copy drifting from a stale local `knap` binary's embedded
  copy." Add a
  scope note at the top of the file (mirroring the v0.17 `knap fix` removal
  note) recording that `knap skill` was dropped in v0.25 in favor of `npx
  skills`.
- **`CHANGELOG.md`** — new `v0.25` entry: skill moved to `skills/knap/` for
  `npx skills` discoverability, `SKILL.md` split into `SKILL.md` +
  `reference/{lint,apply,rename,index}.md`, `knap skill` subcommand removed
  (breaking: anyone scripting `knap skill --global`/`--path` needs `npx
  skills add sleb/knap [-g]` instead — note `--path <arbitrary-dir>` has no
  direct equivalent; only project/global scope are supported).
- Every other design/plan doc under `docs/design/releases/archive/` that
  references `skill/knap/SKILL.md` (v0.13, v0.15 ×2, v0.17, v0.19, v0.21,
  v0.24) and `docs/design/experiments/agentic-efficiency-benchmark.md` stays
  as-is — those are historical records of what was true when written, same
  as the v0.17 scope note's treatment of pre-removal story IDs. Only
  `docs/design/releases/archive/v0.21/knap-skill-command/` is a candidate
  for a "superseded in v0.25" note at the top of its `design.md`, matching
  how superseded designs are marked elsewhere in the archive (check
  `docs/design/releases/archive/v0.17/drop-fix/design.md`'s own front-matter
  note for the pattern before adding one).

---

## Testing

This release is docs + CLI-surface removal; there's no new runtime behavior
to unit-test beyond confirming the removal is total and the move didn't
break what already worked.

### Unit tests

| Test                                                          | What it verifies                                                              |
| --------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| (none — `Commands::Skill` and its 4 tests are deleted, not added) | `cargo test` has no remaining reference to `Commands::Skill` or `skill::run`    |

### Integration tests

| Test                                          | What it verifies                                                                        |
| ------------------------------------------------ | -------------------------------------------------------------------------------------------- |
| `knap_skill_subcommand_no_longer_exists`         | `knap skill --global` exits non-zero with clap's "unrecognized subcommand" error            |
| `cli_help_omits_skill_subcommand`                | `knap --help` output does not contain the string `skill`                                    |

### Manual checkpoint

Run `npx skills add . -l` (local-path form, `-l`/`--list` lists available
skills without installing) from the repo root after the move and confirm it
lists `knap` as a discovered skill at `skills/knap/` before publishing —
this is the one thing no automated test in this repo can verify, since it
depends on an external package's live discovery behavior.
