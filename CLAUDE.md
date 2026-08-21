# Working notes for Claude

### LSP usage

Always use the `LSP` tool (rust-analyzer) when coding on this project. Prefer it over grep/Read for:

- Resolving types and trait bounds (`hover`)
- Finding all real call sites before renaming or refactoring (`findReferences`)
- Verifying a function's callers/callees (`incomingCalls`, `outgoingCalls`)
- Navigating to a definition across module boundaries (`goToDefinition`)

### Git workflow

Do work directly on `main` whenever possible — don't create a feature
branch by default. Only branch when there's a specific reason to (e.g. the
user asks for a PR, or work needs to stay isolated from `main` for a
while).
