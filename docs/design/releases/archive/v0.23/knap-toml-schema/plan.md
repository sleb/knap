# v0.23 Implementation Plan — knap.toml Schema

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

This is a docs/schema-only feature (GitHub issue #75) — no Rust production
code changes. `KnapToml` already has the shape the new schema describes; each
step's tests are written first and confirmed to fail (Step 1) or pass on the
first try because the underlying struct was already right (Step 1's unit
test), same posture as v0.22's schema-sync release.

---

## Status

| Step                                                      | Status | Notes |
| ----------------------------------------------------------- | ------ | ----- |
| 1 — Regression tests for the new schema                     | Done   |       |
| 2 — Add the schema file                                     | Done   |       |
| 3 — Document the schema and how to wire it up with taplo    | Done   |       |

---

## Step 1 — Regression tests for the new schema

Write every test first, against a `schemas/v1/knap_toml.json` that doesn't
exist yet, and confirm the integration tests fail for that reason (file not
found / empty), not for an unrelated one. This is what proves the tests
actually pin the schema's shape once Step 2 adds it.

**Deliverables:**

- Extend `tests/schema.rs` with a `knap_toml_schema_json()` helper (same
  shape as the existing `schema_json()`, reading `schemas/v1/knap_toml.json`
  instead) and four new tests: `knap_toml_schema_is_valid_json`,
  `knap_toml_schema_top_level_properties_match_wire_contract`,
  `knap_toml_schema_frontmatter_schema_properties_match_wire_contract`,
  `knap_toml_schema_frontmatter_schema_fields_entry_properties_match_wire_contract`.
- New test in `src/config/tests.rs`: `knap_toml_accepts_readme_example` —
  parses the literal `knap.toml` example block from `README.md`'s `###
  knap.toml` section with `toml::from_str::<KnapToml>` and asserts it
  succeeds with the expected `extensions`, `skip_dirs`, and
  `frontmatter_schema.fields` values. This one should already pass —
  `KnapToml` is already correct — and exists to catch this example ever
  drifting from the struct in the future, not to reproduce a bug.

**Unit tests:**

| Test                                | What it verifies                                                            |
| -------------------------------------- | ---------------------------------------------------------------------------- |
| `knap_toml_accepts_readme_example`     | The `README.md` `### knap.toml` example deserializes into `KnapToml` correctly |

**Integration tests (written now, run against the missing file to confirm they fail):**

| Test                                                                              | What it verifies                                                                                              |
| ------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| `knap_toml_schema_is_valid_json`                                                     | `schemas/v1/knap_toml.json` parses as JSON — **expected to fail**: file doesn't exist yet                        |
| `knap_toml_schema_top_level_properties_match_wire_contract`                          | Top-level `properties` keys match `KnapToml`'s wire fields — **expected to fail**: same reason                   |
| `knap_toml_schema_frontmatter_schema_properties_match_wire_contract`                 | `frontmatter_schema.properties` keys are `{require_frontmatter, warn_unknown_keys, fields}` — **expected to fail** |
| `knap_toml_schema_frontmatter_schema_fields_entry_properties_match_wire_contract`    | `fields`'s value-object keys are `{required, values}` — **expected to fail**                                     |

Run `cargo test schema` and confirm all four new integration tests fail
(file-not-found, not an assertion mismatch) and `knap_toml_accepts_readme_example`
passes, before moving to Step 2.

> **Manual checkpoint:** No editor checkpoint — this step only adds tests,
> content is added in Step 2.

---

## Step 2 — Add the schema file

Add `schemas/v1/knap_toml.json`, per the exact JSON in `design.md`'s Schema
Changes section.

**Deliverables:**

- New `schemas/v1/knap_toml.json`: `extensions`, `new_note_dir`, `exclude`,
  `skip_dirs`, `ignore_link_targets` top-level properties, plus
  `frontmatter_schema` with its `require_frontmatter`/`warn_unknown_keys`/
  `fields` nested shape (`fields`' per-key shape: `required`/`values`).
  `additionalProperties: false` at every level.

**Unit tests:** none new — Step 1's tests cover this step's correctness.

Run `cargo test schema` and confirm all four new `tests/schema.rs` tests now
pass. Run `cargo clippy -- -D warnings`.

> **Manual checkpoint:** In an editor with taplo TOML support (e.g. Zed),
> open a scratch `knap.toml` with `#:schema
> file:///<repo>/schemas/v1/knap_toml.json` as its first line, then type a
> `[frontmatter_schema]` block using `require_frontmatter`/
> `warn_unknown_keys`/`fields.<key>.required`/`fields.<key>.values` — confirm
> it validates clean and typing inside `fields.<key>` offers `required`/
> `values` completions. Then add a bogus camelCase key (e.g.
> `warnUnknownKeys`) — confirm the editor flags it as invalid.

---

## Step 3 — Document the schema and how to wire it up with taplo

Add the taplo-facing docs described in `design.md`'s Docs Changes section:
`schemas/README.md` names both schema files, `docs/GETTING_STARTED.md` gets
a new "Schema (taplo / TOML-aware editors)" subsection, and `README.md`'s
`### knap.toml` section points at the new file.

**Deliverables:**

- `schemas/README.md`: "Current version" lists both `schemas/v1/
  initialization_options.json` and `schemas/v1/knap_toml.json` with their raw
  GitHub URLs.
- `docs/GETTING_STARTED.md`, `## 3. Configuration`: new "Schema (taplo /
  TOML-aware editors)" subsection after the existing "Schema (Zed /
  JSON-aware editors)" one, showing both the inline `#:schema` directive and
  a `taplo.toml` glob association, per the exact examples in `design.md`.
  Links to `schemas/README.md`.
- `README.md`, `### knap.toml` section: extend the existing schema pointer
  to also name `schemas/v1/knap_toml.json`, with a one-clause mention of the
  `#:schema` directive.

**Unit tests:** none — this is a prose/example addition with no executable
surface beyond what Step 1 already guards (the `#:schema` URL isn't tested
directly, but the schema file it points at is pinned by `tests/schema.rs`).

> **Manual checkpoint:** Read the rendered `docs/GETTING_STARTED.md`
> "Schema (taplo / TOML-aware editors)" section and `README.md`'s `###
> knap.toml` section on GitHub (or a local Markdown preview). Confirm the
> `#:schema` example and the `taplo.toml` example both reference
> `schemas/v1/knap_toml.json`, and confirm both sections link to
> `schemas/README.md`.

---

## Done — v0.23 complete

| Story | Feature                                                                                       | Delivered in step |
| ----- | ------------------------------------------------------------------------------------------------ | ------------------ |
| US-61 | Autocompletion and validation for `knap.toml` in taplo-aware editors, via a published JSON Schema (#75) | Step 2 (schema file), Step 1 (regression coverage), Step 3 (taplo wiring docs) |
