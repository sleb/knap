# v0.22 Implementation Plan — Schema Sync

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

This is a docs/schema-only bugfix (GitHub issue #72) — no Rust production
code changes. Every step below is TDD in the sense that its regression test
is written and confirmed failing against the current, stale content before
the content is fixed.

---

## Status

| Step                                        | Status | Notes |
| -------------------------------------------- | ------ | ----- |
| 1 — Regression tests for the schema drift    | Todo   |       |
| 2 — Fix the schema file                      | Todo   |       |
| 3 — Fix stale schema path, link `schemas/README.md` | Todo   |       |

---

## Step 1 — Regression tests for the schema drift

Write every test first, against the current (broken) `schemas/v1/
initialization_options.json`, and confirm each one fails for the reason
the issue describes — not for an unrelated reason (e.g. a typo in the test's
expected key set). This is what proves the tests actually cover the bug.

**Deliverables:**

- New `tests/schema.rs` with the three top-level content-drift tests. Each
  reads `schemas/v1/initialization_options.json` (via
  `env!("CARGO_MANIFEST_DIR")`-relative path, same pattern other integration
  tests in `tests/` use for fixture files), parses it with `serde_json`, and
  compares an exact `HashSet<&str>` of property keys at the relevant nesting
  level against the expected wire-contract set.
- New test in `src/config/tests.rs`:
  `init_options_accepts_getting_started_frontmatter_schema_example` —
  deserializes the literal JSON block from `docs/GETTING_STARTED.md`'s
  "Frontmatter schema" section into `InitOptions` and asserts it succeeds
  with the expected `fields`/`requireFrontmatter`/`warnOnUnknownKeys`
  values. This one should already pass — `InitOptions` is already correct —
  and exists to catch this specific example ever drifting from the struct
  in the future, not to reproduce #72 itself.

**Unit tests:**

| Test                                                                    | What it verifies                                                                 |
| ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| `init_options_accepts_getting_started_frontmatter_schema_example`             | The `docs/GETTING_STARTED.md` `frontmatterSchema` example deserializes into `InitOptions` correctly |

**Integration tests (written now, run against unfixed schema to confirm they fail):**

| Test                                                                       | What it verifies                                                                                          |
| -------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `schema_is_valid_json`                                                            | `schemas/v1/initialization_options.json` parses as JSON — expected to already pass                        |
| `schema_top_level_properties_match_wire_contract`                                 | Top-level `properties` keys exactly match `InitOptions`' wire fields — **expected to fail**: current file has `attachmentsDir` and lacks `exclude`/`skipDirs`/`ignoreLinkTargets` |
| `schema_frontmatter_schema_properties_match_wire_contract`                        | `frontmatterSchema.properties` keys are `{requireFrontmatter, warnOnUnknownKeys, fields}` — **expected to fail**: current file has `{properties, required}` |
| `schema_frontmatter_schema_fields_entry_properties_match_wire_contract`           | `fields`'s value-object keys are `{required, values}` — **expected to fail**: this branch doesn't exist yet in the current file |

Run `cargo test schema` and confirm `schema_top_level_properties_match_wire_contract`,
`schema_frontmatter_schema_properties_match_wire_contract`, and
`schema_frontmatter_schema_fields_entry_properties_match_wire_contract` fail
against the current file, and `schema_is_valid_json` plus the
`init_options_accepts_getting_started_frontmatter_schema_example` unit test
pass, before moving to Step 2.

> **Manual checkpoint:** No editor checkpoint — this step only adds tests,
> content is fixed in Step 2.

---

## Step 2 — Fix the schema file

Rewrite `schemas/v1/initialization_options.json` to match the real wire
contract, per the Schema Changes section of `design.md`. This stays `v1` —
per `schemas/README.md`'s versioning rules this is a same-version fix, not a
breaking change (see `design.md`'s Goal section for why: the wire shape
never actually changed, only the schema file's description of it was ever
wrong).

**Deliverables:**

- `schemas/v1/initialization_options.json`:
  - Remove the `attachmentsDir` property.
  - Add `exclude` (`string[]`), `skipDirs` (`string[]`), `ignoreLinkTargets`
    (`string[]`) top-level properties.
  - Replace `frontmatterSchema`'s nested `properties`/`required` shape with
    `requireFrontmatter`/`warnOnUnknownKeys`/`fields`, and `fields`'s
    per-key shape with `required`/`values`, per the exact JSON in
    `design.md`.
  - Keep `additionalProperties: false` at every level that already has it.

**Unit tests:** none new — Step 1's tests cover this step's correctness.

Run `cargo test schema` and confirm all four `tests/schema.rs` tests and the
`init_options_accepts_getting_started_frontmatter_schema_example` unit test
now pass. Run `cargo clippy -- -D warnings`.

> **Manual checkpoint:** In an editor with JSON Schema support (e.g. Zed),
> open a scratch `settings.json` with
> `"$schema": "file:///<repo>/schemas/v1/initialization_options.json"` and a
> `frontmatterSchema` block using `fields`/`values`/`requireFrontmatter`/
> `warnOnUnknownKeys` — confirm it validates clean and typing inside
> `fields.<key>` offers `required`/`values` completions. Then add a bogus
> `frontmatterSchema.properties` key (the old wrong shape) — confirm the
> editor now flags it as invalid.

---

## Step 3 — Fix the stale schema path in docs, and link `schemas/README.md`

Correct `docs/GETTING_STARTED.md`'s "Schema (Zed / JSON-aware editors)"
section, which still points at the pre-`v1/` path, and give
`schemas/README.md` — the doc that actually states the versioning rules —
a link from both places a reader would look for it. It currently has none,
which plausibly let this drift go unnoticed for several releases.

**Deliverables:**

- `docs/GETTING_STARTED.md`: both occurrences of
  `schemas/initialization_options.json` (prose sentence and the
  `file:///path/to/knap/...` JSON example) become
  `schemas/v1/initialization_options.json`; add a link to
  `schemas/README.md` in the same section.
- `README.md`: add a one-line link to `schemas/README.md` in the `###
  knap.toml` section (or the `## Configuration` intro), alongside the
  existing mention of `initializationOptions`/`knap.toml` as the two config
  sources.

**Unit tests:** none — this is a prose/link correction with no executable
surface. No regression test guards it beyond visual review, same posture as
other doc-only fixes in this codebase's history.

> **Manual checkpoint:** Read the rendered `README.md` `## Configuration`
> section and `GETTING_STARTED.md`'s "Schema (Zed / JSON-aware editors)"
> section on GitHub (or a local Markdown preview). Confirm both the prose
> path and the fenced example's `$schema` value read
> `schemas/v1/initialization_options.json`, and confirm both sections now
> link to `schemas/README.md`.

---

## Done — v0.22 complete

| Story | Feature                                                                     | Delivered in step |
| ----- | ---------------------------------------------------------------------------- | ------------------ |
| #72   | `schemas/v1/initialization_options.json` is stale — doesn't match current `frontmatterSchema` shape | Step 2 (fix), Step 1 (regression coverage), Step 3 (stale doc-path fix + `schemas/README.md` discoverability) |
