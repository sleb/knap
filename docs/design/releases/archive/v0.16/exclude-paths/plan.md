# v0.16 Implementation Plan — Exclude Paths

Describes the order in which changes are made, what is tested after each
step, and the checkpoints where the server should be manually verified
against a real editor.

The guiding principle: each step produces something testable. No step lays
down untested code for the next step to build on.

---

## Status

| Step                                    | Status | Notes |
| --------------------------------------- | ------ | ----- |
| 1 — `exclude` in `Config`               | Done   |       |
| 2 — `index::build` honors `exclude`     | Done   |       |
| 3 — Wire `exclude` through every caller | Done   |       |
| 4 — `--exclude` CLI flag                | Done   |       |
| 5 — Integration tests                   | Done   |       |

---

## Step 1 — `exclude` in `Config`

Add the `exclude` field end-to-end through the config-loading layer, with no
behavioral effect yet (nothing reads it downstream). This is safe to land
alone because `Config.exclude` starts as an inert, fully-tested field.

Uses TDD:

1. Write all unit tests for this step first — stub `exclude: Vec<String>` on
   every struct so the file compiles, and give `for_path` its new
   `exclude_additions: &[String]` parameter (update existing call sites to
   pass `&[]`) before writing any merge logic.
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement `merge()`'s union behavior and `for_path`'s append behavior
   until tests pass, then run `cargo clippy -- -D warnings`.

**Deliverables:**

- `exclude: Vec<String>` added to `InitOptions`, `KnapToml`, `RawConfig`,
  `Config` in `src/config/mod.rs`
- `RawConfig::from(InitOptions)` / `RawConfig::from(KnapToml)` populate the
  new field
- `merge()` unions `primary.exclude` and `fallback.exclude` (concatenate,
  no dedup needed — duplicate patterns are harmless, just redundant checks)
- `finalize()` defaults `exclude` to `vec![]`
- `for_path(path, extensions_override, exclude_additions: &[String])` —
  appends `exclude_additions` to the loaded `knap.toml` list before
  `finalize`
- Update `for_path`'s existing callers (`src/cli/lint.rs`,
  `src/cli/index.rs`, `src/cli/rename.rs`, `src/cli/fix.rs`) to pass `&[]`
  for now — no CLI flag yet, that's Step 4

**Unit tests (`src/config/tests.rs`):**

| Test                                                | What it verifies                                                                            |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `for_path_absent_knap_toml_exclude_defaults_empty`  | `config.exclude` is `[]` when `knap.toml` doesn't set it and no `--exclude` is passed       |
| `for_path_loads_knap_toml_exclude`                  | `exclude = ["a/**"]` in `knap.toml` appears in `config.exclude`                             |
| `for_path_exclude_additions_appended`               | `exclude_additions` passed to `for_path` are appended to, not replacing, `knap.toml`'s list |
| `for_lsp_exclude_unions_knap_toml_and_init_options` | `initializationOptions.exclude` and `knap.toml`'s `exclude` both appear in the result       |
| `for_lsp_exclude_default_empty`                     | `config.exclude` is `[]` when neither source sets it                                        |

> **Manual checkpoint:** No editor checkpoint — `Config.exclude` has no
> reader yet, covered entirely by unit tests.

---

## Step 2 — `index::build` honors `exclude`

Give `walk_dir`/`build` the actual exclusion behavior. This is the step
that changes what ends up in the index.

Uses TDD:

1. Write all unit tests for this step first, against the new
   `build(roots, extensions, exclude) -> Result<(NoteIndex, IndexDelta)>`
   signature (note the `Result` — a malformed pattern now errors).
2. Run `cargo test` and confirm the new tests **fail** (won't compile until
   the signature change lands — stub `build` to return
   `Ok(Default::default())` first if needed to get a red run).
3. Implement pattern compilation and the `walk_dir` exclusion check until
   tests pass, then run `cargo clippy -- -D warnings`.

**Deliverables:**

- Add `glob = "0.3"` to `Cargo.toml`
- `index::build(roots: &[PathBuf], extensions: &[&str], exclude: &[String]) -> Result<(NoteIndex, IndexDelta)>` in `src/index/mod.rs` — compiles `exclude` into `Vec<glob::Pattern>` via `glob::Pattern::new` once, propagating a parse error as `Err`
- `walk_dir(dir: &Path, root: &Path, excludes: &[glob::Pattern], out: &mut Vec<PathBuf>)` gains the `root`/`excludes` parameters; before recursing into a subdirectory or pushing a file, computes the entry's path relative to `root` and skips it (directory: don't recurse; file: don't push) if any pattern matches
- `walk_files(root, excludes)` passes `root` through as both the walk's starting point and the relative-path base

**Unit tests (`src/index/tests.rs`):**

| Test                                               | What it verifies                                                                                    |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `build_excludes_directory_by_exact_path`           | A file under a directory matching an exact-path pattern (`tests/fixtures`) is absent from the index |
| `build_excludes_directory_by_glob`                 | `tests/fixtures/**` excludes the same subtree as the exact-path form                                |
| `build_excludes_file_by_glob`                      | `**/*.draft.md` excludes matching files while leaving sibling files indexed                         |
| `build_excluded_file_not_registered_as_attachment` | A non-note file matching an exclude pattern is absent from `all_files`, not just unparsed           |
| `build_no_excludes_is_unchanged`                   | Empty `exclude` slice produces the same index as before this step                                   |
| `build_malformed_pattern_errors`                   | An invalid glob pattern returns `Err` instead of panicking                                          |

> **Manual checkpoint:** No editor checkpoint yet — `build`'s new
> `Result`-returning signature isn't wired into `knap lsp`/`knap
lint`/`knap index` until Step 3.

---

## Step 3 — Wire `exclude` through every caller

Update every `index::build` call site to pass `&config.exclude` and handle
the new `Result`. This is where excluded fixtures actually stop showing up
in `knap lint`, `knap index`, and the editor.

Not TDD — this step is pure call-site plumbing with no new logic of its
own; correctness is covered by Step 2's unit tests (the behavior) and Step
5's integration tests (the wiring).

**Deliverables:**

- `src/server/mod.rs:121` — pass `&config.exclude`; propagate the `Result`
  through `initialize`'s existing error path (same treatment as a malformed
  `knap.toml` today — fails the session outright)
- `src/cli/lint.rs` (both `index::build` calls, including the `--fix`
  rebuild) — pass `&config.exclude` / `&fix_config.exclude`
- `src/cli/index.rs` — pass `&config.exclude`
- `src/cli/rename.rs` (all three call sites) — pass `&config.exclude`
- `src/cli/fix.rs` — pass `&config.exclude`
- Each CLI call site maps the new `Result` to the same
  `anyhow::Result`-propagating `?` these functions already use

> **Manual checkpoint:** Add `exclude = ["fixtures/**"]` to this repo's own
> `knap.toml` (temporarily), open a file under `tests/fixtures/` in Zed,
> confirm the Problems panel shows nothing from it, then revert the config
> change.

---

## Step 4 — `--exclude` CLI flag

Add the flag to `knap lint` and `knap index`, feeding `for_path`'s
`exclude_additions` parameter from Step 1.

Not TDD — this is `clap` declaration plus threading an already-tested value
(`for_path`'s union behavior was covered in Step 1); the observable
behavior is covered by Step 5's integration tests.

**Deliverables:**

- `Commands::Lint` gains `#[arg(long)] exclude: Vec<String>` in
  `src/cli/mod.rs`
- `Commands::Index` gains the same `exclude: Vec<String>` field
- `cli::lint::run`/`cli::index::run` signatures gain an `exclude: &[String]`
  parameter, passed to `config::for_path(path, None, exclude)`
- `run()`'s dispatch in `src/cli/mod.rs` passes the parsed `exclude` through
  for both subcommands

> **Manual checkpoint:** From a shell in this repo, run
> `knap lint tests/fixtures --exclude '**'` and confirm it reports zero
> diagnostics (everything under the target excluded); run plain
> `knap lint tests/fixtures` and confirm the pre-existing fixture
> diagnostics are back — proves the flag is additive, not a permanent
> config change.

---

## Step 5 — Integration tests

End-to-end tests over the full LSP message loop and CLI process boundary.
Always the last step.

**Deliverables:**

- `tests/exclude.rs` with all integration tests below
- `cargo test` passes, `cargo clippy -- -D warnings` clean
- `README.md`'s `knap.toml` reference block gains an `exclude` example line
- `docs/ARCHITECTURE.md`'s `Config` shape and CLI subcommand table updated
  with `exclude` / `--exclude`

| Test                                       | What it verifies                                                                                                                                      |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lint_excludes_configured_directory`       | `knap lint` on a vault with `knap.toml` `exclude = ["fixtures/**"]` reports no diagnostics from `fixtures/`                                           |
| `lint_exclude_flag_adds_to_config`         | `knap lint --exclude other/** path` skips both the flag's pattern and `knap.toml`'s existing ones                                                     |
| `index_json_omits_excluded_notes`          | `knap index --json` on a vault with excludes doesn't list excluded files under `notes`                                                                |
| `lsp_initialize_applies_knap_toml_exclude` | An in-process LSP session started against a vault with `knap.toml` excludes never publishes diagnostics for the excluded file, even after it's edited |

> **Manual checkpoint (full session):** Open this repo in Zed with a
> temporary `knap.toml` `exclude = ["tests/fixtures/**"]`. Confirm the
> Problems panel is clean of fixture noise, Go to Definition and Workspace
> Symbols never surface a fixture file, and editing a real note's links
> still produces diagnostics normally. Remove the temporary config and
> confirm fixture diagnostics return.

---

## Done — v0.16 complete

| Story | Feature                                                            | Delivered in step |
| ----- | ------------------------------------------------------------------ | ----------------- |
| US-55 | `knap.toml` `exclude` glob patterns; `--exclude` on `lint`/`index` | Steps 1–5         |
