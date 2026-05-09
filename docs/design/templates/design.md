# vX.Y Design — [Release Name]

Covers the stories in the vX.Y release:

| Story | Feature |
| ----- | ------- |
| US-XX | ...     |

---

## Goal

One paragraph describing the user value this release delivers and why these
stories belong together.

---

## Config Changes

_Omit this section if no new config options are introduced._

New fields added to `InitOptions` and `Config`:

```rust
new_option: Type,  // description
```

---

## Parser Changes

_Omit this section if the parser is unchanged._

New or modified types in `src/parser/mod.rs`:

```rust
pub struct NewType {
    pub field: Type,  // description
}
```

Algorithm description for any new extraction function:

```rust
fn new_function(args) -> ReturnType
```

Edge cases to handle:

- Case 1 → expected behaviour
- Case 2 → expected behaviour

---

## Note Index Changes

_Omit this section if the index is unchanged._

New fields or methods on `NoteIndex`:

```rust
new_field: Type,
```

```rust
pub fn new_method(&self, args) -> ReturnType
```

---

## Handler Changes

_Omit this section if no handlers are new or modified._

One subsection per affected handler.

### `handle_xxx` (`textDocument/xxx`)

What changes and why. Include the updated function signature if it gains
parameters.

```rust
pub fn handle_xxx(params: XxxParams, index: &NoteIndex, ...) -> XxxResult
```

Trigger conditions (for completion) or priority order (for definition/hover):

1. Condition one → result
2. Condition two → result

---

## Testing

### Unit tests

List tests by file. One row per test case.

| Test        | What it verifies  |
| ----------- | ----------------- |
| `test_name` | Short description |

### Integration tests (`tests/xxx.rs`)

| Test                   | What it verifies  |
| ---------------------- | ----------------- |
| `test_name_round_trip` | Short description |
