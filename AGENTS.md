# AGENTS.md

This file provides guidance to agents when working with code in this repository.

## Project

knap is a Markdown LSP server written in Rust. It brings IDE-quality linking and
navigation to any LSP-compatible editor using standard Markdown `[text](path)`
links — no wiki-link extensions. See `README.md` for the one-paragraph summary.

## Commands

```bash
cargo build                        # build
cargo test                         # all tests
cargo test <test_name>             # single test by name
cargo test --test <integration>    # single integration test file
cargo clippy -- -D warnings        # lint (warnings are errors)

# Debug CLI — invoke components directly without a running editor
cargo run -- parse <file>          # print links extracted from a file
cargo run -- index <dir>           # print the note index built from a directory
```

## Documentation

All design decisions live in `docs/`. Read the relevant doc before starting any
task:

| Doc                                   | When to read                                                         |
| ------------------------------------- | -------------------------------------------------------------------- |
| `docs/ARCHITECTURE.md`                | Touching any component boundary or adding a new component            |
| `docs/ROADMAP.md`                     | Scoping work — confirms what's in vs. out for the current release    |
| `docs/RELEASING.md`                   | Cutting a release — use the `/release` skill to walk through it      |
| `docs/design/releases/vX.Y/design.md` | Implementing anything in the current release                         |
| `docs/design/releases/vX.Y/plan.md`   | Implementation order and testing checkpoints for the current release |
| `docs/design/components/*.md`         | Implementing a specific component (parser, index, handlers, etc.)    |

**Starting a new release cycle:** copy `docs/design/releases/templates/design.md` and
`docs/design/releases/templates/plan.md` into `docs/design/releases/vX.Y/`, fill in the release
name, stories, and steps, then work through the plan in order.

**Docs must stay in sync with the code.** If you deviate from a design doc
during implementation — a better approach is found, an edge case changes the
design — update the relevant doc in the same change. Never let the docs drift
from the code.

## Architecture

See [ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full component design, data flows, and
invariants.

## Testing

### Test-driven development

All new handler functions, index mutations, and non-trivial logic follow TDD:

1. Write the tests first — stub the function signature so the file compiles
2. Run `cargo test` and confirm the new tests **fail** before writing any implementation
3. Implement until all tests pass, then run `cargo clippy -- -D warnings`

Do not write implementation code before the failing tests exist.

### Guidelines

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

## Extension Repositories

Editor-specific extensions live in separate repos:

| Repo                                               | Editor  |
| -------------------------------------------------- | ------- |
| [zed-knap](https://github.com/sleb/zed-knap)       | Zed     |
| [vscode-knap](https://github.com/sleb/vscode-knap) | VS Code |

**Extension documentation policy:** Extension repos contain only what is unique
to that extension (installation steps, binary resolution, editor-specific
settings). All feature documentation, architecture decisions, and policies live
here. Extension READMEs link back to `knap` rather than replicating content.

## Skills

Skills provide specialized instructions and workflows for specific tasks. Use
the skill tool to load a skill when a task matches its description.

<available_skills>
<skill>
<name>release</name>
<description>
Interactive release checklist for knap. Use this skill whenever the user
mentions cutting a release, tagging a version, shipping a milestone, or
anything like "let's release", "time to tag", "ready to ship v0.x", or
"how do I release". Walk through the full docs/RELEASING.md checklist
step-by-step, surfacing current state, proposing each action, and waiting
for explicit confirmation before doing anything irreversible.
</description>
<location>file://.agents/skills/release/SKILL.md</location>
</skill>
</available_skills>

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
