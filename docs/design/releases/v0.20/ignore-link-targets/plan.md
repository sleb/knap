# v0.20 Ignore Link Targets — Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                                    | Status | Notes |
| ---------------------------------------------------------- | ------ | ----- |
| 1 — Frontmatter `ignore-link-targets` extraction             | Todo   |       |
| 2 — `knap.toml`/`initializationOptions`/CLI config field      | Todo   |       |
| 3 — `compute_diagnostics` suppression                        | Todo   |       |
| 4 — Integration tests                                        | Todo   |       |

---

## Step 1 — Frontmatter `ignore-link-targets` extraction

Data model first: `Frontmatter` gains the field the rest of the feature reads
from. This is independently testable against `parser::parse` alone, with no
dependency on config or handler changes.

This step uses TDD:

1. Write all unit tests for `extract_ignore_link_targets` first, plus
   `parse_populates_frontmatter_ignore_link_targets` — stub
   `extract_ignore_link_targets` to return `vec![]` unconditionally so the
   crate compiles.
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement `extract_ignore_link_targets` (mirroring `extract_tags`'s
   three-form scan, minus ranges) until tests pass, then run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `Frontmatter.ignore_link_targets: Vec<String>` field in
  `src/parser/mod.rs`
- `fn extract_ignore_link_targets(content: &str) -> Vec<String>` in
  `src/parser/mod.rs`
- `parse()` sets `fm.ignore_link_targets = extract_ignore_link_targets(content)`
  alongside the existing `fm.tags` assignment

**Unit tests (`src/parser/tests.rs`):**

| Test                                                     | What it verifies                                                            |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------ |
| `extract_ignore_link_targets_bare_scalar`                   | `ignore-link-targets: ../a/**` → `vec!["../a/**"]`                          |
| `extract_ignore_link_targets_inline_list`                   | `ignore-link-targets: [../a/**, ../b.md]` → both entries, in order          |
| `extract_ignore_link_targets_block_list`                     | `ignore-link-targets:\n  - ../a/**\n  - ../b.md` → both entries, in order   |
| `extract_ignore_link_targets_absent_key_returns_empty`       | Frontmatter block with no `ignore-link-targets:` key → `vec![]`             |
| `extract_ignore_link_targets_no_frontmatter_returns_empty`   | No `---` block at all → `vec![]`                                            |
| `extract_ignore_link_targets_block_scalar_ignored`          | `ignore-link-targets: \|\n  ...` → `vec![]`, same as `tags:`                |
| `extract_ignore_link_targets_duplicate_entries_both_kept`    | `ignore-link-targets: [a, a]` → `vec!["a", "a"]`, no dedup in the parser     |
| `parse_populates_frontmatter_ignore_link_targets`           | `parser::parse` on a note with `ignore-link-targets:` → `note.frontmatter.ignore_link_targets` populated |

> **Manual checkpoint:** No editor checkpoint — `Frontmatter.ignore_link_targets`
> isn't read by any handler yet. Covered by unit tests only.

---

## Step 2 — `knap.toml`/`initializationOptions`/CLI config field

Adds the workspace-wide and one-off halves of the feature, mirroring
`exclude`'s shape, union-merge precedence, and `--exclude` flag. Independently
testable against `config::for_path`/`config::for_lsp` alone.

TDD:

1. Write all unit tests below first — stub `Config.ignore_link_targets` and
   `Config.ignore_link_target_patterns` as empty so the crate compiles.
2. Run `cargo test` and confirm the new tests **fail**.
3. Wire `ignore_link_targets` through `InitOptions`, `KnapToml`,
   `RawConfig`, `merge()` (union, same as `exclude`), and `finalize()`
   (compile patterns, propagate `Err` on a malformed one); add
   `ignore_link_target_additions` to `for_path`'s signature; add the
   `--ignore-link-target` flag to `Commands::Lint`/`Commands::Index` in
   `src/cli/mod.rs` and thread it through `lint::run`/`index::run` to the
   new `for_path` parameter, until tests pass, then run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `ignore_link_targets: Option<Vec<String>>` on `InitOptions`, `KnapToml`,
  `RawConfig` in `src/config/mod.rs`
- `merge()` unions `ignore_link_targets` the same way it unions `exclude`
- `Config.ignore_link_targets: Vec<String>` (raw) and
  `Config.ignore_link_target_patterns: Vec<glob::Pattern>` (compiled in
  `finalize()`, `Err` on a malformed pattern)
- `for_path(path, extensions_override, exclude_additions, ignore_link_target_additions)`
  — new fourth parameter, appended to `knap.toml`'s list the same way
  `exclude_additions` is
- `--ignore-link-target` flag (`Vec<String>`, repeatable) on
  `Commands::Lint` and `Commands::Index` in `src/cli/mod.rs`, threaded
  through `lint::run`/`index::run` to the new `for_path` parameter

**Unit tests (`src/config/tests.rs`):**

| Test                                                          | What it verifies                                                                     |
| ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `for_path_absent_knap_toml_ignore_link_targets_defaults_empty`   | `config.ignore_link_targets` is `[]` when unset                                        |
| `for_path_loads_knap_toml_ignore_link_targets`                  | `ignore_link_targets = ["../a/**"]` in `knap.toml` appears in `config.ignore_link_targets` |
| `for_path_ignore_link_target_additions_appended`                | `ignore_link_target_additions` passed to `for_path` are appended to, not replacing, `knap.toml`'s list |
| `for_lsp_ignore_link_targets_unions_knap_toml_and_init_options`  | Both sources' patterns present in the result, no duplicates dropped                    |
| `finalize_malformed_ignore_link_targets_pattern_errors`          | An invalid glob in `ignore_link_targets` returns `Err`, same as `exclude`               |

> **Manual checkpoint:** No editor checkpoint — nothing reads
> `Config.ignore_link_target_patterns` yet, and the CLI flag has nothing to
> observably change until Step 3. Covered by unit tests only.

---

## Step 3 — `compute_diagnostics` suppression

Wires both data sources from Steps 1–2 into the one place that emits
`broken-link` diagnostics, which is also where `compute_diagnostics_with_suggestions`
and `knap lint` both read from — this is the step that makes the feature
observable.

TDD:

1. Write all unit tests below first, including the malformed-pattern and
   found-link-unaffected cases — `is_ignored_link_target` can be stubbed to
   always return `false` so the crate compiles and the new tests fail for
   the right reason (diagnostic still present, not a compile error).
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement `is_ignored_link_target` and call it from the `Broken` arm of
   `compute_diagnostics` until tests pass, then run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `fn is_ignored_link_target(target: &str, note: &parser::Note, config: &crate::config::Config) -> bool`
  in `src/handlers.rs` (frontmatter patterns checked first, then
  `config.ignore_link_target_patterns`; malformed frontmatter patterns
  logged via `warn!` and skipped, not propagated as an error)
- `compute_diagnostics`'s `ResolvedLink::Broken` arm calls it and `continue`s
  on a match, before constructing the diagnostic

**Unit tests (`src/handlers.rs`):**

| Test                                                                       | What it verifies                                                                              |
| ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------ |
| `compute_diagnostics_broken_link_ignored_by_frontmatter_exact_match`            | Doc's `ignore-link-targets: [../out/x.md]`; link to `../out/x.md` (outside index) → no `broken-link` diagnostic |
| `compute_diagnostics_broken_link_ignored_by_frontmatter_glob`                  | Doc's `ignore-link-targets: [../out/**]`; link to `../out/x.md` → no `broken-link` diagnostic   |
| `compute_diagnostics_broken_link_not_ignored_by_other_docs_frontmatter`         | Doc B has no `ignore-link-targets`; same broken target Doc A ignores → Doc B still gets the diagnostic |
| `compute_diagnostics_broken_link_ignored_by_knap_toml_pattern`                 | `config.ignore_link_target_patterns` matches the target, doc has no frontmatter → no diagnostic |
| `compute_diagnostics_broken_link_still_reported_when_no_pattern_matches`        | Target matches neither doc nor config patterns → `broken-link` diagnostic unchanged            |
| `compute_diagnostics_found_link_unaffected_by_ignore_patterns`                 | A link that resolves (`Found`) is never suppressed, even if it happens to match a pattern       |
| `compute_diagnostics_broken_anchor_diagnostic_unaffected_by_ignore_patterns`     | A `Found` link's `broken-anchor` diagnostic is untouched by an unrelated `ignore_link_targets` pattern |
| `compute_diagnostics_malformed_frontmatter_pattern_skipped_not_panicking`        | Doc's `ignore-link-targets: ["["]` (malformed glob) → no panic, diagnostic still reported for that target |
| `compute_diagnostics_with_suggestions_omits_suggestions_for_ignored_target`      | Same broken target as an ignored-link test, run through `compute_diagnostics_with_suggestions` → no diagnostic and no suggestion data |

> **Manual checkpoint:** Open a test vault in Zed with `knap lsp` running.
> Add `../../elsewhere/notes.md` (a path genuinely outside the workspace) as
> a link in a note; confirm the Problems panel shows a `broken-link`
> warning. Add `ignore-link-targets: [../../elsewhere/notes.md]` to that
> note's frontmatter, save; confirm the warning disappears without a server
> restart.

---

## Step 4 — Integration tests

End-to-end tests over the full LSP message loop and the `knap lint`/`knap
index` CLI paths, confirming both config sources and the new flag reach
`compute_diagnostics` the way an editor session and a headless run actually
construct `Config`.

**Deliverables:**

- `tests/ignore_link_targets.rs` with all integration tests below
- `cargo test` passes, `cargo clippy -- -D warnings` clean
- `README.md`'s `knap.toml` reference block gains `ignore_link_targets`
  alongside `exclude`/`skip_dirs`, its `--exclude` flag paragraph gains a
  mention of `--ignore-link-target`, and the Frontmatter section gains a
  line documenting `ignore-link-targets`

| Test                                                        | What it verifies                                                                                              |
| ----------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lint_frontmatter_ignore_link_targets_suppresses_diagnostic`  | `knap lint` on a doc with `ignore-link-targets:` naming its own out-of-workspace target → no `broken-link` diagnostic for that link |
| `lint_knap_toml_ignore_link_targets_suppresses_across_docs`   | `knap.toml` `ignore_link_targets = ["../sibling/**"]`; two docs both link into `../sibling/` → neither reports `broken-link` |
| `lint_ignore_link_target_flag_adds_to_config`                | `knap lint --ignore-link-target other/** path` suppresses both the flag's pattern and `knap.toml`'s existing ones |
| `lint_ignore_link_targets_does_not_affect_other_broken_links` | A doc with one ignored link and one genuinely broken in-workspace link → only the second is reported          |
| `index_json_still_reports_ignored_link_as_unresolved`         | `knap index --json` on a vault with `ignore_link_targets` still shows the matching link's `resolved: null` — ignoring is diagnostics-only, not an indexing fact |
| `lsp_initialize_applies_knap_toml_ignore_link_targets`        | An in-process LSP session started against a vault with `knap.toml` `ignore_link_targets` never publishes a `broken-link` diagnostic for a matching target, even after the file is edited |

> **Manual checkpoint (full session):** Open a real vault in Zed with a
> `knap.toml` setting `ignore_link_targets = ["../sibling-vault/**"]` at the
> root. Confirm links from multiple notes into `../sibling-vault/` show no
> Problems-panel warnings, while an unrelated broken link elsewhere still
> does. Confirm `knap lint` on the same vault from a terminal agrees with
> the editor, and that `knap lint --ignore-link-target 'other/**'` suppresses
> an additional one-off pattern without touching `knap.toml`. Confirm `knap
> index --json` still reports the ignored links' true resolution status
> (unchanged — ignoring is a diagnostics-only concern, not an indexing one).

---

## Done — v0.20 (Ignore Link Targets) complete

| Story | Feature                                                            | Delivered in step |
| ----- | ---------------------------------------------------------------------- | ------------------ |
| US-59 | Doc-scoped `ignore-link-targets` frontmatter key                        | Step 3             |
| US-60 | `knap.toml` `ignore_link_targets` + `--ignore-link-target` flag         | Step 3             |
