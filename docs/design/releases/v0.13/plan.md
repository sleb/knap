# v0.13 Implementation Plan — Headless CLI

Describes the order in which changes are made, what is tested after each
step, and the checkpoints where the CLI should be manually verified.

The guiding principle: each step produces something testable. No step lays
down untested code for the next step to build on.

---

## Status

| Step                                                | Status | Notes |
| --------------------------------------------------- | ------ | ----- |
| 1 — Dependencies                                    | Done   |       |
| 2 — Shared `Config` loader + `knap.toml`            | Done   |       |
| 3 — Rewire `server.rs`/`handlers.rs` to `config.rs` | Done   |       |
| 4 — `NoteIndex::report()` for `knap index --json`   | Todo   |       |
| 5 — `src/cli/` restructure + clap wiring            | Todo   |       |
| 6 — `knap lint`                                     | Todo   |       |
| 7 — `knap index` rewrite                            | Todo   |       |
| 8 — Full test suite + clippy                        | Todo   |       |
| 9 — Docs                                            | Todo   |       |
| 10 — Release coordination                           | Todo   |       |

---

## Step 1 — Dependencies

Add the crates the rest of this plan needs before anything references them.

**Deliverables:**

- `Cargo.toml`: add `clap = { version = "4", features = ["derive"] }`,
  `toml = "0.8"`, and `tempfile` under `[dev-dependencies]`.
- `cargo build` still green (unused deps only warn, not error).

---

## Step 2 — Shared `Config` loader + `knap.toml`

The core of this release: one config-loading module used identically by
`lsp`, `lint`, and `index`, so headless commands stop diverging from the LSP.

**Deliverables:**

- New `src/config.rs`: move `Config`, `FrontmatterSchema`, `SchemaField`,
  and the private `SchemaFieldOpts`/`FrontmatterSchemaOpts`/`InitOptions`
  shims out of `src/server/mod.rs:30-124` verbatim. Add `KnapToml`
  (snake_case), `find_knap_toml`, `load_knap_toml`, `for_lsp`, `for_path`,
  and the shared `FrontmatterSchema`-building helper. Add `pub mod config;`
  to `src/lib.rs`.
- Write the unit tests below **first** (they won't compile until the module
  exists — that's the expected failing state per `AGENTS.md` TDD), then
  implement until green.
- Do this in the same step as Step 3's rewiring, not split across two
  commits — an intermediate state with two competing `Config` types won't
  compile.

**Unit tests:**

| Test                                        | What it verifies                                                                                    |
| ------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `for_path_absent_knap_toml_uses_defaults`   | No `knap.toml` → defaults (`extensions == ["md"]`, `new_note_dir == None`)                          |
| `for_path_loads_knap_toml`                  | `extensions` from a `knap.toml` fixture reflected in `Config`                                       |
| `for_path_malformed_knap_toml_errors`       | Invalid TOML syntax → `Err`, not silently defaulted                                                 |
| `for_path_knap_toml_wrong_type_errors`      | `extensions = "md"` (string, not array) → `Err`                                                     |
| `for_lsp_knap_toml_and_init_options_merge`  | `knap.toml` sets `extensions`, `initializationOptions` sets `new_note_dir` — both present in result |
| `for_lsp_init_options_overrides_knap_toml`  | Conflicting `extensions` in both sources — `initializationOptions` wins                             |
| `for_lsp_malformed_init_options_falls_back` | Malformed `initializationOptions` still `warn!`s and defaults (existing lenient behavior preserved) |
| `for_lsp_malformed_knap_toml_errors`        | Malformed `knap.toml` fails `for_lsp` outright, same as `for_path`                                  |

> **Manual checkpoint:** none yet — `config.rs` has no CLI surface until
> Step 5. Verified purely by `cargo test`.

---

## Step 3 — Rewire `server.rs`/`handlers.rs` to `config.rs`

**Deliverables:**

- `src/server/mod.rs`: delete the moved definitions; `run()` calls
  `crate::config::for_lsp(&init_params)?` in place of
  `Config::from_params(&init_params)`.
- Global rename `crate::server::Config`/`FrontmatterSchema`/`SchemaField` →
  `crate::config::*` across `src/handlers.rs` and `src/server/tests.rs`. No
  re-export/alias left behind (`AGENTS.md` Fearless Refactoring).

**Unit tests:**

No new tests this step — this is pure plumbing. `cargo test` must stay
green for every existing test in `src/server/tests.rs`, `src/handlers.rs`,
and `tests/lsp.rs` unchanged, since the LSP path's observable behavior
(other than the new-but-untriggered `knap.toml` layering) doesn't change.

> **Manual checkpoint:** `cargo run -- ` (bare, still falls through to LSP
> at this point since Step 5 hasn't landed yet) starts the server against a
> real editor exactly as before.

---

## Step 4 — `NoteIndex::report()` for `knap index --json`

**Deliverables:**

- `src/index/mod.rs` (or a `report` submodule): `IndexReport`,
  `NoteSummary`, `HeadingSummary`, `LinkSummary` (all `Serialize`),
  `NoteIndex::report(&self) -> IndexReport`, built from the existing
  `all_notes()`/`resolve()`/`links_to()`/`all_tags()`/`notes_by_tag()` — no
  new resolution logic.

**Unit tests:**

| Test                                       | What it verifies                          |
| ------------------------------------------ | ----------------------------------------- |
| `report_includes_all_notes_sorted_by_path` | Deterministic ordering                    |
| `report_link_summary_marks_broken_links`   | `resolved: None` for a broken target      |
| `report_link_summary_marks_resolved_links` | `resolved: Some(path)` for a valid target |
| `report_tags_map_groups_by_tag`            | Notes sharing a tag are grouped under it  |

> **Manual checkpoint:** none yet — no CLI surface until Step 7.

---

## Step 5 — `src/cli/` restructure + clap wiring

**Deliverables:**

- Delete `src/cli.rs`, create `src/cli/{mod,lsp,lint,index,parse,check,version}.rs`.
- `mod.rs`: clap `Cli`/`Commands` (`Lsp`, `Lint { path, json }`,
  `Index { path, json }`, `Parse { path }`, `Check`, `Version`) and
  `pub fn run() -> anyhow::Result<()>` dispatching to each submodule.
- `parse.rs`/`check.rs`/`version.rs`: today's `cmd_parse`/`cmd_check`/
  `cmd_version` bodies moved verbatim, signatures adjusted for typed clap
  args instead of `&[String]`. Behavior unchanged.
- `lsp.rs`: today's `main.rs` stdio-bootstrap moved here verbatim, now
  behind an explicit subcommand instead of the no-args fallback.
- `lint.rs`/`index.rs`: stubs for now (`todo!()` or minimal placeholder) —
  filled in Steps 6–7.
- `src/main.rs` collapses to logging setup + `knap::cli::run()`. All
  hand-rolled `args[1]` matching and the bare-args-falls-through-to-LSP
  branch are deleted outright.

**Unit tests:**

| Test                                     | What it verifies                                                      |
| ---------------------------------------- | --------------------------------------------------------------------- |
| `no_args_exits_nonzero_and_prints_usage` | (`tests/cli.rs`) Bare `knap` — non-success exit, usage text on stderr |
| `version_subcommand_prints_version`      | (`tests/cli.rs`) Existing behavior unchanged, now behind clap         |
| `parse_subcommand_still_works`           | (`tests/cli.rs`) New coverage — behavior unchanged                    |
| `check_subcommand_still_works`           | (`tests/cli.rs`) New coverage — behavior unchanged                    |

> **Manual checkpoint:** `cargo run -- lsp` starts the server against a real
> editor exactly as bare `knap` did before this step; bare `cargo run --`
> now exits non-zero with usage text instead of starting the server.

---

## Step 6 — `knap lint`

**Deliverables:**

- `src/cli/lint.rs`: `config::for_path` → `index::build` →
  `handlers::compute_diagnostics` per target file. Text output (default)
  and `--json` (wraps `lsp_types::Diagnostic`'s existing `Serialize`) per
  the shapes in `design.md`. Exit 1 if any diagnostic found, else 0.
- New `tests/fixtures/lint_basic/` (a note with a broken link and a missing
  anchor, plus its link target) and `tests/fixtures/lint_clean/`.
- New `tests/fixtures/knap_toml/` (a `knap.toml` declaring a non-default
  extension, plus a note using it) and a malformed-TOML fixture variant.

**Unit tests:**

No new unit tests in `src/` — `lint.rs` is a thin composition of already-
tested `config::for_path`, `index::build`, `handlers::compute_diagnostics`.
Coverage lives in the integration tests below.

**Integration tests (`tests/cli.rs`):**

| Test                                        | What it verifies                                                                  |
| ------------------------------------------- | --------------------------------------------------------------------------------- |
| `lint_text_output_reports_broken_link`      | `knap lint tests/fixtures/lint_basic` — expected text line, exit code 1           |
| `lint_json_output_parses_and_matches_shape` | `--json` output parses as JSON, `problem_count > 0`                               |
| `lint_clean_dir_exits_zero`                 | `tests/fixtures/lint_clean` — exit 0, empty diagnostics                           |
| `lint_respects_knap_toml_extensions`        | `tests/fixtures/knap_toml` — non-default extension only picked up via `knap.toml` |
| `lint_malformed_knap_toml_fails_loudly`     | Malformed `knap.toml` fixture — non-zero exit, parse-error message on stderr      |

> **Manual checkpoint:** run `cargo run -- lint .` against this repo's own
> `docs/` tree (or a scratch vault) and confirm the output matches what the
> LSP would show as diagnostics for the same files.

---

## Step 7 — `knap index` rewrite

**Deliverables:**

- `src/cli/index.rs`: same `config::for_path` loading as `lint` (this is
  the fix for the hardcoded `extensions: &["md"]` bug). Text output
  unchanged from today's format. `--json` serializes `NoteIndex::report()`.
- New `tests/fixtures/index_basic/` (a couple of linked, tagged notes).

**Unit tests:**

Covered by Step 4's `NoteIndex::report()` tests — no new `src/` unit tests
here.

**Integration tests (`tests/cli.rs`):**

| Test                                 | What it verifies                                                                                                                          |
| ------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `index_json_output_shape`            | `knap index --json tests/fixtures/index_basic` — `notes[].headings`, `notes[].links[].resolved`, top-level `tags` all present and correct |
| `index_text_output_unchanged_format` | Human-readable format matches today's shape                                                                                               |

> **Manual checkpoint:** run `cargo run -- index tests/fixtures/knap_toml --json`
> and confirm the non-default extension from that fixture's `knap.toml`
> shows up in the note list — proof the divergent-config bug is fixed.

---

## Step 8 — Full test suite + clippy

End-to-end verification across the whole crate.

**Deliverables:**

- `cargo test` passes, `cargo clippy -- -D warnings` clean.

| Test                        | What it verifies                                        |
| --------------------------- | ------------------------------------------------------- |
| (all tests above, together) | No regressions from the config/CLI restructure combined |

> **Manual checkpoint (full session):** open an editor on a real vault via
> `knap lsp`, confirm completions/diagnostics/rename/etc. behave exactly as
> before this release. Then run `knap lint` and `knap index --json` against
> the same vault from a terminal and confirm the diagnostics match what the
> editor shows.

---

## Step 9 — Docs

**Deliverables:**

- `docs/ARCHITECTURE.md`: rewrite "Configuration" (no longer
  `initializationOptions`-only) and "Debug CLI" (add `lsp`/`lint`, note
  bare `knap` now requires a subcommand) sections.
- `docs/ROADMAP.md`: new v0.13 entry; Backlog bullet for a future
  `--fail-on <severity>` threshold on `knap lint`.
- `README.md`: document `knap lsp`/`knap lint`/`knap index --json` usage
  and `knap.toml` as a config source.
- `CHANGELOG.md`: new `[0.13.0]` entry, breaking-change callout up top.

---

## Step 10 — Release coordination

**Deliverables:**

- Confirm `zed-knap` and `vscode-knap` have (or are ready to ship) their
  server-launch command updated from bare `knap` to `knap lsp` — **do not
  tag/push this release until that's coordinated**, since it breaks both
  extensions on upgrade otherwise.
- `cargo run -- version` reports `0.13.0` after the version bump.
- Release via `/knap-release`.

---

## Done — v0.13 complete

| Story  | Feature                                                         | Delivered in step |
| ------ | --------------------------------------------------------------- | ----------------- |
| US-D07 | `knap.toml` project config, shared `Config` loader              | Step 2, 3         |
| US-D06 | `knap lsp` explicit subcommand; bare `knap` no longer starts it | Step 5            |
| US-D04 | `knap lint [path] [--json]`                                     | Step 6            |
| US-D05 | `knap index <path> --json`                                      | Step 4, 7         |
