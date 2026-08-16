# v0.16 Implementation Plan — Path Filter Authority

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                             | Status | Notes |
| ------------------------------------------------ | ------ | ----- |
| 1 — Regression test proving the bug              | Done   |       |
| 2 — `PathFilter` type                            | Done   |       |
| 3 — Wire `PathFilter` through `Config`           | Todo   |       |
| 4 — Wire `PathFilter` through `index::build`     | Todo   |       |
| 5 — Wire `PathFilter` through the three handlers | Todo   |       |
| 6 — `apply.rs` and doc cleanup                   | Todo   |       |
| 7 — Integration tests                            | Todo   |       |

---

## Step 1 — Regression test proving the bug

Write the failing test first, on the unfixed code, so the rest of the plan is
provably fixing the reported bug and not just refactoring around it.

**Deliverables:**

- Add `lsp_did_change_watched_files_on_excluded_path_is_ignored` to
  `tests/exclude.rs`: `knap.toml` with `exclude = ["fixtures/**"]`, initialize
  the server, then send a `workspace/didChangeWatchedFiles` `Created` event
  for `fixtures/broken.md` (constructed the same way `tests/lsp.rs`'s existing
  watched-files tests do — see `tests/lsp.rs:379`). Assert no diagnostics for
  `broken.md` appear afterward.
- Run `cargo test lsp_did_change_watched_files_on_excluded_path_is_ignored`
  and confirm it **fails** against the current `main` — this is the
  reproduction from issue #68's step (b). Do not proceed to Step 2 until it's
  confirmed failing for the right reason (diagnostics for `broken.md` do
  appear).

**Unit tests:**

None — this step only adds the integration-level regression test.

> **Manual checkpoint:** No editor checkpoint — the failing `cargo test` run
> itself is the checkpoint; screenshot or paste its output before moving on.

---

## Step 2 — `PathFilter` type

Build the new authority in isolation, fully unit-tested, before anything
calls it. Uses TDD: write the tests against the type's intended API first.

1. Write all unit tests for this step first, against a `PathFilter` with
   only its signatures stubbed in (`compile`, `should_skip_dir`,
   `should_index`, `is_note` — bodies can `todo!()` or return a placeholder
   that fails the test).
2. Run `cargo test` and confirm the new tests **fail** (or don't compile,
   which is fine for `todo!()` bodies — comment them in one at a time if
   needed to keep the crate compiling).
3. Implement `PathFilter` in `src/config/mod.rs` per the design doc until all
   tests pass, then run `cargo clippy -- -D warnings`.

**Deliverables:**

- `pub(crate) struct PathFilter { excludes: Vec<glob::Pattern>, extensions: Vec<String> }` in `src/config/mod.rs`
- `PathFilter::compile(exclude: &[String], extensions: &[String]) -> anyhow::Result<Self>` — moves the pattern-compiling logic (including the `/**`-suffix directory-equivalent form) out of `index::build`
- `PathFilter::is_skip_dir_name(name: &str) -> bool` (private) — moved from `index::should_skip_dir`
- `PathFilter::matches_exclude(&self, relative: &Path) -> bool` (private)
- `PathFilter::should_skip_dir(&self, root: &Path, dir_path: &Path, dir_name: &str) -> bool`
- `PathFilter::should_index(&self, root: &Path, path: &Path) -> bool`
- `PathFilter::is_note(&self, path: &Path) -> bool`
- `#[derive(Default)]` on `PathFilter` (empty excludes, empty extensions) so `Config`'s existing `#[derive(Default)]` and the `..Default::default()` test-literal pattern in `handlers.rs` keep compiling once `Config` gains the field in Step 3

**Unit tests:**

| Test                                                      | What it verifies                                                                            |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `path_filter_should_index_true_for_plain_file`            | A path with no excluded ancestor and no exclude-glob match returns `true`                   |
| `path_filter_should_index_false_for_excluded_glob_match`  | A path matching an `exclude` pattern returns `false`                                        |
| `path_filter_should_index_false_under_hardcoded_skip_dir` | A path under `.git/`, `node_modules/`, or `target/` returns `false` regardless of `exclude` |
| `path_filter_should_index_true_for_leaf_dotfile`          | A dotfile leaf (`.hidden.md`) directly under an included root returns `true`                |
| `path_filter_should_skip_dir_true_for_hardcoded_name`     | `should_skip_dir` prunes `.git`/`node_modules`/`target` regardless of `exclude`             |
| `path_filter_should_skip_dir_true_for_exclude_match`      | `should_skip_dir` prunes a directory matching an `exclude` pattern                          |
| `path_filter_is_note_true_for_configured_extension`       | `is_note` returns `true` for a path whose extension is in `extensions`                      |
| `path_filter_is_note_false_for_other_extension`           | `is_note` returns `false` for an extension not in `extensions`                              |
| `path_filter_compile_dir_form_from_glob_star_star_suffix` | `compile` adds the `/**`-stripped directory-equivalent pattern                              |
| `path_filter_compile_rejects_malformed_pattern`           | `compile` returns `Err` for an invalid glob pattern                                         |

> **Manual checkpoint:** No editor checkpoint — `PathFilter` isn't wired into
> anything yet; covered entirely by unit tests.

---

## Step 3 — Wire `PathFilter` through `Config`

**Deliverables:**

- Add `pub(crate) path_filter: PathFilter` to `Config` (`src/config/mod.rs`), alongside the existing `exclude: Vec<String>` (unchanged, still raw)
- Change `finalize` to `fn finalize(raw: RawConfig, index_roots: Vec<PathBuf>) -> anyhow::Result<Config>`, calling `PathFilter::compile(&exclude, &extensions)?` and setting the new field
- Update `for_lsp` and `for_path` to `finalize(raw, index_roots)` (drop the `Ok(...)` wrapper — `finalize` now returns the `Result` directly)
- Add `Config::should_index(&self, path: &Path) -> bool` (longest-prefix `index_roots` match → `self.path_filter.should_index`, `true` if `path` isn't under any root)
- Add `Config::is_note(&self, path: &Path) -> bool` (delegates to `self.path_filter.is_note`)

**Unit tests:**

| Test                                                       | What it verifies                                                                                       |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `path_filter_should_index_true_for_path_outside_all_roots` | `Config::should_index` returns `true` for a path that doesn't start with any `index_roots` entry       |
| `config_finalize_propagates_path_filter_compile_error`     | `for_path` (and by extension `for_lsp`) surfaces a malformed `exclude` pattern as `Err`, not a default |

Existing `config/tests.rs` merge tests (`config.exclude` assertions) are
untouched — `exclude` stays the raw field they inspect.

> **Manual checkpoint:** No editor checkpoint — `Config::should_index`/`is_note` have no caller yet; covered by unit tests. `cargo build` must succeed with the new fallible `finalize` signature threaded through both callers.

---

## Step 4 — Wire `PathFilter` through `index::build`

**Deliverables:**

- Change `index::build` signature to `pub fn build(roots: &[PathBuf], filter: &PathFilter) -> anyhow::Result<(NoteIndex, IndexDelta)>` (`src/index/mod.rs`)
- Update `walk_files`/`walk_dir` to take `&PathFilter` instead of `&[glob::Pattern]`; directory branch calls `filter.should_skip_dir(root, &entry_path, &name)`, file branch calls `filter.should_index(root, &entry_path)`
- Delete `index::should_skip_dir` (moved into `PathFilter::is_skip_dir_name` in Step 2) and the local glob-compiling block at the top of `build`
- Replace `build`'s locally-computed `is_note` with `filter.is_note(&path)`
- Update every call site to pass `&config.path_filter` instead of `&exts, &config.exclude`: `src/server/mod.rs:121`, `src/cli/index.rs:23`, `src/cli/rename.rs:88,129,163`, `src/cli/lint.rs:58,66,77`, `src/cli/fix.rs:63`

**Unit tests:**

Existing `src/index/tests.rs` tests that call `build(roots, extensions, exclude)` are updated to call `build(roots, &filter)` with a `PathFilter::compile(...)` built inline — no new test cases in this step (the behaviour they cover is unchanged; only the call shape moves). Confirm `cargo test index::` still passes with identical assertions.

> **Manual checkpoint:** No editor checkpoint yet — `knap index <vault-with-exclude>` and `knap lint <vault-with-exclude>` from a terminal should produce identical output to before this step (same notes, same diagnostics). Spot-check one vault with an `exclude` pattern.

---

## Step 5 — Wire `PathFilter` through the three handlers

1. Write the unit-level guard tests first (see table below), against
   `on_did_open`/`on_did_change`/`on_did_change_watched_files` as they exist
   today (no `should_index` check yet) — confirm they **fail**.
2. Add the `if !config.should_index(&path) { return/continue; }` guard to
   each handler per the design doc.
3. Delete `should_skip_path` (`src/server/mod.rs:395`) and its use in
   `on_did_change_watched_files`, replaced by `config.should_index`. Replace
   the ad hoc extension check in `on_did_change_watched_files` with
   `config.is_note(&path)`.
4. Run `cargo test` and confirm this step's tests pass, then
   `cargo clippy -- -D warnings`.
5. Re-run Step 1's `lsp_did_change_watched_files_on_excluded_path_is_ignored`
   and confirm it now **passes**.

**Deliverables:**

- `src/server/mod.rs`: guard in `on_did_open`, guard in `on_did_change`, `should_skip_path` deleted, `on_did_change_watched_files` updated to use `config.should_index`/`config.is_note`

**Unit tests:**

This step's coverage is the Step 1 regression test plus the three new
integration tests in Step 7 (`on_did_open`/`on_did_change` don't have
existing unit-level harnesses separate from the LSP message loop — see
`tests/lsp.rs`'s pattern of driving handlers via `spawn_server()` — so their
guard behaviour is verified at the integration level, not in isolation).

> **Manual checkpoint:** In an editor (e.g. Zed) with `knap.toml` containing
> `exclude = ["fixtures/**"]`, open `fixtures/broken.md` directly. Confirm no
> diagnostics appear for it and it doesn't show up in workspace symbol
> search — where before this step it would.

---

## Step 6 — `apply.rs` and doc cleanup

**Deliverables:**

- `src/cli/apply.rs`'s two `index::should_skip_dir` call sites (`:84`, `:105`) switch to the `Config`/`PathFilter` equivalent available at those call sites — confirm at implementation time (per the design doc's open question) whether `should_index`/`should_skip_dir` parity with the rest of the codebase is correct there, and adjust `apply.rs`'s existing tests if the switch changes which stub-note candidates are found
- Update `docs/design/components/protocol-handler.md`'s three notification-handler prose blocks (`textDocument/didOpen`, `textDocument/didChange`, `workspace/didChangeWatchedFiles`) to show the new `should_index` guard
- Update `tests/exclude.rs:260-269`'s doc comment on `lsp_initialize_applies_knap_toml_exclude`, which currently asserts "no handler special-cases excluded paths" — no longer true after Step 5
- Add a `docs/ROADMAP.md` v0.16 entry (or amend the existing one) noting the path-filter-authority fix alongside `US-55`, referencing this design doc

**Unit tests:**

None — this step is call-site and doc cleanup; `apply.rs`'s existing test
suite (if its behaviour changes) is the coverage.

> **Manual checkpoint:** `knap apply --suggest-stub <broken-link>` (or
> equivalent) on a vault with an `exclude`d directory containing a
> plausible stub target — confirm the excluded directory's files are (or
> are deliberately not, per the Step 6 decision) offered as candidates.

---

## Step 7 — Integration tests

End-to-end tests over the full LSP message loop. Always the last step.

**Deliverables:**

- `tests/exclude.rs` gains `lsp_did_open_on_excluded_file_is_not_indexed`, `lsp_did_change_on_excluded_file_is_not_indexed`, `lsp_did_change_watched_files_admits_non_excluded_sibling` (Step 1's `lsp_did_change_watched_files_on_excluded_path_is_ignored` already lands here, written in Step 1)
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                                       | What it verifies                                                                                         |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `lsp_did_open_on_excluded_file_is_not_indexed`             | Sending `didOpen` directly for a path under `exclude` never publishes diagnostics for it                 |
| `lsp_did_change_on_excluded_file_is_not_indexed`           | Sending `didChange` directly for a path under `exclude` has no indexing effect                           |
| `lsp_did_change_watched_files_on_excluded_path_is_ignored` | A `didChangeWatchedFiles` `Created`/`Changed` event for an excluded path never indexes the file (Step 1) |
| `lsp_did_change_watched_files_admits_non_excluded_sibling` | A watched-file event for a non-excluded path still indexes normally — the fix doesn't over-exclude       |

> **Manual checkpoint (full session):** Open a vault with `knap.toml`
> containing `exclude = ["fixtures/**"]` in a real editor. Open a file under
> `fixtures/` directly — confirm no diagnostics, no workspace-symbol entry,
> no Go to Definition target. Run `git checkout` (or touch a file) under
> `fixtures/` while the session is open — confirm it stays invisible. Then
> edit a real, non-excluded note and confirm diagnostics, completions, and
> navigation all still work normally there — v0.16's original `exclude`
> behaviour (US-55) is unaffected by this fix.

---

## Done — v0.16 path-filter-authority complete

| Story   | Feature                                                                         | Delivered in step |
| ------- | ------------------------------------------------------------------------------- | ----------------- |
| Bug #68 | `PathFilter` authority consulted by the crawl and all three live-index handlers | Step 7            |
