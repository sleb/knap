# v0.23 Implementation Plan — `lint --suggest` Text Output

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the fix should be manually verified against a real
terminal.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                          | Status | Notes |
| ---------------------------------------------- | ------ | ----- |
| 1 — Regression tests + text-mode fix            | Done   |       |
| 2 — Docs                                        | Done   |       |

---

## Step 1 — Regression tests + text-mode fix

This is the whole fix: `src/cli/lint.rs`'s text-mode branch never reads
`d.data`, so `--suggest` has no visible effect without `--json`. Write the
regression tests first, confirm they fail against the current code, then
implement `print_suggestions` and wire it into the text-mode loop.

1. Write `lint_suggest_prints_candidates_in_text_mode`,
   `lint_suggest_text_mode_notes_text_mismatch`, and
   `lint_without_suggest_prints_no_candidate_lines` in `tests/cli.rs` (see
   Deliverables below for exact assertions).
2. Run `cargo test --test cli` and confirm the two `--suggest` tests **fail**
   (the third, `lint_without_suggest_prints_no_candidate_lines`, already
   passes against unfixed code — it's a regression guard, not a red test —
   confirm it passes both before and after).
3. Implement `print_suggestions` in `src/cli/lint.rs` and call it from the
   text-mode loop until all three tests pass, then run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `fn print_suggestions(data: &serde_json::Value)` in `src/cli/lint.rs` —
  reads `data["suggestions"]` (array of `{target, distance, text_distance}`)
  and prints one `    -> {target} (distance {distance}[, text distance
  {text_distance}])` line per entry; prints a trailing `    (top match by
  distance differs from best text match — verify before applying)` line when
  `data["text_mismatch"] == true`. No-op if `data` has no `suggestions` key.
- Text-mode loop in `run()` calls `print_suggestions(data)` for each
  diagnostic where `d.data` is `Some`, immediately after printing the
  diagnostic's own line.
- `src/cli/mod.rs`: `suggest` arg's doc comment updated to drop the
  "in --json output" qualifier (see design doc for exact wording).

**Unit tests:** none for this step — see design doc's Testing section for
why (`print_suggestions` is a thin formatter over already-tested ranking
logic; integration tests below cover its actual output).

**Integration tests** (in `tests/cli.rs`):

| Test                                                        | What it verifies                                                                 |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `lint_suggest_prints_candidates_in_text_mode`                 | `knap lint . --suggest` against the `fix_repoint_broken_link` fixture (no `--json`) prints a line matching `    -> config.md (distance` |
| `lint_suggest_text_mode_notes_text_mismatch`                   | `knap lint . --suggest` against the `fix_text_mismatch_link` fixture prints the literal line `    (top match by distance differs from best text match — verify before applying)` |
| `lint_without_suggest_prints_no_candidate_lines`               | `knap lint .` against the `fix_repoint_broken_link` fixture (no `--suggest`) — stdout contains no line starting with `    -> ` |

> **Manual checkpoint:** In a scratch vault with one broken link, run `knap
> lint . --suggest` in a terminal (no `--json`). Confirm an indented `-> ...
> (distance N)` line appears directly under the `broken-link` diagnostic
> line, and that running plain `knap lint .` (no `--suggest`) shows no such
> line.

---

## Step 2 — Docs

Bring `README.md` in line with the new behavior — the `--suggest` docs
currently only show a `--json --suggest` example, which reads as an implicit
JSON requirement that no longer exists.

**Deliverables:**

- `README.md`'s `## Linting` section, `--suggest` bullet: add a text-mode
  example (`knap lint . --suggest` with sample indented output) alongside
  the existing `--json --suggest` example, per the design doc's sample
  output block.

**Unit tests:** none — prose-only change.

> **Manual checkpoint:** Read the rendered `--suggest` bullet in
> `README.md` on GitHub (or a local Markdown preview). Confirm it no longer
> implies `--json` is required to see suggestions.

---

## Done — v0.23 complete (this feature)

| Story | Feature                                                                 | Delivered in step |
| ----- | -------------------------------------------------------------------------- | ------------------ |
| #74   | `knap lint --suggest` prints ranked candidate fixes in text-mode output, not only `--json` (Bug) | Step 1 (fix + regression tests), Step 2 (docs) |
