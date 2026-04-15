# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with
code in this repository.

## Project

Knap is a Markdown LSP server written in Rust. It brings Obsidian-style
`[[wiki-link]]` navigation to any LSP-compatible editor. See `README.md` for the
one-paragraph summary.

## Commands

```bash
cargo build                        # build
cargo test                         # all tests
cargo test <test_name>             # single test by name
cargo test --test <integration>    # single integration test file
cargo clippy -- -D warnings        # lint (warnings are errors)

# Debug CLI — invoke components directly without a running editor
cargo run -- parse <file>          # print stem + wiki-links extracted from a file
cargo run -- index <dir>           # print the note index built from a directory (Step 3+)
```

## Documentation

All design decisions live in `docs/`. Read the relevant doc before starting any
task:

| Doc                           | When to read                                                      |
| ----------------------------- | ----------------------------------------------------------------- |
| `docs/ARCHITECTURE.md`        | Touching any component boundary or adding a new component         |
| `docs/ROADMAP.md`             | Scoping work — confirms what's in vs. out for the current release |
| `docs/RELEASING.md`           | Cutting a release — use the `/release` skill to walk through it   |
| `docs/design/v0.3/design.md`  | Implementing anything in the v0.3 release                         |
| `docs/design/v0.3/plan.md`    | Implementation order and testing checkpoints for v0.3             |
| `docs/design/components/*.md` | Implementing a specific component (parser, index, handlers, etc.) |

**Docs must stay in sync with the code.** If you deviate from a design doc
during implementation — a better approach is found, an edge case changes the
design — update the relevant doc in the same change. Never let the docs drift
from the code.

## Architecture

See `docs/ARCHITECTURE.md` for the full component design, data flows, and
invariants.

## Testing

Keep tests lean:

- **Unit test** pure logic: the parser, `LineIndex`, `NoteIndex` operations
  (`index`, `remove`, `resolve`), handler functions
- **Integration test** the full message loop with a real `Connection` over
  in-process pipes — at least one test per LSP capability
- Don't test `pulldown-cmark` or `lsp-server` behaviour — they are already
  tested upstream
- Don't mock the `NoteIndex` in handler tests — build a real index with fixture
  notes instead; mocking it just tests the mock
- One focused assertion per test case. If a test needs extensive setup to
  exercise one behaviour, that's a signal the component has too much coupling

## Fearless Refactoring

This is a personal project. Never add backward-compatible fallback logic:

- If a file is expected to exist, read it and propagate the error — don't
  silently skip
- If a config value is required, fail loudly on startup — don't substitute a
  default that hides a misconfiguration
- If a type or function is renamed or removed, delete the old name — no
  re-exports, no aliases, no deprecation shims
- `unwrap()` and `expect("reason")` are appropriate for invariants that should
  never fail; prefer `expect` with a clear message over bare `unwrap`
- When changing a data structure or interface, update all call sites immediately
  rather than keeping old paths alive
