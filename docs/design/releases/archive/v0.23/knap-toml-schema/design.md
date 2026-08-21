# v0.23 Design — knap.toml Schema

Covers the stories in the v0.23 release:

| Story | Feature                                                                                       |
| ----- | ---------------------------------------------------------------------------------------------- |
| US-61 | Autocompletion and validation for `knap.toml` in taplo-aware editors, via a published JSON Schema (#75) |

---

## Goal

A workspace owner editing `knap.toml` in a taplo-aware editor (Zed's built-in
TOML support, VS Code + "Even Better TOML") gets the same inline
autocompletion and validation that `initializationOptions` already offers in
JSON-Schema-aware editors (US-31) — instead of guessing key names and casing
from memory or the README.

`initializationOptions` already has a JSON Schema at
`schemas/v1/initialization_options.json`. It cannot simply be pointed at
`knap.toml` as well: `InitOptions` (`src/config/mod.rs`) is deliberately
`camelCase` — it's a JSON wire contract editor extensions depend on — while
`KnapToml` is a separate, deliberately `snake_case` struct, and the two
aren't even a pure casing mirror of each other. The clearest divergence is
the frontmatter block: `InitOptions`' is `warnOnUnknownKeys`, `KnapToml`'s
is `warn_unknown_keys` (`FrontmatterSchemaTomlOpts`, `src/config/mod.rs`).
Pointing a TOML-aware editor at the existing schema would validate
`knap.toml` against the wrong key names and flag valid files as errors.

This release adds a second schema, `schemas/v1/knap_toml.json`, describing
`KnapToml`'s actual shape, and documents the two ways taplo lets an editor
find it: an inline `#:schema` directive as the first line of `knap.toml`, or
a `taplo.toml`/`.taplo.toml` config associating the schema with the
`knap.toml` glob workspace-wide. Both schema files stay `v1` and follow the
same versioning discipline `schemas/README.md` already lays out — this
release doesn't change that discipline, it just gives it a second schema to
apply to.

No Rust code changes: `KnapToml` already has the right shape (added
incrementally across v0.16–v0.20 as `exclude`/`skip_dirs`/
`ignore_link_targets` landed). This release only adds a schema file, a
regression test guarding it against the same kind of drift `schemas/v1/
initialization_options.json` suffered (#72, fixed in v0.22), and the docs
that describe how to wire it up.

---

## Schema Changes

New file `schemas/v1/knap_toml.json`, describing `KnapToml`'s snake_case
shape — a sibling of `schemas/v1/initialization_options.json`, not a
replacement:

```json
{
  "$schema": "http://json-schema.org/draft-07/schema",
  "title": "knap.toml",
  "description": "Workspace configuration for knap, read by `knap lsp`, `knap lint`, and `knap index` alike. See schemas/README.md for versioning rules.",
  "type": "object",
  "properties": {
    "extensions": {
      "type": "array",
      "items": { "type": "string" },
      "default": ["md"],
      "description": "File extensions treated as notes. Files with other extensions are treated as attachments. Default: [\"md\"]."
    },
    "new_note_dir": {
      "type": "string",
      "description": "Folder relative to the workspace root where Quick Fix 'Create note' actions create new files (e.g. \"0-Inbox\"). Defaults to the same directory as the current note."
    },
    "exclude": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Glob patterns excluded from indexing, matched against each entry's path relative to its index root."
    },
    "skip_dirs": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Glob patterns matched against bare directory names, pruned from the crawl. Replaces the built-in default (\".*\", \"node_modules\", \"target\") outright when set."
    },
    "ignore_link_targets": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Glob patterns matched against a link's raw target text, suppressing broken-link diagnostics."
    },
    "frontmatter_schema": {
      "type": "object",
      "description": "Defines allowed keys and values for a doc's frontmatter. Enables key/value completions and diagnostics.",
      "properties": {
        "require_frontmatter": {
          "type": "boolean",
          "default": false,
          "description": "Warn on docs that have no '---' frontmatter block at all when required keys exist."
        },
        "warn_unknown_keys": {
          "type": "boolean",
          "default": false,
          "description": "Warn on frontmatter keys not listed in 'fields'."
        },
        "fields": {
          "type": "object",
          "description": "Map of frontmatter key names to their constraints.",
          "additionalProperties": {
            "type": "object",
            "properties": {
              "required": {
                "type": "boolean",
                "default": false,
                "description": "Warn when the key is absent from the doc's frontmatter."
              },
              "values": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Allowed values for this key (exact-case). Omit to allow any value."
              }
            },
            "additionalProperties": false
          }
        }
      },
      "additionalProperties": false
    }
  },
  "additionalProperties": false
}
```

No `$schema` property inside `properties` — unlike `initializationOptions`
(a JSON blob nested under an editor's own `settings.json`, where an
in-band `"$schema"` key is the natural place to reference it), taplo's
convention points a TOML file at its schema out-of-band: an inline
`#:schema <path>` comment on line 1, or a `taplo.toml` glob association.
Neither puts a `$schema` key inside `knap.toml`'s own TOML content, so
`additionalProperties: false` should reject one if it ever showed up
there.

This is a first version, not a fix to an existing one — there's no prior
`schemas/v1/knap_toml.json` to have drifted, so nothing in `schemas/
README.md`'s versioning table applies yet. Future changes to it follow that
table starting now.

---

## Docs Changes

`schemas/README.md`:

- Restructure "Current version" to list both schema files (`initialization_options.json` and the new `knap_toml.json`) with their raw GitHub URLs, rather than naming only the JSON one.
- The versioning rules table and "When a new version is needed" steps apply to either file unchanged — no rewording needed there, just note both files share the same discipline.

`docs/GETTING_STARTED.md`, `## 3. Configuration`:

- New subsection after "Schema (Zed / JSON-aware editors)": **"Schema (taplo / TOML-aware editors)"**. Documents both wiring options:

  Inline directive — first line of `knap.toml`:

  ```toml
  #:schema https://raw.githubusercontent.com/sleb/knap/main/schemas/v1/knap_toml.json

  extensions = ["md", "mdx"]
  ```

  Or a `taplo.toml`/`.taplo.toml` at the workspace root, associating the
  schema with the `knap.toml` glob without editing `knap.toml` itself:

  ```toml
  [[schema]]
  path = "https://raw.githubusercontent.com/sleb/knap/main/schemas/v1/knap_toml.json"
  include = ["knap.toml"]
  ```

  Links to `schemas/README.md` for versioning rules, same as the existing
  JSON section.

`README.md`, `### knap.toml` section:

- Extend the existing one-line pointer ("`initializationOptions` has a JSON
  Schema at … see `schemas/README.md`") to also name `schemas/v1/
  knap_toml.json` for `knap.toml` itself, with a one-clause mention of the
  `#:schema` directive as the quickest way to wire it up.

---

## Testing

Pure JSON/Markdown content — no Rust types change (`KnapToml` is already
correct), so there's no function to unit-test in the usual sense. Same
posture as v0.22's schema-sync release: a regression test pins the new
schema file's own key sets against the real wire contract, plus one unit
test confirming the documented example still deserializes.

### Integration tests (`tests/schema.rs`, extended)

Mirrors the existing `initialization_options.json` tests structurally, but
against `KnapToml`'s snake_case key set (no `$schema` property to allow —
see Schema Changes):

| Test                                                                    | What it verifies                                                                                              |
| ------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| `knap_toml_schema_is_valid_json`                                         | `schemas/v1/knap_toml.json` parses as JSON                                                                        |
| `knap_toml_schema_top_level_properties_match_wire_contract`              | Top-level `properties` keys are exactly `{extensions, new_note_dir, exclude, skip_dirs, ignore_link_targets, frontmatter_schema}` |
| `knap_toml_schema_frontmatter_schema_properties_match_wire_contract`     | `frontmatter_schema.properties` keys are exactly `{require_frontmatter, warn_unknown_keys, fields}`               |
| `knap_toml_schema_frontmatter_schema_fields_entry_properties_match_wire_contract` | `frontmatter_schema.properties.fields.additionalProperties.properties` keys are exactly `{required, values}` |

### Unit tests (`src/config/tests.rs`)

`KnapToml` is private to the `config` module, so the doc/struct round-trip
check lives here, same as `InitOptions`' equivalent test.

| Test                                                          | What it verifies                                                                                          |
| ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `knap_toml_accepts_readme_example`                                | The exact `knap.toml` example block from `README.md`'s `### knap.toml` section deserializes into `KnapToml` without error, with the expected `extensions`/`skip_dirs`/`frontmatter_schema.fields` values |
