# Working notes for Claude

### LSP usage

Always use the `LSP` tool (rust-analyzer) when coding on this project. Prefer it over grep/Read for:

- Resolving types and trait bounds (`hover`)
- Finding all real call sites before renaming or refactoring (`findReferences`)
- Verifying a function's callers/callees (`incomingCalls`, `outgoingCalls`)
- Navigating to a definition across module boundaries (`goToDefinition`)
