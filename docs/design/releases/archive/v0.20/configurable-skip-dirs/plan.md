# v0.20 Implementation Plan — Configurable Skip-Dir Defaults

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                             | Status | Notes |
| ------------------------------------------------- | ------ | ----- |
| 1 — `PathFilter::skip_dirs`, `default_skip_dirs`   | Done   | `PathFilter` is now data-driven; `is_skip_dir_name` removed |
| 2 — `Config`/`RawConfig` plumbing                  | Done   | `skip_dirs` wired through `InitOptions`/`KnapToml`/`RawConfig`/`merge`/`finalize`, override precedence |
| 3 — Docs                                           | Done   | README.md, docs/ARCHITECTURE.md, docs/USER_STORIES.md updated |
| 4 — Integration tests                              | Done   | `tests/skip_dirs.rs` added; `knap lint`/`knap index` exercised end to end |

---

## Step 1 — `PathFilter::skip_dirs`, `default_skip_dirs`

Makes `PathFilter` itself data-driven for skip-dir names, with no config
plumbing yet — every call site keeps working via the new hand-written
`Default` impl and updated `compile` call sites, so this step is a pure
refactor: same runtime behaviour, verified by tests before any new
user-facing option exists.

Uses TDD:

1. Write all unit tests for this step first — `PathFilter::compile` won't
   yet accept a third argument, so stub the new signature (accept and
   ignore `skip_dirs: &[String]` if needed to compile) before writing
   assertions against it.
2. Run `cargo test` and confirm the new tests **fail** (or don't compile)
   against the old `is_skip_dir_name` free function.
3. Implement `skip_dirs: Vec<glob::Pattern>` on `PathFilter`,
   `matches_skip_dir`, `default_skip_dirs()`, and the hand-written `Default`
   impl until tests pass, then run `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/config/mod.rs`: `PathFilter.skip_dirs: Vec<glob::Pattern>` field
- `src/config/mod.rs`: `PathFilter::compile(exclude, extensions, skip_dirs)` — three-argument signature, skip_dirs compiled the same way `exclude` is
- `src/config/mod.rs`: `PathFilter::matches_skip_dir(&self, name: &str) -> bool`, replacing the deleted `is_skip_dir_name` free function; `should_skip_dir` and `should_index` call it instead
- `src/config/mod.rs`: `pub(crate) fn default_skip_dirs() -> Vec<String>` returning `vec![".*", "node_modules", "target"]`
- `src/config/mod.rs`: hand-written `impl Default for PathFilter` calling `compile(&[], &[], &default_skip_dirs())`
- Every existing `PathFilter::compile(...)` call site (`src/config/mod.rs`'s `finalize`, `src/config/tests.rs`, `src/index/tests.rs`'s `filter()` helper) updated to pass a third argument — `default_skip_dirs()` where the test wants today's default behaviour unchanged, `&[]` where the test is isolating `exclude`-only behaviour

**Unit tests:**

| Test                                                     | What it verifies                                                                                              |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `path_filter_default_skip_dirs_matches_hardcoded_names`    | `PathFilter::default()` and a filter compiled with `default_skip_dirs()` both skip `.git`, `.obsidian`, `node_modules`, `target` |
| `path_filter_skip_dirs_empty_disables_pruning`              | A filter compiled with `skip_dirs: &[]` does not skip `.git`                                                     |
| `path_filter_skip_dirs_custom_pattern`                      | A filter compiled with `skip_dirs: &["vendor".into()]` skips `vendor` but not `node_modules`                     |
| `path_filter_skip_dirs_malformed_pattern_errors`            | An invalid glob in `skip_dirs` returns `Err` from `compile`                                                       |
| `path_filter_should_skip_dir_true_for_exclude_match` *(existing, updated)* | Still passes with `skip_dirs: &[]`, isolating the `exclude`-pattern path from skip-dir defaults      |

> **Manual checkpoint:** No editor checkpoint — this step is an internal
> refactor of `PathFilter` with no new config surface yet. Covered entirely
> by unit tests.

---

## Step 2 — `Config`/`RawConfig` plumbing

Wires `skip_dirs` through `InitOptions`, `KnapToml`, `RawConfig`, `merge`,
and `finalize`, so `knap.toml` and `initializationOptions` can actually set
it. Builds directly on Step 1's `PathFilter::compile` three-arg signature
and `default_skip_dirs()`.

Uses TDD:

1. Write all unit tests for this step first against the new `Config`/
   `RawConfig` fields (stub the fields so it compiles).
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement `InitOptions.skip_dirs`, `KnapToml.skip_dirs`,
   `RawConfig.skip_dirs`, `merge()`'s `skip_dirs` line, and `finalize()`'s
   default-when-unset call to `default_skip_dirs()` until tests pass, then
   run `cargo clippy -- -D warnings`.

**Deliverables:**

- `src/config/mod.rs`: `InitOptions.skip_dirs: Option<Vec<String>>` (camelCase `skipDirs` on the wire)
- `src/config/mod.rs`: `KnapToml.skip_dirs: Option<Vec<String>>` (snake_case `skip_dirs` in TOML)
- `src/config/mod.rs`: `RawConfig.skip_dirs: Option<Vec<String>>`, plus the matching lines in both `From<InitOptions>` and `From<KnapToml>` impls
- `src/config/mod.rs`: `merge()` gains `skip_dirs: primary.skip_dirs.or(fallback.skip_dirs)` — override precedence, not `exclude`'s union
- `src/config/mod.rs`: `finalize()` resolves `let skip_dirs = raw.skip_dirs.unwrap_or_else(default_skip_dirs);` and passes it to `PathFilter::compile`
- `src/config/mod.rs`: `Config.skip_dirs: Vec<String>` raw field (`#[allow(dead_code)]`, same posture as `extensions`/`exclude` — kept for tests, `path_filter` is the authority)

**Unit tests:**

| Test                                                    | What it verifies                                                                                        |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `for_path_absent_knap_toml_skip_dirs_defaults_to_builtin`  | `config.skip_dirs` equals `default_skip_dirs()` when `knap.toml` doesn't set it                                |
| `for_path_loads_knap_toml_skip_dirs`                       | `skip_dirs = ["vendor"]` in `knap.toml` appears verbatim in `config.skip_dirs` — not unioned with the default   |
| `for_lsp_skip_dirs_init_options_overrides_knap_toml`        | `initializationOptions.skipDirs` fully replaces `knap.toml`'s `skip_dirs` when both are set                    |
| `for_lsp_skip_dirs_default_when_unset`                      | `config.skip_dirs` equals `default_skip_dirs()` when neither source sets it                                    |

> **Manual checkpoint:** No editor checkpoint yet — `Config` now carries
> `skip_dirs` end to end, but nothing in the editor or CLI surfaces it
> visibly beyond what Step 4's integration tests exercise headlessly.

---

## Step 3 — Docs

Brings `README.md`, `docs/ARCHITECTURE.md`, and `docs/USER_STORIES.md` in
sync with the shipped behaviour, following this project's release convention
of docs matching code before a release closes.

**Deliverables:**

- `README.md`: `knap.toml` reference example gains a `skip_dirs` line next to `exclude`, with a comment noting it's matched against bare directory names (not paths) and replaces rather than unions with the built-in default
- `README.md`: the `initializationOptions` layering paragraph notes `skip_dirs` follows the same override precedence as `extensions`, distinct from `exclude`'s union
- `docs/ARCHITECTURE.md`: Configuration section's `PathFilter` paragraph replaces "resolves the hardcoded `.git`/`node_modules`/`target` skip-list" with a description of `skip_dirs` as a compiled, `knap.toml`-configurable field defaulting to `default_skip_dirs()`
- `docs/USER_STORIES.md`: US-58 added alongside US-55

No unit tests — doc-only step, verified by review.

> **Manual checkpoint:** Open `README.md`'s `knap.toml` section and confirm
> the `skip_dirs` example reads consistently with the `exclude` example
> immediately above it — same comment style, same "relative to what" clarity.

---

## Step 4 — Integration tests

End-to-end tests over `knap lint`/`knap index`, confirming `skip_dirs`
behaves correctly through the full config-load-to-crawl path, not just at
the `PathFilter`/`Config` unit level. Always the last step.

**Deliverables:**

- `tests/skip_dirs.rs` with all integration tests below
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                             | What it verifies                                                                                                                       |
| --------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `index_json_indexes_dotted_dir_when_opted_out`      | `knap index --json` on a vault with `knap.toml` `skip_dirs = []` lists notes under a `.notes/` directory that would otherwise be pruned         |
| `lint_default_skip_dirs_prunes_node_modules`        | `knap lint` on a vault with `node_modules/broken.md`, no `knap.toml`, reports no diagnostics from it (built-in default applies)                |
| `index_custom_skip_dirs_replaces_default`           | `knap.toml` `skip_dirs = ["vendor"]` on a vault with both `vendor/` and `node_modules/` prunes only `vendor/`; `node_modules/` is indexed        |

> **Manual checkpoint (full session):** Open a scratch vault in the editor
> containing a `.notes/` directory with a real markdown file inside it, and a
> `knap.toml` with `skip_dirs = []`. Confirm the file inside `.notes/`
> appears in path completions and Go to Definition resolves to it — then
> remove `skip_dirs` from `knap.toml`, restart the server, and confirm it
> disappears from completions again. Confirm `.git`/`node_modules`/`target`
> in the workspace are still pruned by default with no `knap.toml` present at
> all, matching pre-v0.20 behaviour.

---

## Done — v0.20 complete

| Story | Feature                                                                | Delivered in step |
| ----- | -------------------------------------------------------------------------- | ------------------ |
| US-58 | `knap.toml` `skip_dirs` — configurable, overridable crawl-prune defaults   | Step 2 (Step 4 verifies end to end) — Done |
