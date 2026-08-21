# v0.23 Design — `lint --suggest` Text Output

Covers the stories in the v0.23 release:

| Story | Feature                                                                 |
| ----- | ------------------------------------------------------------------------ |
| #74   | `knap lint --suggest` prints ranked candidate fixes in text-mode output, not only `--json` (Bug) |

---

## Goal

A writer running `knap lint --suggest` from a terminal — the natural first
thing to try, per the flag's own help text — gets zero visible indication
the flag did anything: `compute_diagnostics_with_suggestions` always computes
ranked candidates and attaches them to each `broken-link`/`broken-anchor`
diagnostic's `data` field, but the text-mode print loop in `src/cli/lint.rs`
only ever reads `d.message`, never `d.data`. `--suggest` is currently only
observable in `--json` mode, which isn't documented or enforced anywhere —
`clap`'s `suggest` arg has no `requires = "json"`, and its help text doesn't
mention JSON as a prerequisite.

This fixes it by printing each diagnostic's suggestions as indented lines
under the diagnostic in text mode too, so `--suggest` is useful standalone,
matching how the flag is documented today (no JSON requirement stated). No
new data is computed — `compute_diagnostics_with_suggestions` already
produces everything needed; this is purely a CLI-output-shaping fix confined
to `src/cli/lint.rs`.

---

## CLI Changes

`src/cli/lint.rs`'s text-mode print loop gains a helper that reads a
diagnostic's existing `data` field (already populated by
`compute_diagnostics_with_suggestions` when `--suggest` is passed) and
prints each ranked suggestion as an indented line immediately below the
diagnostic:

```rust
/// Prints each ranked suggestion in `data.suggestions` (as attached by
/// `compute_diagnostics_with_suggestions`) as an indented line under the
/// diagnostic that owns it, plus a one-line note when `data.text_mismatch`
/// is set. No-op if `data` is absent or has no `suggestions` array — i.e.
/// `--suggest` wasn't passed, or this diagnostic isn't a broken-link/
/// broken-anchor.
fn print_suggestions(data: &serde_json::Value) {
    let Some(suggestions) = data.get("suggestions").and_then(|s| s.as_array()) else {
        return;
    };
    for s in suggestions {
        let target = s.get("target").and_then(|t| t.as_str()).unwrap_or("?");
        let distance = s.get("distance").and_then(|d| d.as_u64()).unwrap_or(0);
        match s.get("text_distance").and_then(|d| d.as_u64()) {
            Some(td) => println!("    -> {target} (distance {distance}, text distance {td})"),
            None => println!("    -> {target} (distance {distance})"),
        }
    }
    if data.get("text_mismatch").and_then(|v| v.as_bool()) == Some(true) {
        println!(
            "    (top match by distance differs from best text match — verify before applying)"
        );
    }
}
```

Called from the existing text-mode loop, immediately after the diagnostic's
own line:

```rust
for file in &files {
    for d in &file.diagnostics {
        println!(
            "{}:{}:{}: {}: {}",
            file.path.display(),
            d.range.start.line + 1,
            d.range.start.character + 1,
            severity_label(d.severity),
            d.message,
        );
        if let Some(data) = &d.data {
            print_suggestions(data);
        }
    }
}
```

Sample output:

```
$ knap lint . --suggest
docs/index.md:12:3: warning: broken link to 'docs/missing.md'
    -> docs/found.md (distance 2, text distance 0)
    -> docs/other.md (distance 4, text distance 6)
    (top match by distance differs from best text match — verify before applying)

1 problem(s) in 1 file(s)
```

No change to `--json` output — `LintReport`'s `Diagnostic::data` field is
already serialized as-is; this only teaches the text branch to read the same
field the JSON branch already exposes.

**Help text:** `src/cli/mod.rs`'s `suggest` arg doc comment currently reads
"as `data.suggestions` in --json output" — drop the "`--json` output"
qualifier since it no longer describes the actual behavior:

```rust
/// Attach up to N ranked candidate fixes to each broken-link or
/// broken-anchor diagnostic — printed as indented lines in text output, or
/// as `data.suggestions` in --json output. Closest match first. Bare
/// `--suggest` defaults to 3; omit to skip.
```

**README:** `## Linting` section's `--suggest` bullet currently only shows
the `--json --suggest` example. Add a short text-mode example alongside it
so the docs no longer imply `--json` is required.

---

## Testing

### Unit tests

None — `print_suggestions` is a thin `println!` formatter over already-tested
data (`compute_diagnostics_with_suggestions`'s suggestion computation and
ranking are covered by existing `src/handlers.rs` unit tests, unchanged by
this fix). Its behavior is verified end-to-end by the integration test below.

### Integration tests (`tests/cli.rs`)

| Test                                                        | What it verifies                                                                 |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `lint_suggest_prints_candidates_in_text_mode`                 | `knap lint . --suggest` (no `--json`) prints at least one `    -> <target> (distance ...)` line under a `broken-link` diagnostic |
| `lint_suggest_text_mode_notes_text_mismatch`                   | `knap lint . --suggest` on the `fix_text_mismatch_link` fixture prints the "verify before applying" note line |
| `lint_without_suggest_prints_no_candidate_lines`               | `knap lint .` (no `--suggest`) prints diagnostic lines with no indented `-> ` lines following (regression guard: bare `lint` output is unchanged) |
