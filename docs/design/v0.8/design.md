# v0.8 Design — Frontmatter Schema

Covers the story in the v0.8 release:

| Story | Feature                                                                         |
| ----- | ------------------------------------------------------------------------------- |
| US-24 | Completions and validation for frontmatter keys/values via user-provided schema |

---

## Goal

Let users declare which frontmatter keys their vault uses and what values are valid. The server then offers completions for those keys and values and publishes diagnostics when the frontmatter of a note violates the schema.

---

## Schema Format

The schema is a JSON object passed in `initializationOptions.frontmatterSchema`. It uses a subset of JSON Schema vocabulary that covers the cases a note-taking vault needs.

```json
{
  "properties": {
    "status": { "enum": ["draft", "review", "published"] },
    "priority": { "enum": ["low", "medium", "high"] },
    "author": {}
  },
  "required": ["status"]
}
```

**Field definitions:**

| Property     | Type            | Effect                                                            |
| ------------ | --------------- | ----------------------------------------------------------------- |
| `properties` | object          | Map of key name → field definition. Defines which keys are known. |
| `required`   | array of string | Keys that must be present in every note's frontmatter.            |

**Field definition object:**

| Property | Type            | Effect                                                                    |
| -------- | --------------- | ------------------------------------------------------------------------- |
| `enum`   | array of string | Allowed values. Triggers value completions + enum validation diagnostics. |

A key listed in `properties` with an empty definition `{}` is recognized — key completions are offered and unknown-key diagnostics are suppressed — but no value validation is performed.

If `initializationOptions` contains no `frontmatterSchema`, the feature is disabled and no schema-related completions or diagnostics are emitted.

---

## `initializationOptions` Changes

Two new Rust types, added to `src/server/mod.rs` alongside `InitOptions`:

```rust
#[derive(serde::Deserialize, Default, Clone)]
pub struct FrontmatterFieldSchema {
    #[serde(rename = "enum", default)]
    pub enum_values: Vec<String>,
}

#[derive(serde::Deserialize, Default, Clone)]
pub struct FrontmatterSchema {
    #[serde(default)]
    pub properties: std::collections::HashMap<String, FrontmatterFieldSchema>,
    #[serde(default)]
    pub required: Vec<String>,
}
```

`InitOptions` gains:

```rust
frontmatter_schema: Option<FrontmatterSchema>,
```

`Config` gains:

```rust
pub frontmatter_schema: Option<FrontmatterSchema>,
```

`Config::from_params` maps `opts.frontmatter_schema` → `config.frontmatter_schema`.

The schema is passed to `compute_diagnostics` and `handle_completion` as
`schema: Option<&FrontmatterSchema>`, following the same pattern as `new_note_dir`.

---

## Parser Changes

The parser currently extracts only `title` and `tags` from frontmatter. Schema-driven features need all key-value pairs. Two new types are added to `src/parser/mod.rs`:

```rust
pub struct FrontmatterField {
    pub key: String,
    pub key_range: LspRange,
    pub value: Option<String>,       // None for block scalars, inline lists, nested objects
    pub value_range: Option<LspRange>, // range of the scalar value text, if present
}
```

`Frontmatter` gains:

```rust
pub fields: Vec<FrontmatterField>, // all key-value pairs, in document order
```

**Extraction algorithm** (added to `src/parser/mod.rs`):

Scan the frontmatter block line by line. For each line that matches `key: value`:

- Record `key`, `key_range`, and the scalar `value` trimmed of surrounding whitespace and optional matched quotes.
- Record `value_range` covering the scalar text only (not the quotes).
- If the value is empty, starts with `|`/`>` (block scalars), or starts with `[` (inline list) or `-` (block list continuation), set `value = None`, `value_range = None`.
- Skip lines that do not contain `:`.

`title:` and `tags:` are extracted as before; they are also included in `fields` so schema validation sees them. That means a schema declaring `title` or `tags` in `properties` works naturally.

---

## Completion Handler Changes

`handle_completion` gains a `schema: Option<&FrontmatterSchema>` parameter.

The existing trigger priority is extended:

1. **Tag trigger** (`check_tag_trigger`) → tag completions (unchanged)
2. **Wiki-link trigger** (`check_trigger`) → note completions (unchanged)
3. **Schema value trigger** → enum value completions for the key on the current line
4. **Schema key trigger** → key name completions for unknown/absent schema keys

### Schema value trigger

Fires when:

- Cursor is inside the frontmatter (between the `---` delimiters)
- The current line contains `:` and the cursor is at or after the `:` character
- The key extracted from that line exists in `schema.properties` and has `enum_values`
- The key is not `tags` (tags has its own trigger)

Returns: one `CompletionItem` per `enum_value`, `kind: VALUE`.

```rust
fn check_schema_value_trigger(content: &str, pos: Position, schema: &FrontmatterSchema)
    -> Option<Vec<CompletionItem>>
```

### Schema key trigger

Fires when:

- Cursor is inside the frontmatter
- The current line has no `:` (or is blank / whitespace-only)
- Schema is present

Returns: one `CompletionItem` per key in `schema.properties` whose name is not already a key in the current note's frontmatter `fields`. `kind: PROPERTY`.

```rust
fn check_schema_key_trigger(content: &str, pos: Position, schema: &FrontmatterSchema,
    existing_fields: &[FrontmatterField]) -> Option<Vec<CompletionItem>>
```

---

## Diagnostics Changes

`compute_diagnostics` gains a `schema: Option<&FrontmatterSchema>` parameter.

When `schema` is `Some` and the note has frontmatter, three new diagnostic classes are emitted:

### 1. Unknown key

For each `field` in `frontmatter.fields` whose `key` is **not** in `schema.properties`:

```
Unknown frontmatter key: 'foo'
```

- Severity: `WARNING`
- Range: `field.key_range`

### 2. Invalid enum value

For each `field` in `frontmatter.fields` where:

- `key` is in `schema.properties`
- The field definition has non-empty `enum_values`
- `field.value` is `Some(v)` and `v` is not in `enum_values` (case-sensitive)

```
Invalid value 'v' for 'key': expected one of [a, b, c]
```

- Severity: `WARNING`
- Range: `field.value_range` if present, else `field.key_range`

### 3. Missing required key

For each key in `schema.required` that has no matching entry in `frontmatter.fields`:

```
Missing required frontmatter key: 'status'
```

- Severity: `WARNING`
- Range: line 0, characters 0–3 (the opening `---` delimiter)

When the note has no frontmatter block at all and `required` is non-empty, one diagnostic is emitted per missing required key, all at `(0,0)–(0,0)`.

---

## Testing

### Unit tests (`src/handlers.rs` inline)

| Test                                     | What it verifies                                                             |
| ---------------------------------------- | ---------------------------------------------------------------------------- |
| `schema_value_completion_enum`           | Cursor after `status: ` → completions are the enum values                    |
| `schema_value_completion_no_enum`        | Key in schema but no `enum` → no schema value completions                    |
| `schema_value_completion_unknown_key`    | Key not in schema → no schema value completions                              |
| `schema_key_completion`                  | Blank line in frontmatter → completions are absent schema keys               |
| `schema_key_completion_excludes_present` | Key already in frontmatter → not offered in key completions                  |
| `schema_diag_unknown_key`                | Key not in schema → Warning diagnostic on key_range                          |
| `schema_diag_invalid_enum_value`         | Value not in enum → Warning diagnostic on value_range                        |
| `schema_diag_missing_required`           | Required key absent → Warning diagnostic at line 0                           |
| `schema_diag_no_schema`                  | No schema provided → no schema diagnostics emitted                           |
| `schema_diag_valid_note`                 | All keys known, all values valid, all required keys present → no diagnostics |

### Unit tests (`src/parser/` inline)

| Test                          | What it verifies                                        |
| ----------------------------- | ------------------------------------------------------- | -------------------------- |
| `fields_scalar_values`        | `key: value` → field with key and scalar value          |
| `fields_block_scalar_skipped` | `key:                                                   | `→ field with`value: None` |
| `fields_inline_list_skipped`  | `key: [a, b]` → field with `value: None`                |
| `fields_empty_value`          | `key:` (no value) → field with `value: None`            |
| `fields_quoted_value`         | `key: "hello world"` → value without quotes             |
| `fields_key_range_correct`    | key_range covers only the key name, not the colon       |
| `fields_value_range_correct`  | value_range covers only the scalar text, not whitespace |
| `fields_no_frontmatter`       | Note with no `---` block → `fields` is empty            |

### Integration tests (`tests/frontmatter_schema.rs`)

| Test                                      | What it verifies                                             |
| ----------------------------------------- | ------------------------------------------------------------ |
| `schema_value_completion_round_trip`      | Cursor after `status: ` → enum completions returned over LSP |
| `schema_key_completion_round_trip`        | Blank line in frontmatter → schema key completions returned  |
| `schema_diag_invalid_value_round_trip`    | Invalid enum value → diagnostic published after didOpen      |
| `schema_diag_missing_required_round_trip` | Missing required key → diagnostic published after didOpen    |
