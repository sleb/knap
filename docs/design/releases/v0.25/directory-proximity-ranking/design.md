# v0.25 Design — Directory-Proximity Ranking

Covers the stories in this feature (bundled into the v0.25 release alongside
`skill-restructure`):

| Story  | Feature                                                            |
| ------ | ------------------------------------------------------------------- |
| US-D28 | Directory-proximity signal in `knap lint --suggest`'s link ranking |
| US-D29 | `same_basename` flag on `--suggest` diagnostics                    |

---

## Goal

An agent repointing a broken link with `knap lint --suggest` can trust that
the top-ranked candidate is at least the best the ranking *can* tell — and
knows when it can't tell at all. Trial 10 of the [agentic efficiency
benchmark](../../../experiments/agentic-efficiency-benchmark.md) (see
"Opportunities for improvement surfaced by Trial 10") found a reproducible
ranking gap distinct from the one US-D20 (v0.16) closed:
when several candidates share a file's basename, raw path-edit-distance
rewards a candidate that keeps the broken link's `../<dir>/` prefix shape
over the correct same-directory candidate that drops it, and since every
same-basename candidate ties on `text_distance` too (US-D20's signal), there
is nothing in the ranking to break the tie correctly. Two knap-assisted runs
in Trial 10 picked a wrong-directory decoy over the correct same-directory
target for exactly this reason.

This feature adds directory proximity as a third, independent ranking signal
(US-D28) and makes the tied-basename shape visible to the caller instead of
silently indistinguishable (US-D29), so an agent that can't trust ranking
order alone at least knows it, the same shift US-D20's `text_mismatch` flag
made for the text-disagreement case.

---

## Handler Changes

### `dir_distance()` — new, `src/handlers.rs`

Factored out of `relative_path`'s existing "how many `..` levels" arithmetic
so it can be used as an independent ranking signal, not just embedded in the
string it builds:

```rust
/// Directory levels from_dir must climb before it can descend to to's
/// parent — 0 when to lives in from_dir itself. The same "up" count
/// relative_path already computes internally, factored out so ranking can
/// use it as a signal independent of the string it produces.
fn dir_distance(from_dir: &Path, to: &Path) -> usize {
    let from: Vec<Component> = from_dir.components().collect();
    let to_comps: Vec<Component> = to.components().collect();
    let common = from
        .iter()
        .zip(to_comps.iter())
        .take_while(|(a, b)| a == b)
        .count();
    from.len() - common
}
```

`relative_path` is refactored to call `dir_distance` for its own "how many
`..` to emit" count instead of recomputing `common` inline — same output,
one source of truth for the up-count.

### `combined_distance()` — generalized to a third optional term

Currently blends two optional-weighted terms (path always present, text
optional) with a hardcoded 50/50 split when both are present, falling back to
path alone when text is `None`. Generalizing to a third term needs the
fallback logic to generalize too — from "two known cases" to "however many
terms are actually present, weighted average of just those":

```rust
const PATH_WEIGHT: f64 = 1.0 / 3.0;
const TEXT_WEIGHT: f64 = 1.0 / 3.0;
const DIR_WEIGHT: f64 = 1.0 / 3.0;

/// Normalize a directory-level count to roughly [0, 1) — 0 for the same
/// directory, approaching 1 as the candidate gets further away — so it's
/// comparable in magnitude to the other two normalized terms regardless of
/// how deep the vault's directory tree happens to be.
fn normalized_dir_distance(levels: usize) -> f64 {
    let levels = levels as f64;
    levels / (levels + 1.0)
}

fn combined_distance(
    path_distance: usize,
    path_a: &str,
    path_b: &str,
    text_distance: Option<usize>,
    text_a: &str,
    text_b: &str,
    dir_distance: Option<usize>,
) -> f64 {
    let mut weighted_sum = PATH_WEIGHT * normalized_distance(path_distance, path_a, path_b);
    let mut weight_total = PATH_WEIGHT;
    if let Some(d) = text_distance {
        weighted_sum += TEXT_WEIGHT * normalized_distance(d, text_a, text_b);
        weight_total += TEXT_WEIGHT;
    }
    if let Some(levels) = dir_distance {
        weighted_sum += DIR_WEIGHT * normalized_dir_distance(levels);
        weight_total += DIR_WEIGHT;
    }
    weighted_sum / weight_total
}
```

With equal thirds, this reduces to exactly today's behavior in both existing
cases — `dir_distance: None` (anchors, no directory concept) collapses back
to the current path/text 50/50 blend; `text_distance: None` (no usable link
text) collapses back to path alone — so `rank_anchor_candidates` and the
no-link-text path through `rank_link_candidates` are unaffected. No trial
evidence yet favors a different split than equal thirds; a future trial
re-run is what would justify moving these, same caveat the existing
`PATH_WEIGHT`/`TEXT_WEIGHT` comment already carries.

### `rank_link_candidates()` — passes the new signal

```rust
fn rank_link_candidates(
    broken_target: &str,
    link_text: &str,
    source: &Path,
    index: &NoteIndex,
) -> Vec<RankedCandidate<String>>
```

Unchanged signature. Internally, each candidate now also computes
`dir_distance(source_dir, &n.path)` and passes it through to
`combined_distance`. `rank_anchor_candidates` is untouched — headings within
one file have no directory to be near or far from, so it keeps calling
`combined_distance` with `dir_distance: None`.

`RankedCandidate<T>` gains a field so `text_mismatch`-style callers and tests
can inspect the raw signal directly, same as `path_distance`/`text_distance`
are exposed today:

```rust
struct RankedCandidate<T> {
    candidate: T,
    path_distance: usize,
    text_distance: Option<usize>,
    dir_distance: Option<usize>, // None for anchor candidates
    combined: f64,
}
```

### `same_basename()` — new, `src/handlers.rs`

```rust
/// True when >=2 of suggestions' targets share a file stem — the shape
/// that makes text_distance (and dir_distance) tied across every candidate
/// that matters, so ranking order alone carries no signal for which one is
/// correct. Only meaningful for broken-link suggestions (target is a
/// relative path); broken-anchor suggestions never set this.
fn same_basename(suggestions: &[FixSuggestion]) -> bool {
    let mut seen = HashSet::new(); // already imported at the top of handlers.rs
    suggestions
        .iter()
        .any(|s| !seen.insert(file_stem(Path::new(&s.target))))
}
```

Checked against the `top_n` suggestions actually returned (what the caller
can see), not the full ranked list — matches how an agent reading
`--suggest`'s output would notice the collision itself, just made explicit
instead of implicit.

### `compute_diagnostics_with_suggestions()` — attaches the new flag

```rust
pub(crate) fn compute_diagnostics_with_suggestions(
    path: &Path,
    index: &NoteIndex,
    config: &crate::config::Config,
    top_n: usize,
) -> Vec<Diagnostic>
```

Signature unchanged. In the `broken-link` branch, after building
`suggestions: Vec<FixSuggestion>`, also compute
`same_basename(&suggestions)` and — when `true` — set
`data["same_basename"] = true` alongside the existing `text_mismatch`
handling (both flags are independent and can both be present: a same-
basename tie can still have a text-signal disagreement layered on top).
`broken-anchor` suggestions never set `same_basename` — Trial 10's failure
shape is link-target-specific (multiple files sharing a name), not something
headings exhibit the same way.

`FixSuggestion` itself is unchanged (`same_basename` is diagnostic-level, not
per-suggestion, mirroring `text_mismatch`'s placement) — no `distance`/
`text_distance` field changes.

### CLI: `print_suggestions()` (`src/cli/lint.rs`)

Add a line mirroring the existing `text_mismatch` note, printed when
`data.same_basename` is `true`:

```
    (top suggestions share a filename — path ranking alone can't tell them apart; verify by directory or content)
```

---

## Documentation Changes

- **`docs/design/components/handlers.md`** — extend the "Text-aware ranking"
  section (currently titled for `RankedCandidate`/`combined_distance()`/
  `unambiguous_winner()`/`text_mismatch()`, v0.16) to also cover
  `dir_distance()`, the generalized `combined_distance()` signature, and
  `same_basename()`; update `compute_diagnostics_with_suggestions()`'s doc
  entry to mention the `same_basename` flag alongside `text_mismatch`.
- **`skills/knap/reference/lint.md`** — the "`--suggest`: how to pick, not
  just that you must" section gains:
  - A paragraph on `data.same_basename: true`, parallel to the existing
    `text_mismatch` paragraph: when set, the visible candidates' path/text
    signals are tied and can't discriminate — don't pick the
    least-implausible one.
  - New guidance for the case neither existing paragraph covers: when
    *every* visible suggestion looks wrong (not just tied), don't settle —
    list files by that basename directly (`knap index --json` or `grep`)
    before repointing. This is Trial 10's Opportunity 3, addressing the
    `gateway.md` case where both top-ranked candidates were wrong-directory
    decoys and neither the agent nor the (pre-fix) ranking had a signal that
    the actual answer wasn't even in the visible list.
  - A new `--suggest` JSON example showing `same_basename: true` on a
    diagnostic, alongside the existing "ambiguous fix" and "text_mismatch"
    examples.
- **`docs/USER_STORIES.md`** — add US-D28 and US-D29 under the "Fixing"
  section, immediately after US-D20 (same section, chronological order).
- **`CHANGELOG.md`** — new entry under the `v0.25` heading (alongside
  `skill-restructure`'s entries): `knap lint --suggest`'s link ranking now
  weighs directory proximity to the linking file as a third signal, and
  diagnostics carry a new `same_basename` flag when returned suggestions
  share a filename.

---

## Testing

### Unit tests (`src/handlers.rs`)

| Test                                                                          | What it verifies                                                                                              |
| ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| `dir_distance_same_directory_is_zero`                                        | `dir_distance` returns `0` when `to`'s parent equals `from_dir`                                                |
| `dir_distance_counts_up_levels`                                              | `dir_distance` returns `1`/`2` for one/two parent-directory climbs                                             |
| `relative_path_same_dir` / `relative_path_parent_dir` / `relative_path_subdirectory` | existing tests (unmodified) still pass — confirms the `dir_distance` refactor didn't change `relative_path`'s output |
| `combined_distance_with_all_three_terms_averages_equally`                    | equal `PATH_WEIGHT`/`TEXT_WEIGHT`/`DIR_WEIGHT` thirds produce the plain average when all three terms are `Some` |
| `combined_distance_falls_back_to_path_and_text_when_dir_distance_is_none`    | `dir_distance: None` reproduces the exact pre-change 50/50 path/text blend (anchor case)                       |
| `combined_distance_falls_back_to_path_and_dir_when_text_distance_is_none`    | `text_distance: None` with `dir_distance: Some` still blends path+dir 50/50, not path alone                    |
| `rank_link_candidates_prefers_same_directory_over_closer_raw_path_distance` | reproduces the Trial 10 `gateway.md` shape directly: three same-basename candidates at different directory depths, asserts the same-directory one ranks first despite worse raw `path_distance` |
| `rank_anchor_candidates_unaffected_by_dir_distance_signal`                  | anchor ranking output is byte-identical before/after this change (no directory concept applies)                |
| `same_basename_true_when_two_suggestions_share_a_stem`                       | `same_basename` returns `true` for `["../notes/storage.md", "storage.md"]`-shaped input                       |
| `same_basename_false_when_all_targets_distinct`                              | `same_basename` returns `false` when every target has a unique stem                                            |
| `same_basename_false_for_single_suggestion`                                  | `same_basename` returns `false` with fewer than 2 suggestions (nothing to collide with)                        |
| `compute_diagnostics_with_suggestions_sets_same_basename_flag`               | `data.same_basename: true` appears on a `broken-link` diagnostic whose top-`N` suggestions share a stem         |
| `compute_diagnostics_with_suggestions_omits_same_basename_when_absent`      | `data` has no `same_basename` key when suggestions' stems are all distinct (mirrors the existing `text_mismatch` omission test) |
| `compute_diagnostics_with_suggestions_never_sets_same_basename_for_anchors` | `broken-anchor` diagnostics never carry `same_basename`, even with duplicate heading text in the target note   |

### Unit tests (`src/cli/lint.rs`)

| Test                                    | What it verifies                                                                                    |
| ------------------------------------------ | ---------------------------------------------------------------------------------------------------------- |
| `print_suggestions_notes_same_basename` | the same-basename note line appears in non-JSON `knap lint --suggest` output when `data.same_basename` is set, matching whatever test pattern the existing `text_mismatch` CLI note uses |

### Integration tests

No new integration test file — this is a pure ranking/data-shape change
covered end-to-end by the existing `knap lint --suggest --json` CLI tests
plus the unit tests above. `cargo test` and `cargo clippy -- -D warnings`
must be clean.
