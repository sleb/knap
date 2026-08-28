# v0.25 Implementation Plan — Skill Restructure & npm Discoverability

Describes the order in which changes are made, what is tested after each
step, and the checkpoints where the result should be manually verified.

The guiding principle: each step produces something testable. No step lays
down untested code for the next step to build on.

---

## Status

| Step                                                    | Status | Notes |
| -------------------------------------------------------- | ------ | ----- |
| 1 — Move `skill/knap` → `skills/knap`                    | Done   |       |
| 2 — Split `SKILL.md` into entrypoint + `reference/*.md`  | Done   |       |
| 3 — Remove `knap skill` CLI subcommand                   | Done   |       |
| 4 — Update docs (README, GETTING_STARTED, ARCHITECTURE, USER_STORIES, CHANGELOG) | Done   |       |
| 5 — Integration tests + full verification                | Done   |       |

---

## Step 1 — Move `skill/knap` → `skills/knap`

Get the directory where `npx skills` can actually find it before touching
its contents, so every later step edits files at their final path.

**Deliverables:**

- `git mv skill/knap skills/knap` (moves `SKILL.md` as-is; content untouched
  in this step).
- Remove the now-empty `skill/` directory if `git mv` doesn't already clean
  it up.
- Update the one source reference to the old path ahead of Step 3 removing
  it anyway: `src/cli/skill.rs`'s `include_str!("../../skill/knap/SKILL.md")`
  → `include_str!("../../skills/knap/SKILL.md")`, and the `Cargo.toml`
  comment `# skill/knap/ must stay out of this list...` → `skills/knap/`.
  (Both lines are deleted outright in Step 3 — this keeps `cargo build`
  green in between rather than leaving it broken for two steps.)
- `tests/cli.rs`'s `std::fs::read_to_string("skill/knap/SKILL.md")` →
  `"skills/knap/SKILL.md"` (also deleted in Step 3, same reasoning).
- Add `metadata:\n  internal: true` to the frontmatter of
  `.claude/skills/knap-design/SKILL.md`,
  `.claude/skills/knap-implementor/SKILL.md`, and
  `.claude/skills/knap-release/SKILL.md` — without this, `npx skills`
  scanning this repo (it treats `.claude/skills/` as a discovery path same
  as `skills/`) would surface knap's contributor-only tooling skills
  alongside the actual product skill at `skills/knap/`.

**Unit tests:** none — this step is a pure move plus a path-string update
that existing tests already cover (they'll fail to compile/pass if the path
is wrong), plus a frontmatter-only edit to three unrelated skill files.

> **Manual checkpoint:** `cargo build` succeeds and `cargo test skill`
> passes with no code change beyond the path strings — confirms the move
> alone didn't break the still-live `knap skill` command. Separately, run
> `npx skills add . -l` and confirm `knap-design`/`knap-implementor`/
> `knap-release` do **not** appear in the listing.

---

## Step 2 — Split `SKILL.md` into entrypoint + `reference/*.md`

Now that the directory is at its final path, do the content restructure
described in the design doc's "Skill Documentation Changes" section.

**Deliverables:**

- `skills/knap/reference/lint.md`: diagnostic-code table, `--suggest`
  ranking/tie-break guidance (`combined`/`distance`/`text_distance`,
  `text_mismatch`), the three JSON examples (`--json` diagnostics,
  `--suggest` candidates, `text_mismatch`), `blocking_count` vs.
  `problem_count` example — moved from today's `SKILL.md`, `hint` field
  folded into each JSON example as it now appears in real CLI output.
- `skills/knap/reference/apply.md`: the `repoint-link`/`repoint-anchor`
  round-trip example, the range-copy warning and "unparseable markdown"
  error example, the rename-ops-compose-with-repoint-ops note, `apply`'s
  `hint` field.
- `skills/knap/reference/rename.md`: argument shapes for all three
  `rename-*` subcommands (+ `move-file` alias), what "atomic rewrite"
  covers, the "no dedicated `--json` shape" clarification, and the four
  failure-mode examples (`no such file`, `already exists`, `no heading
  matching`, `no note uses tag`) — verify each error string against
  `src/cli/rename.rs`'s actual `anyhow::ensure!`/`anyhow::anyhow!` text
  before writing the example, don't paraphrase from memory.
- `skills/knap/reference/index.md`: single-doc vs. workspace-wide `--json`
  shapes, `index`'s `hint` field.
- `skills/knap/SKILL.md` rewritten down to: frontmatter, "When to reach for
  this", the edit → verify loop's decision table, and one link to each
  `reference/*.md` placed next to the row/step it backs.

**Unit tests:** none — Markdown content, not exercised by `cargo test`.

> **Manual checkpoint:** Open `skills/knap/SKILL.md` and read it start to
> finish — confirm it reads as a complete, short loop on its own with no
> dangling forward reference that isn't backed by an actual
> `reference/*.md` link. Then open each `reference/*.md` file and diff its
> JSON examples against a real `knap lint --json --suggest` /
> `knap apply --json` / `knap index <file> --json` run in this repo to
> confirm the `hint` field and every other value is current, not a stale
> copy-paste from the old `SKILL.md`.

---

## Step 3 — Remove `knap skill` CLI subcommand

The skill is discoverable and complete at its new path; the bespoke
installer is now dead weight.

**Deliverables:**

- Delete `src/cli/skill.rs`.
- `src/cli/mod.rs`: remove `mod skill;`, the `Skill { .. }` variant (and its
  `clap::ArgGroup` attribute) from `Commands`, and its `Commands::Skill =>
  skill::run(...)` dispatch arm.
- `src/cli/mod.rs`'s `#[cfg(test)]` module: delete
  `path_and_global_are_mutually_exclusive`, `neither_path_nor_global_is_required`,
  `global_alone_parses`, `path_alone_parses`.
- `Cargo.toml`: remove `dirs = "6"` from `[dependencies]` and the `# skills/knap/
  must stay out of this list...` comment near `exclude` (confirm via `grep
  -rn "dirs::" src/` that nothing else references it before removing).
- `Cargo.lock`: regenerate via `cargo build` after the `Cargo.toml` edit.
- `tests/cli.rs`: delete the `knap skill --path`-exercising tests
  (`skill_path_subcommand_writes_skill_md` and its siblings around line
  1263–1295) and any fixture/setup only they used.

**Unit tests:**

| Test | What it verifies |
| ---- | ----------------- |
| (none added — this step only deletes) | `cargo test` has zero references to `Commands::Skill`, `skill::run`, or `SKILL_MD` after the step |

> **Manual checkpoint:** `cargo build` succeeds, `cargo clippy -- -D
> warnings` is clean, and `knap --help` no longer lists `skill` among the
> subcommands.

---

## Step 4 — Update docs

Bring every doc that names `knap skill` or `skill/knap/` into agreement with
Steps 1–3.

**Deliverables:**

- `README.md` "Coding agents" section: replace the `knap skill --global` /
  `knap skill --path <dir>` code block with `npx skills add sleb/knap`
  (project scope) and `npx skills add sleb/knap -g` (global); update the
  `skill/knap/SKILL.md` prose reference to `skills/knap/SKILL.md`.
- `docs/GETTING_STARTED.md` "5. Coding agents" section: same replacement,
  same path update.
- `docs/ARCHITECTURE.md`: delete the `skill` row from the CLI subcommand
  table; update the `skill/knap/SKILL.md` path reference in the paragraph
  above it.
- `docs/USER_STORIES.md`:
  - Reword US-D15's `skill/knap/SKILL.md` reference to
    `skills/knap/SKILL.md`.
  - Replace US-D22's body with the `npx skills add sleb/knap [-g]` story
    text from the design doc, keeping the `US-D22` ID.
  - Add a scope note near the top of the file (same form as the existing
    v0.17 `knap fix` removal note) recording that `knap skill` was dropped
    in v0.25 in favor of `npx skills`.
- `CHANGELOG.md`: new `v0.25` entry covering the directory move, the
  `SKILL.md` split, and the `knap skill` removal — mark the removal as
  breaking for anyone scripting `knap skill --global`/`--path` directly,
  and call out that `--path <arbitrary-dir>` has no direct `npx skills`
  equivalent (only project/global scope are supported).
- `docs/design/releases/archive/v0.21/knap-skill-command/design.md`: add a
  short "superseded in v0.25" note at the top, matching the pattern used by
  `docs/design/releases/archive/v0.17/drop-fix/design.md` for a design that
  was later reversed — read that file's note first to match its exact form
  before writing this one.

**Unit tests:** none — docs only.

> **Manual checkpoint:** `grep -rn "knap skill\|skill/knap" README.md
> docs/GETTING_STARTED.md docs/ARCHITECTURE.md docs/USER_STORIES.md
> CHANGELOG.md` returns nothing (every live reference updated; archive/
> design docs are intentionally left alone per the design doc's
> Documentation Changes section).

---

## Step 5 — Integration tests + full verification

Confirm the removal is total and the new layout actually works end to end.

**Deliverables:**

- `tests/cli.rs`: add `knap_skill_subcommand_no_longer_exists` (asserts
  `knap skill --global` exits non-zero with clap's unrecognized-subcommand
  error) and `cli_help_omits_skill_subcommand` (asserts `knap --help`'s
  stdout doesn't contain the substring `skill`).
- `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` all pass.

**Unit tests:** none beyond the two integration tests above (this step adds
no library code).

| Test                                    | What it verifies                                                              |
| ------------------------------------------ | -------------------------------------------------------------------------------- |
| `knap_skill_subcommand_no_longer_exists`   | `knap skill --global` exits non-zero via clap's own unrecognized-subcommand error |
| `cli_help_omits_skill_subcommand`          | `knap --help` stdout does not contain `skill`                                    |

> **Manual checkpoint (full session):** From the repo root, run `npx skills
> add . -l` (`-l`/`--list` lists available skills in the target without
> installing) and confirm it reports `knap` discovered at `skills/knap/`.
> This is the one check nothing else in this plan can substitute for, since
> it depends on the external `skills` package's live behavior rather than
> anything in this repo.

---

## Done — v0.25 complete

| Story  | Feature                                                              | Delivered in step |
| ------ | ----------------------------------------------------------------------- | ------------------ |
| US-D15 | `skills/knap/SKILL.md` split into entrypoint + `reference/*.md`         | Step 2              |
| US-D22 | Install via `npx skills add sleb/knap [-g]`, `knap skill` CLI removed   | Steps 1, 3, 4       |
