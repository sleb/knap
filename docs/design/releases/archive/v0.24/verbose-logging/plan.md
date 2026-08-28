# v0.24 Implementation Plan — Verbose Logging

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the flag should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                          | Status | Notes |
| ---------------------------------------------- | ------ | ----- |
| 1 — Verbosity → filter mapping                 | Done   |       |
| 2 — `Cli` flag + logger bootstrap move          | Done   |       |
| 3 — Integration tests                          | Done   |       |

---

## Step 1 — Verbosity → filter mapping

Introduce the pure mapping function first, with its tests, before touching
`Cli` or `main.rs` — it has no dependency on clap or `env_logger` and is
fully testable in isolation.

This step uses TDD:

1. Write all unit tests for this step first — add a stub
   `fn default_log_filter(verbose: u8) -> &'static str { "info" }` to
   `src/cli/mod.rs` so the module compiles.
2. Run `cargo test` and confirm the new tests **fail** (the `1`/`2+` cases
   won't match `"info"`).
3. Implement the real `match` until tests pass, then run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `fn default_log_filter(verbose: u8) -> &'static str` in `src/cli/mod.rs`
  (private; only `cli::run()` calls it, added in Step 2)

**Unit tests:**

| Test                                       | What it verifies                                                    |
| ------------------------------------------- | ---------------------------------------------------------------------- |
| `default_log_filter_zero_is_info`           | `default_log_filter(0) == "info"`                                      |
| `default_log_filter_one_is_debug`           | `default_log_filter(1) == "debug"`                                     |
| `default_log_filter_two_or_more_is_trace`   | `default_log_filter(2)` and `default_log_filter(5)` both `== "trace"`  |

> **Manual checkpoint:** No editor checkpoint — this function isn't wired up
> to anything yet; covered entirely by the unit tests above.

---

## Step 2 — `Cli` flag + logger bootstrap move

Add the `verbose` field to `Cli`, and move `env_logger` initialization out of
`src/main.rs` into `cli::run()` so it can see the parsed flag.

This step uses TDD for the parsing behaviour (the logger-bootstrap half isn't
unit-testable in-process — see Step 3):

1. Write all unit tests for this step first, in the existing
   `#[cfg(test)] mod tests` block in `src/cli/mod.rs` (alongside the
   `Skill`-flag tests already there) — they'll fail to compile until the
   `verbose` field exists, which is the expected "failing" state for this
   step.
2. Add the `verbose: u8` field to `Cli` (with `global = true`,
   `action = clap::ArgAction::Count`) so the tests compile and run; confirm
   they pass.
3. Move the `env_logger::Builder::from_env(...).init()` call from
   `src/main.rs` into the top of `cli::run()`, using
   `default_log_filter(cli.verbose)` as the `filter_or` default, followed by
   the one `debug!(...)` plumbing-confirmation line. Shrink `main.rs` to just
   call `knap::cli::run()`. Run `cargo clippy -- -D warnings`.

**Deliverables:**

- `verbose: u8` field added to `Cli` in `src/cli/mod.rs`, with its doc
  comment (`-v` → debug, `-vv` → trace, `KNAP_LOG` overrides)
- `cli::run()` in `src/cli/mod.rs` builds the `env_logger::Builder` using
  `default_log_filter(cli.verbose)` and logs the one `debug!` plumbing line
- `src/main.rs` reduced to `fn main() -> anyhow::Result<()> { knap::cli::run() }`
- `use log::debug;` added to `src/cli/mod.rs`'s imports

**Unit tests:**

| Test                                       | What it verifies                                                       |
| ------------------------------------------- | -------------------------------------------------------------------------- |
| `verbose_defaults_to_zero`                  | `Cli::try_parse_from(["knap", "version"])` parses with `verbose == 0`      |
| `single_v_parses_as_one`                    | `knap -v version` parses with `verbose == 1`                              |
| `double_v_parses_as_two`                    | `knap -vv version` parses with `verbose == 2`                             |
| `repeated_long_flag_counts`                 | `knap --verbose --verbose version` parses with `verbose == 2`             |
| `verbose_flag_is_global_after_subcommand`   | `knap lint . -v` parses with `verbose == 1`                               |

> **Manual checkpoint:** Run `KNAP_LOG= knap -vv index .` (unset `KNAP_LOG`
> explicitly, some shells inherit it) from the repo root and confirm stderr
> now shows a `TRACE`/`DEBUG`-prefixed line for the logging-initialized
> message that plain `knap index .` doesn't print.

---

## Step 3 — Integration tests

End-to-end tests over the real `knap` binary as a subprocess, confirming the
flag actually changes what reaches stderr and that `KNAP_LOG` still wins when
both are set. Always the last step, since it exercises the fully wired-up
binary from Steps 1–2.

**Deliverables:**

- New tests added to `tests/cli.rs` (using the existing `knap()` helper;
  `env_logger` writes to stderr, which `Command::output()` already captures
  in that file's other tests)
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                                | What it verifies                                                                                                     |
| ----------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `verbose_flag_raises_stderr_log_level`                | `knap -v version` stderr contains the debug plumbing line; plain `knap version` stderr does not                       |
| `double_verbose_flag_reaches_trace`                   | `knap -vv version` stderr still contains the plumbing line (level filter is `trace`, at least as permissive as `-v`)   |
| `knap_log_env_var_overrides_verbose_flag`             | `knap -vv version` run with `KNAP_LOG=error` set has no plumbing line on stderr — the env var wins over `-v`           |
| `no_verbose_flag_preserves_current_default_output`    | `knap version` with neither `-v` nor `KNAP_LOG` set has no debug/trace lines on stderr — regression guard for the `main.rs` → `cli::run()` move |

> **Manual checkpoint (full session):** Point an editor's LSP client at
> `knap lsp -v` (or wherever the editor lets you add subprocess args) instead
> of bare `knap lsp`. Open a note with a broken link, trigger a diagnostics
> refresh, and confirm the editor-visible stderr/output-panel log for the
> `knap` subprocess (or a redirected log file, depending on the editor) now
> includes `debug`-level lines like `request: method=...` that were silent
> before. Confirm a second editor window using bare `knap lsp` (no `-v`) is
> unaffected — the flag is per-invocation, not global state.

---

## Done — v0.24 complete

| Story  | Feature                                        | Delivered in step |
| ------ | ----------------------------------------------- | ------------------ |
| US-D23 | `--verbose`/`-v` flag on every `knap` subcommand | Step 3              |
