# v0.8 Design — Frontmatter Schema

Covers the stories in the v0.8 release:

| Story | Feature                                                                         |
| ----- | ------------------------------------------------------------------------------- |
| US-24 | Completions and validation for frontmatter keys/values via user-provided schema |

---

## Goal

A writer who structures notes with consistent frontmatter (a project tracker that
always needs a `status:` field, a meeting log that always needs a `date:`) can
define a schema in their editor's LSP config. knap then offers completions for
key names and enum values as they type, and raises warnings when a required field
is absent or when a value falls outside the defined set. The parser already
extracts all frontmatter key-value pairs into `FrontmatterField` records, and the
completion and diagnostics handlers already manage frontmatter context — this
release wires those two together via a new config option, with no new LSP
capabilities to register.

---

## Config Changes

The schema is delivered as a single object under `frontmatterSchema` in
`initializationOptions`. It has three parts: a `fields` map defining per-key
rules, and two opt-in diagnostic flags that are `false` by default.

```json
"initializationOptions": {
    "frontmatterSchema": {
        "fields": {
            "status": { "values": ["draft", "published", "archived"], "required": true },
            "type":   { "values": ["note", "meeting", "project"] },
            "title":  { "required": true }
        },
        "requireFrontmatter": false,
        "warnOnUnknownKeys": false
    }
}
```

`requireFrontmatter: true` extends required-key diagnostics to notes that have no
frontmatter block at all. `warnOnUnknownKeys: true` emits a warning for every
frontmatter key not defined in `fields`. Both flags are `false` by default —
notes without frontmatter are silently skipped, and extra keys (used by static
site generators, other tools, etc.) are never flagged unless the writer explicitly
opts in.

New Rust types in `src/server/mod.rs`:

```rust
// Deserialization intermediates for initializationOptions.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct FrontmatterSchemaOpts {
    fields: HashMap<String, SchemaFieldOpts>,
    require_frontmatter: bool,
    warn_on_unknown_keys: bool,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct SchemaFieldOpts {
    values: Option<Vec<String>>,  // allowed values; None = any string
    required: bool,
}
```

```rust
// Runtime types carried in Config.

/// Per-key schema rule.
pub(crate) struct SchemaField {
    pub(crate) values: Option<Vec<String>>,  // None = any string is valid
    pub(crate) required: bool,
}

/// The full frontmatter schema: field rules plus two diagnostic flags.
pub(crate) struct FrontmatterSchema {
    pub(crate) fields: Vec<(String, SchemaField)>,  // sorted by key for stable completions
    pub(crate) require_frontmatter: bool,
    pub(crate) warn_unknown_keys: bool,
}

impl Default for FrontmatterSchema {
    fn default() -> Self {
        FrontmatterSchema { fields: vec![], require_frontmatter: false, warn_unknown_keys: false }
    }
}
```

Updated `Config`:

```rust
pub(crate) struct Config {
    pub(crate) index_roots: Vec<PathBuf>,
    pub(crate) extensions: Vec<String>,
    pub(crate) new_note_dir: Option<String>,
    pub(crate) frontmatter_schema: FrontmatterSchema,  // replaces the old Vec<(String, SchemaField)>
}
```

`Config::from_params` converts the `HashMap<String, SchemaFieldOpts>` in
`FrontmatterSchemaOpts.fields` into a `Vec<(String, SchemaField)>` sorted
alphabetically by key (deterministic completion order), and copies the two flags.

The field added to `InitOptions`:

```rust
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct InitOptions {
    extensions: Option<Vec<String>>,
    new_note_dir: Option<String>,
    frontmatter_schema: Option<FrontmatterSchemaOpts>,  // new
}
```

---

## Handler Changes

### `handle_completion` (`textDocument/completion`) — US-24

Gains a `config` parameter and two new trigger branches.

```rust
pub(crate) fn handle_completion(
    params: CompletionParams,
    index: &NoteIndex,
    config: &Config,
) -> Vec<CompletionItem>
```

**New trigger 1 — frontmatter value completion**

`check_frontmatter_value_trigger(content, pos) → Option<(String, String, Range)>`

Returns `Some((key, partial, replace_range))` when:
- The file opens with `---\n` and the cursor is inside the frontmatter block
  (not on either `---` delimiter line).
- The current line is a scalar key-value form `key: value` and the cursor is in
  the value portion (after the `: ` separator).
- The line does not start with `tags:` (that context is handled by the existing
  `check_tag_trigger`).
- The value portion is not a block scalar (`|`, `>`) or inline list (`[`).

`key` is the key text to the left of the colon (trimmed). `partial` is the text
typed in the value position from the start of the value to the cursor.
`replace_range` spans from the start of the value to the cursor.

When this trigger fires the handler looks up `key` in
`config.frontmatter_schema.fields` (case-insensitive key match). If the key is
present and has a `values` list, it returns one `CompletionItem` per allowed
value, filtered to those that start with `partial` (exact-case prefix match,
since value matching is case-sensitive). Each item uses
`CompletionItemKind::VALUE` and a `TextEdit` covering `replace_range`.

**New trigger 2 — frontmatter key completion**

`check_frontmatter_key_trigger(content, pos) → Option<(String, Range)>`

Returns `Some((partial, replace_range))` when:
- Inside the frontmatter block (same gate as above).
- The current line is a top-level key line: it does not start with whitespace or
  `- ` (not a list item) and is not a YAML comment (`#`).
- The cursor is in the key portion: either the line has no `:` yet, or the
  cursor is before the first `:` on the line.

`partial` is the text from the first non-whitespace on the line to the cursor.
`replace_range` spans from the first non-whitespace character on the line to the
cursor.

When this trigger fires the handler returns one `CompletionItem` per schema key
(in `config.frontmatter_schema.fields`) that:
1. Is not already present in `note.frontmatter.fields` (case-insensitive key
   comparison).
2. Has a lowercase form starting with `partial.to_lowercase()`.

Each item uses `CompletionItemKind::FIELD`. The `text_edit` replaces
`replace_range` with `"key: "` (key followed by colon and space).

**Updated priority order** in `handle_completion`:

1. Tag trigger → tag completions (unchanged)
2. **Frontmatter value trigger → schema value completions** (new)
3. Anchor trigger → anchor completions (unchanged)
4. Directory trigger → path completions (unchanged)
5. **Frontmatter key trigger → schema key completions** (new)
6. Return empty (unchanged)

**Code sketch:**

```rust
// Step 2 — schema value completion
if let Some((key, partial, replace_range)) = check_frontmatter_value_trigger(&note.content, pos) {
    let schema = &config.frontmatter_schema;
    if let Some((_, sf)) = schema.fields.iter().find(|(k, _)| k.eq_ignore_ascii_case(&key)) {
        if let Some(allowed) = &sf.values {
            return allowed.iter()
                .filter(|v| v.starts_with(&partial as &str))  // exact-case prefix match
                .map(|v| CompletionItem {
                    label: v.clone(),
                    kind: Some(CompletionItemKind::VALUE),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: replace_range,
                        new_text: v.clone(),
                    })),
                    ..Default::default()
                })
                .collect();
        }
    }
}

// Step 5 — schema key completion
if let Some((partial, replace_range)) = check_frontmatter_key_trigger(&note.content, pos) {
    let schema = &config.frontmatter_schema;
    if !schema.fields.is_empty() {
        let used: HashSet<String> = note.frontmatter
            .as_ref()
            .map(|fm| fm.fields.iter().map(|f| f.key.to_lowercase()).collect())
            .unwrap_or_default();
        return schema.fields.iter()
            .filter(|(k, _)| !used.contains(&k.to_lowercase()))
            .filter(|(k, _)| k.to_lowercase().starts_with(&partial.to_lowercase()))
            .map(|(key, _)| CompletionItem {
                label: key.clone(),
                kind: Some(CompletionItemKind::FIELD),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: replace_range,
                    new_text: format!("{key}: "),
                })),
                ..Default::default()
            })
            .collect();
    }
}
```

---

### `compute_diagnostics` — US-24

Gains a `config` parameter and a new validation block appended after the
existing link-diagnostics loop.

```rust
pub(crate) fn compute_diagnostics(
    path: &Path,
    index: &NoteIndex,
    config: &Config,
) -> Vec<Diagnostic>
```

The new block runs only when `config.frontmatter_schema.fields` is non-empty (or
one of the flags is set). It handles three cases distinguished by whether the
note has a frontmatter block:

**Case A — no frontmatter block, `require_frontmatter: false` (default)**

Skip. Notes without frontmatter are not diagnosed.

**Case B — no frontmatter block, `require_frontmatter: true`**

For each `(key, field)` in `schema.fields` where `field.required` is `true`,
emit a warning at a zero-width range at `(0, 0)`:

```
Missing required frontmatter field: 'status'
```

**Case C — note has a frontmatter block**

Three sub-checks run in order:

*Required keys* — For each `(key, field)` in `schema.fields` where
`field.required` is `true`, search `fm.fields` for a case-insensitive key
match. If absent, emit a warning at `(0, 0)`:

```
Missing required frontmatter field: 'status'
```

*Enum values (exact-case)* — For each `FrontmatterField` in `fm.fields`,
find its schema entry (case-insensitive key match). If the schema entry has a
`values` list and `field.value` is `Some(v)` and `v` does not appear in the
`values` list with exact-case equality, emit a warning at
`field.value_range` (falling back to `field.key_range`):

```
Invalid value 'Draft' for 'status'. Expected one of: draft, published, archived
```

Value comparison is `==`, not case-insensitive. Fields whose `field.value` is
`None` (complex YAML forms the parser cannot represent as a scalar) are skipped.

*Unknown keys* — Only when `schema.warn_unknown_keys` is `true`. For each
`FrontmatterField` in `fm.fields` whose key has no case-insensitive match in
`schema.fields`, emit a warning at `field.key_range`:

```
Unknown frontmatter key: 'foobar'
```

**Code sketch:**

```rust
let schema = &config.frontmatter_schema;
if !schema.fields.is_empty() || schema.require_frontmatter || schema.warn_unknown_keys {
    match &note.frontmatter {
        None if schema.require_frontmatter => {
            for (key, sf) in &schema.fields {
                if sf.required {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position { line: 0, character: 0 },
                            end:   Position { line: 0, character: 0 },
                        },
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: format!("Missing required frontmatter field: '{key}'"),
                        source: Some("knap".to_string()),
                        ..Default::default()
                    });
                }
            }
        }
        None => {}  // requireFrontmatter: false — skip
        Some(fm) => {
            // Required keys
            for (key, sf) in &schema.fields {
                if sf.required && !fm.fields.iter().any(|f| f.key.eq_ignore_ascii_case(key)) {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position { line: 0, character: 0 },
                            end:   Position { line: 0, character: 0 },
                        },
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: format!("Missing required frontmatter field: '{key}'"),
                        source: Some("knap".to_string()),
                        ..Default::default()
                    });
                }
            }
            // Enum value check — exact-case equality
            for field in &fm.fields {
                let Some((_, sf)) = schema.fields.iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(&field.key))
                else { continue };
                let Some(allowed) = &sf.values else { continue };
                let Some(value) = &field.value else { continue };
                if !allowed.iter().any(|v| v == value) {
                    let range = field.value_range.unwrap_or(field.key_range);
                    diagnostics.push(Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: format!(
                            "Invalid value '{}' for '{}'. Expected one of: {}",
                            value, field.key, allowed.join(", ")
                        ),
                        source: Some("knap".to_string()),
                        ..Default::default()
                    });
                }
            }
            // Unknown keys (opt-in)
            if schema.warn_unknown_keys {
                for field in &fm.fields {
                    if !schema.fields.iter().any(|(k, _)| k.eq_ignore_ascii_case(&field.key)) {
                        diagnostics.push(Diagnostic {
                            range: field.key_range,
                            severity: Some(DiagnosticSeverity::WARNING),
                            message: format!("Unknown frontmatter key: '{}'", field.key),
                            source: Some("knap".to_string()),
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }
}
```

---

### `publish_diagnostics` — US-24

Gains a `config` parameter and passes it through to `compute_diagnostics`.

```rust
pub(crate) fn publish_diagnostics(
    paths: &HashSet<PathBuf>,
    index: &NoteIndex,
    config: &Config,
    sender: &Sender<Message>,
)
```

---

## Protocol Handler Changes

No new capabilities to advertise. `textDocument/completion` and
`textDocument/publishDiagnostics` are already registered. All call sites of
`handle_completion`, `compute_diagnostics`, and `publish_diagnostics` in
`src/server/mod.rs` gain the `config` argument, which is already in scope at
every call site.

---

## Testing

### Unit tests (`src/handlers.rs`)

| Test | What it verifies |
| ---- | ---------------- |
| `schema_key_completion_offers_all_unused_keys` | Schema with `status` and `type`; note has empty frontmatter → both keys returned as `FIELD` items |
| `schema_key_completion_excludes_used_keys` | Note already has `status:` field → only `type` offered |
| `schema_key_completion_filters_by_partial` | Partial `"sta"` typed → only `status` offered |
| `schema_key_completion_insert_text_has_colon` | `status` item → `new_text` in `text_edit` is `"status: "` |
| `schema_key_completion_empty_schema_returns_empty` | No schema configured → key trigger returns empty vec |
| `schema_value_completion_offers_enum_values` | Schema `status: ["draft","published"]`; cursor after `status: ` → both values offered as `VALUE` items |
| `schema_value_completion_filters_by_partial` | Partial `"pub"` → only `"published"` offered |
| `schema_value_completion_partial_is_case_sensitive` | Partial `"Pub"` with allowed value `"published"` (lowercase) → nothing offered |
| `schema_value_completion_empty_partial_returns_all` | Cursor right after `status: ` (no partial) → all enum values offered |
| `schema_value_completion_no_values_list_returns_empty` | Schema `title: {}` (no `values`); cursor after `title: ` → empty vec |
| `schema_value_completion_unknown_key_returns_empty` | `foobar: ` not in schema → empty vec |
| `schema_value_completion_tags_key_skipped` | Cursor after `tags: ` → value trigger returns `None`; tag trigger handles it |
| `schema_value_trigger_outside_frontmatter_returns_none` | Body line `status: draft` (not in frontmatter) → value trigger returns `None` |
| `schema_key_trigger_inside_value_returns_none` | Cursor after `: ` on `status: draft` → key trigger returns `None` |
| `check_frontmatter_value_trigger_basic` | `"---\nstatus: dr\n---\n"` cursor after `dr` → `Some(("status", "dr", range))` |
| `check_frontmatter_key_trigger_basic` | `"---\nstat\n---\n"` cursor after `stat` → `Some(("stat", range))` |
| `check_frontmatter_key_trigger_on_list_item` | `"---\n  - foo\n---\n"` cursor on list item → `None` |
| `schema_diag_required_key_absent` | Schema `status` required; note has `---\ntitle: x\n---\n` → warning at (0,0) mentioning `status` |
| `schema_diag_required_key_present_no_warning` | Note has `status: draft` → no required-key warning |
| `schema_diag_value_match_is_exact_case` | Schema allows `["draft","published"]`; note has `status: Draft` (capital D) → warning |
| `schema_diag_exact_value_match_no_warning` | Schema allows `"draft"`; note has `status: draft` → no warning |
| `schema_diag_no_frontmatter_require_off_no_warning` | Schema has required `status`; note has no `---` block; `requireFrontmatter: false` (default) → no diagnostic |
| `schema_diag_no_frontmatter_require_on_warns` | Same note; `requireFrontmatter: true` → warning at (0,0) for missing `status` |
| `schema_diag_unknown_key_warn_off_no_diagnostic` | Note has `foobar: x`; `warnOnUnknownKeys: false` (default) → no diagnostic |
| `schema_diag_unknown_key_warn_on_warns` | Same note; `warnOnUnknownKeys: true` → warning at key range for `foobar` |
| `schema_diag_complex_value_skipped` | Field with `value: None` (block scalar) and enum constraint → no diagnostic |
| `schema_diag_key_match_is_case_insensitive` | Schema key `Status` (capital); note has `status: draft` with `draft` in values → no warning |
| `schema_empty_no_diagnostics` | No schema; arbitrary frontmatter → no schema diagnostics |

### Integration tests (`tests/lsp.rs`)

| Test | What it verifies |
| ---- | ---------------- |
| `test_schema_key_completion` | `textDocument/completion` in frontmatter key position with schema → schema keys offered |
| `test_schema_value_completion` | `textDocument/completion` after `status: ` with enum schema → allowed values offered |
| `test_schema_required_key_missing_diagnostic` | `didOpen` note with frontmatter but no required key → diagnostic published |
| `test_schema_invalid_value_diagnostic` | `didOpen` note with value not in enum → diagnostic published |
| `test_schema_valid_note_no_diagnostic` | `didOpen` note satisfying all schema rules → no schema diagnostics |
| `test_schema_require_frontmatter_warns_on_missing_block` | `requireFrontmatter: true`; note with no `---` block → required-key warning published |
| `test_schema_warn_unknown_keys` | `warnOnUnknownKeys: true`; note has key absent from schema → warning published |
| `test_no_schema_no_extra_diagnostics` | Server started without schema; arbitrary frontmatter → no schema diagnostics |
