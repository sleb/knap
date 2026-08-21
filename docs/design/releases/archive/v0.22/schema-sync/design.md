# v0.22 Design — Schema Sync

Covers the stories in the v0.22 release:

| Story | Feature                                                                     |
| ----- | ---------------------------------------------------------------------------- |
| #72   | `schemas/v1/initialization_options.json` is stale — doesn't match current `frontmatterSchema` shape (Bug) |

---

## Goal

A writer using Zed (or any JSON-Schema-aware editor) who points `$schema` at
`schemas/v1/initialization_options.json` gets inline completions and
validation for their `initializationOptions`. Today that schema lies to
them: it describes a `frontmatterSchema` shape (`properties`/`enum`/
`required`) that has never been the real wire contract, it still lists
`attachmentsDir`, a key removed from `InitOptions` back in v0.2, and it's
missing `exclude`, `skipDirs`, and `ignoreLinkTargets` entirely. The result
is the opposite of the schema's purpose — a config the server accepts
outright is flagged red in the editor, and a config the editor's completions
would suggest (`properties`/`enum`) is silently ignored by `knap lsp`.

This is a same-version fix, not a schema version bump. Per
`schemas/README.md`'s versioning rules, a bump is only required when a
change would break an *existing valid config* — i.e. when the wire shape
itself changes. Git history shows the wire shape never changed:
`frontmatter_schema`'s `fields`/`requireFrontmatter`/`warnOnUnknownKeys`
shape has been in `src/config/mod.rs` since it was introduced in v0.8
(`f979947`), and the JSON Schema file's `frontmatterSchema` entry — added in
the very same v0.8 release commit (`ec6c4d1`) — was simply authored with the
wrong shape from day one and never corrected. No config that validates
against today's file has ever actually matched what `knap lsp` accepts, so
there is no existing-valid-config case to protect by versioning. The fix is
to make `schemas/v1/initialization_options.json` describe reality: drop
`attachmentsDir`, add the three missing top-level keys, and replace
`frontmatterSchema`'s `properties`/`enum`/`required` shape with
`fields`/`values`/`requireFrontmatter`/`warnOnUnknownKeys`, matching
`README.md` and `docs/GETTING_STARTED.md`, which already document the
correct shape.

While fixing the schema's content, also fix
`docs/GETTING_STARTED.md`'s "Schema (Zed / JSON-aware editors)" section,
which points at `schemas/initialization_options.json` — the pre-`v1/` path
from before US-31 moved the file to `schemas/v1/` (`28523d2`). Same root
cause (a file moved/changed and a doc quoting its old shape/path never
caught up), same fix pass.

`schemas/README.md` — the doc that actually states the versioning rules
this whole issue turns on — has never been linked from anywhere: not
`README.md`, not `GETTING_STARTED.md`, not the schema file itself. Nothing
in a normal editing session surfaces it, which is a plausible contributor to
the drift going unnoticed across several releases (v0.8 through v0.20 all
added or changed config options without anyone hitting a link to "here's
where the schema lives and here's when it needs updating"). Rather than
folding its content into `README.md` (that would just relocate the same
single-source-of-truth problem — versioning rules kept in sync in two
places instead of one), this release adds links to it from the two spots a
reader would actually be looking: `README.md`'s `### knap.toml` /
`## Configuration` section, and `GETTING_STARTED.md`'s schema section.

No Rust code changes: `InitOptions`, `Config`, and every parser/index/
handler/protocol surface are already correct — this release only touches
the schema file and the docs that describe it.

---

## Schema Changes

`schemas/v1/initialization_options.json` — no version bump (see Goal):

- **Remove** the `attachmentsDir` property. It was removed from
  `InitOptions` in v0.2 and `additionalProperties: false` means the schema
  currently rejects nothing extra for it, it's just dead weight describing
  a key `knap lsp` no longer reads.
- **Add** three top-level properties, mirroring `docs/GETTING_STARTED.md`'s
  options table and `InitOptions`' `exclude`/`skip_dirs`/
  `ignore_link_targets` fields:
  - `exclude: string[]` — glob patterns excluded from indexing.
  - `skipDirs: string[]` — glob patterns matched against bare directory
    names, pruned from the crawl.
  - `ignoreLinkTargets: string[]` — glob patterns matched against a link's
    raw target text, suppressing `broken-link` diagnostics.
- **Replace** `frontmatterSchema`'s nested shape. Current (wrong):

  ```json
  "frontmatterSchema": {
    "properties": {
      "additionalProperties": {
        "properties": { "enum": { "type": "array", "items": { "type": "string" } } }
      }
    },
    "required": { "type": "array", "items": { "type": "string" } }
  }
  ```

  Fixed, matching `FrontmatterSchemaJsonOpts` (`src/config/mod.rs`) and
  `docs/GETTING_STARTED.md`'s frontmatter-schema table:

  ```json
  "frontmatterSchema": {
    "type": "object",
    "description": "Defines allowed keys and values for a doc's frontmatter. Enables key/value completions and diagnostics.",
    "properties": {
      "requireFrontmatter": {
        "type": "boolean",
        "default": false,
        "description": "Warn on docs that have no '---' frontmatter block at all when required keys exist."
      },
      "warnOnUnknownKeys": {
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
  ```

`title`, `description`, `$schema` (draft-07), and the unaffected top-level
properties (`extensions`, `newNoteDir`) are unchanged.

---

## Docs Changes

`docs/GETTING_STARTED.md` — "Schema (Zed / JSON-aware editors)" section:

- Both mentions of `schemas/initialization_options.json` (prose and the
  `file:///path/to/knap/...` example) become
  `schemas/v1/initialization_options.json`, matching the file's actual
  location and `schemas/README.md`'s documented URL.
- Add a link to `schemas/README.md` in the same section, so a reader who
  finds the schema also finds the versioning rules governing it.

`README.md` — `### knap.toml` section (or `## Configuration` intro, whose
sources list already names `initializationOptions`/`knap.toml`):

- Add a one-line pointer to `schemas/README.md` alongside the existing
  mention of the two config sources, so the schema's versioning contract is
  reachable from the primary README, not just `GETTING_STARTED.md`.

---

## Testing

Pure JSON/Markdown content — no Rust types change, so no `cargo test`
coverage is possible in the usual sense (no function to unit-test). The
regression test instead pins the schema file's own key sets, so a future
edit that reintroduces drift between `schemas/v1/initialization_options.json`
and the documented wire shape fails loudly.

### Integration tests (`tests/schema.rs`, new file)

Only touch the JSON file's own content — no crate access needed, since
`InitOptions` is `pub(crate)`-private.

| Test                                                     | What it verifies                                                                                          |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------- |
| `schema_is_valid_json`                                       | `schemas/v1/initialization_options.json` parses as JSON                                                    |
| `schema_top_level_properties_match_wire_contract`             | Top-level `properties` keys are exactly `{$schema, extensions, newNoteDir, frontmatterSchema, exclude, skipDirs, ignoreLinkTargets}` — no `attachmentsDir`, nothing missing |
| `schema_frontmatter_schema_properties_match_wire_contract`    | `frontmatterSchema.properties` keys are exactly `{requireFrontmatter, warnOnUnknownKeys, fields}` — not `{properties, required}` |
| `schema_frontmatter_schema_fields_entry_properties_match_wire_contract` | `frontmatterSchema.properties.fields.additionalProperties.properties` keys are exactly `{required, values}` |

### Unit tests (`src/config/tests.rs`)

`InitOptions` is private to the `config` module, so the doc/struct
round-trip check lives here instead of in the integration test above.

| Test                                                     | What it verifies                                                                                          |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------- |
| `init_options_accepts_getting_started_frontmatter_schema_example` | The exact `frontmatterSchema` example JSON block from `docs/GETTING_STARTED.md` deserializes into `InitOptions` without error — the doc's example and the struct agree |
