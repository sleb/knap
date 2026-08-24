# v0.24 Design — Verbose Logging

Covers the stories in the v0.24 release:

| Story  | Feature                                             |
| ------ | ---------------------------------------------------- |
| US-D23 | `--verbose`/`-v` flag on every `knap` subcommand      |

---

## Goal

A developer (or an agent) debugging knap itself — most often reproducing a
report that `knap lsp` isn't publishing diagnostics, or that a `rename-*` did
something unexpected — can add `-v`/`-vv` to any `knap` invocation and get
`debug`/`trace`-level output immediately, without first knowing that
`KNAP_LOG` exists or having a way to set it in front of a subprocess an
editor spawns. This is a single story because it's one mechanism (a global
CLI flag lowering the same `env_logger` filter that `KNAP_LOG` already
controls) applied uniformly, not a per-subcommand feature.

---

## CLI Changes

_No config, parser, note index, or handler changes — this is CLI argument
and logging-bootstrap plumbing only._

### `Cli` gains a global `verbose` flag

`src/cli/mod.rs`:

```rust
#[derive(Parser)]
#[command(
    name = "knap",
    version,
    about = "A minimal, opinionated LSP for Markdown notes"
)]
struct Cli {
    /// Increase log verbosity: -v for debug, -vv (or higher) for trace.
    /// Overridden by KNAP_LOG if it's set, for finer-grained (e.g.
    /// per-module) control.
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}
```

`global = true` is what makes this "apply to every subcommand consistently"
without adding the field to each `Commands` variant: clap accepts the flag
either before or after the subcommand name (`knap -v lint .` and
`knap lint . -v` both parse), including on `Lsp`, which takes no other
arguments today. `ArgAction::Count` gives the repeatable `-v`/`-vv`
convention the issue asks for — `u8` is more than enough range, and clap
rejects a bare `--verbose=n` form so there's no second spelling to document.

### Verbosity maps to a filter level

`src/cli/mod.rs`:

```rust
/// The `env_logger` filter to use when `KNAP_LOG` isn't set, based on how
/// many times `-v` was passed. Kept as a standalone function so the mapping
/// itself is unit-testable without touching the process-global logger.
fn default_log_filter(verbose: u8) -> &'static str {
    match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    }
}
```

`0 => info` preserves today's default exactly, so an invocation with no `-v`
behaves identically to before this change.

### Logger initialization moves from `main.rs` into `cli::run()`

Today `src/main.rs` calls `env_logger::Builder::from_env(...).init()` before
`knap::cli::run()`, which means the logger's default level is fixed before
`Cli::parse()` has a chance to see `-v`. Since the flag has to be parsed
before the logger is built, initialization moves into `run()`, right after
parsing:

```rust
pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let filter = default_log_filter(cli.verbose);
    env_logger::Builder::from_env(env_logger::Env::default().filter_or("KNAP_LOG", filter))
        .init();
    debug!(
        "logging initialized: verbose={} (-v count), effective default filter={filter}",
        cli.verbose
    );

    match cli.command {
        // ... unchanged
    }
}
```

`filter_or` already implements the "escape hatch" requirement for free:
it uses `KNAP_LOG`'s value verbatim when the env var is set, and only falls
back to `filter` (derived from `-v`) when it isn't. So `KNAP_LOG=warn -vv
knap lsp` still runs at `warn`, and `KNAP_LOG` keeps supporting the
finer-grained per-module filter syntax `env_logger` already accepts (e.g.
`KNAP_LOG=knap::handlers=trace,info`) — nothing about that escape hatch
changes.

The one new log statement (`debug!(...)` above) isn't feature logging; it's
the flag's own confirmation that it took effect, at `debug` level so it's
silent by default and only appears once the developer has already asked for
more output — useful because "is `-v` actually doing anything" is exactly
the kind of question this feature exists to make unnecessary to ask via
`KNAP_LOG` trial and error.

`src/main.rs` shrinks to:

```rust
fn main() -> anyhow::Result<()> {
    knap::cli::run()
}
```

### Non-goals (explicitly out of scope for this release)

- No new `log::{debug,info,warn}` call sites in `server.rs`/`handlers.rs`/
  `config/mod.rs` beyond the one plumbing-confirmation line above — existing
  calls simply become reachable at a useful verbosity without an env var.
- No LSP `window/logMessage` forwarding. The issue calls this out explicitly
  as a follow-up, not part of this request, because `knap lsp`'s stderr
  today already receives whatever `env_logger` writes — `-v` makes that
  stream more useful, it just doesn't relocate it into the LSP protocol
  itself. That's future work if editors' subprocess-stderr handling proves
  to be the actual blocker after this ships.
- No change to the `log`/`env_logger` dependency choice.

---

## Testing

### Unit tests (`src/cli/mod.rs`)

| Test                                             | What it verifies                                                       |
| ------------------------------------------------ | ------------------------------------------------------------------------ |
| `default_log_filter_zero_is_info`                | `default_log_filter(0) == "info"`                                        |
| `default_log_filter_one_is_debug`                | `default_log_filter(1) == "debug"`                                       |
| `default_log_filter_two_or_more_is_trace`         | `default_log_filter(2)` and `default_log_filter(5)` both `== "trace"`    |
| `verbose_defaults_to_zero`                        | `Cli::try_parse_from(["knap", "version"])` parses with `verbose == 0`    |
| `single_v_parses_as_one`                          | `knap -v version` parses with `verbose == 1`                             |
| `double_v_parses_as_two`                          | `knap -vv version` parses with `verbose == 2`                            |
| `repeated_long_flag_counts`                       | `knap --verbose --verbose version` parses with `verbose == 2`            |
| `verbose_flag_is_global_after_subcommand`         | `knap lint . -v` parses with `verbose == 1` (flag accepted after subcommand, not just before) |

### Integration tests (`tests/cli.rs`)

| Test                                              | What it verifies                                                                                                   |
| -------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `verbose_flag_raises_stderr_log_level`             | `knap -v version` (real subprocess) has the debug plumbing line on stderr; plain `knap version` does not             |
| `double_verbose_flag_reaches_trace`                 | `knap -vv version`'s stderr contains a `TRACE`-or-lower-filtered line (the same plumbing line, now at a level that also passes a trace filter) — confirms `-vv` isn't capped at `debug` |
| `knap_log_env_var_overrides_verbose_flag`           | With `KNAP_LOG=error` set and `-vv` passed, the debug plumbing line is absent from stderr — `KNAP_LOG` wins over `-v` |
| `no_verbose_flag_preserves_current_default_output`  | `knap version` (no flag, no `KNAP_LOG`) stderr is unchanged from today's baseline (no debug/trace lines) — regression guard for the `main.rs` → `cli::run()` move |
