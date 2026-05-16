# Working notes for Claude

## Development workflow

This project uses GitHub Flow: all work on `feat/` or `fix/` branches; merge to `main` only when cutting a release. See `docs/RELEASING.md` or use the `release` skill.

### LSP usage

Always use the `LSP` tool (rust-analyzer) when coding on this project. Prefer it over grep/Read for:

- Resolving types and trait bounds (`hover`)
- Finding all real call sites before renaming or refactoring (`findReferences`)
- Verifying a function's callers/callees (`incomingCalls`, `outgoingCalls`)
- Navigating to a definition across module boundaries (`goToDefinition`)
