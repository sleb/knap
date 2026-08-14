# v0.16 Design — Text-Aware Repoint Ranking

Covers the stories in the v0.16 release:

| Story  | Feature                                                                                                                                                                                 |
| ------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| US-D20 | Ranking blends link-text distance with path/slug distance; `text_mismatch` flag on `lint --suggest` output; `knap fix`/`lint --fix` decline to auto-apply when the two signals disagree |

---

## Goal

An agent (or `knap fix`'s own auto-apply) picking a repoint target for a
`broken-link`/`broken-anchor` diagnostic stops trusting a candidate that's
merely closer by raw edit distance to the _broken target/slug string_, when
the link's own visible text names a different candidate entirely.

The [agentic efficiency benchmark](../../../experiments/agentic-efficiency-benchmark.md)'s
Trial 4 found this exact failure: `rank_link_candidates`/`rank_anchor_candidates`
(`src/handlers.rs:1443-1511`) score every candidate purely by edit distance
between the broken string and that candidate's own path/slug — they never
look at the link's own Markdown text (`[Sync 835](...)`) or compare it
against candidate names, even though the parser already carries that text on
every `MarkdownLink`. A Haiku 4.5 agent following the skill's existing
"pick from `data.suggestions`" instruction repointed 4 of 12 broken links to
a same-shape decoy (`sync-800.md` over `sync-835.md`, `index-274.md` over
`workflow.md`, etc.) — every miss had a link-text-vs-target mismatch visible
on read, but nothing in `--suggest`'s output said so, and `knap lint`
reported all four as fully resolved since every target was a real file.

This release adds a second edit-distance signal — link text vs. candidate
name — blends it with the existing path/slug signal into one ranking, and
computes an explicit `text_mismatch` flag when the two signals disagree on
which candidate is best. Both `suggest_link_fix`/`suggest_anchor_fix` (the
functions behind `knap fix`'s and `lint --fix`'s unambiguous-only auto-apply)
and `compute_diagnostics_with_suggestions` (behind `lint --suggest`'s ranked
list) share the same underlying ranking, so the two can't drift — same
guarantee the current path-only ranking already gives.

No parser or index change: `MarkdownLink::text` and `Heading::text` are
already parsed and available at every call site that needs them; this is a
change to `src/handlers.rs`'s ranking functions and their callers only. No
new LSP capability.

---

## Handler Changes

### Combined ranking (`src/handlers.rs`)

A new type holds both signals for one candidate, generic over what the
candidate actually is (a relative path `String` for links, a `&Heading` for
anchors):

```rust
/// One ranked candidate, with both distance signals kept separate so
/// callers can inspect either one — `combined` is what candidates are
/// sorted and auto-apply decisions are made on; `text_distance` is `None`
/// when the link has no usable display text to compare against (empty, or
/// identical to its own target — nothing to signal with).
struct RankedCandidate<T> {
    candidate: T,
    path_distance: usize,
    text_distance: Option<usize>,
    combined: f64,
}
```

A shared normalization + blend helper, used by both ranking functions:

```rust
/// Weight given to the path/slug-distance term vs. the link-text-distance
/// term when blending into `combined`. Equal weight by default — no trial
/// evidence yet favors one signal over the other; a future trial re-run
/// (see the design doc's Open Questions) is what would justify moving this.
const PATH_WEIGHT: f64 = 0.5;
const TEXT_WEIGHT: f64 = 0.5;

/// Normalize a raw edit distance to roughly `[0, 1]` by dividing by the
/// longer of the two compared strings' character counts, so a distance of 2
/// on a 4-character string counts for more than a distance of 2 on a
/// 40-character one — otherwise the text-distance term (usually short link
/// text) would be swamped by the path-distance term (full relative paths)
/// or vice versa, regardless of `PATH_WEIGHT`/`TEXT_WEIGHT`.
fn normalized_distance(distance: usize, a: &str, b: &str) -> f64 {
    let len = a.chars().count().max(b.chars().count()).max(1);
    distance as f64 / len as f64
}

/// Blend a path-distance and an optional text-distance into one score.
/// Falls back to the path term alone when there's no text signal, so a
/// link with unusable display text ranks exactly as it would have before
/// this release.
fn combined_distance(
    path_distance: usize,
    path_a: &str,
    path_b: &str,
    text_distance: Option<usize>,
    text_a: &str,
    text_b: &str,
) -> f64 {
    let path_norm = normalized_distance(path_distance, path_a, path_b);
    match text_distance {
        Some(d) => {
            PATH_WEIGHT * path_norm + TEXT_WEIGHT * normalized_distance(d, text_a, text_b)
        }
        None => path_norm,
    }
}
```

`rank_link_candidates` gains a `link_text: &str` parameter and compares
`slug(link_text)` against `slug(file_stem(candidate_path))` — the file stem
only, not the full relative path, so a deeply-nested candidate isn't
penalized on the text term for its directory depth the way it legitimately
is on the path term:

```rust
fn rank_link_candidates(
    broken_target: &str,
    link_text: &str,
    source: &Path,
    index: &NoteIndex,
) -> Vec<RankedCandidate<String>> {
    let clean_target = index::unescape_link_target(broken_target);
    let source_dir = source.parent().unwrap_or(source);
    let text_slug = slug(link_text);
    let text_signal = (!text_slug.is_empty()).then_some(&text_slug);

    let mut ranked: Vec<RankedCandidate<String>> = index
        .all_notes()
        .filter(|n| n.path != source)
        .map(|n| {
            let rel = relative_path(source_dir, &n.path);
            let path_distance = edit_distance(&clean_target, &rel);
            let stem_slug = slug(file_stem(&n.path));
            let text_distance = text_signal.map(|t| edit_distance(t, &stem_slug));
            let combined = combined_distance(
                path_distance, &clean_target, &rel,
                text_distance, text_slug.as_str(), &stem_slug,
            );
            RankedCandidate { candidate: rel, path_distance, text_distance, combined }
        })
        .collect();
    ranked.sort_by(|a, b| {
        a.combined
            .partial_cmp(&b.combined)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.candidate.cmp(&b.candidate))
    });
    ranked
}
```

`file_stem` is a small new private helper (`Path::file_stem` on the note's
path, lossy-converted to `&str`, empty string on a path with none —
practically unreachable since every indexed note has an extension).

`rank_anchor_candidates` gains the same `link_text: &str` parameter,
comparing `slug(link_text)` against `slug(heading.text)` as its text signal
— the existing `path_distance` term (broken anchor slug vs. heading slug)
is unchanged:

```rust
fn rank_anchor_candidates<'a>(
    broken_slug: &str,
    link_text: &str,
    target_note: &'a parser::Note,
) -> Vec<RankedCandidate<&'a parser::Heading>> {
    let text_slug = slug(link_text);
    let text_signal = (!text_slug.is_empty()).then_some(&text_slug);

    let mut ranked: Vec<RankedCandidate<&Heading>> = target_note
        .headings
        .iter()
        .map(|h| {
            let heading_slug = slug(&h.text);
            let path_distance = edit_distance(broken_slug, &heading_slug);
            let text_distance = text_signal.map(|t| edit_distance(t, &heading_slug));
            let combined = combined_distance(
                path_distance, broken_slug, &heading_slug,
                text_distance, text_slug.as_str(), &heading_slug,
            );
            RankedCandidate { candidate: h, path_distance, text_distance, combined }
        })
        .collect();
    ranked.sort_by(|a, b| {
        a.combined.partial_cmp(&b.combined).unwrap_or(std::cmp::Ordering::Equal)
    });
    ranked
}
```

### Unambiguous winner + `text_mismatch` (`src/handlers.rs`)

A shared helper replaces the inline `match ... .as_slice()` pattern both
`suggest_link_fix` and `suggest_anchor_fix` used before — same
unambiguous-only contract (single candidate, or a strict winner over the
runner-up) plus the new `text_mismatch` gate:

```rust
/// The candidate the ranking is confident enough in to auto-apply, or
/// `None` when it isn't: no candidates, two or more tie on `combined`, or
/// the combined winner disagrees with what the link's own text names
/// (`text_mismatch`) — a confident-looking combined score can still be
/// wrong when the two signals actually point at different candidates and
/// the blend just happened to average out in one direction. Shared by
/// `suggest_link_fix`/`suggest_anchor_fix`; `compute_diagnostics_with_suggestions`
/// computes `text_mismatch` itself since it wants to report it, not act on
/// it.
fn unambiguous_winner<T: PartialEq>(ranked: &[RankedCandidate<T>]) -> Option<&T> {
    let winner = match ranked {
        [only] => only,
        [best, next, ..] if best.combined < next.combined => best,
        _ => return None,
    };
    if text_mismatch(ranked) {
        return None;
    }
    Some(&winner.candidate)
}

/// True when the top-`combined`-ranked candidate isn't also the top
/// candidate by text distance alone — i.e. the link's own visible text
/// points somewhere the blended ranking didn't land. `false` (never a
/// mismatch) when no candidate has a text signal (empty/unusable link
/// text) — there's nothing to disagree with.
fn text_mismatch<T: PartialEq>(ranked: &[RankedCandidate<T>]) -> bool {
    let Some(top_combined) = ranked.first() else {
        return false;
    };
    if top_combined.text_distance.is_none() {
        return false;
    }
    ranked
        .iter()
        .min_by_key(|c| c.text_distance.unwrap_or(usize::MAX))
        .is_some_and(|top_text| top_text.candidate != top_combined.candidate)
}
```

`suggest_link_fix`/`suggest_anchor_fix` become thin wrappers:

```rust
pub(crate) fn suggest_link_fix(
    broken_target: &str,
    link_text: &str,
    source: &Path,
    index: &NoteIndex,
) -> Option<String> {
    unambiguous_winner(&rank_link_candidates(broken_target, link_text, source, index)).cloned()
}

pub(crate) fn suggest_anchor_fix<'a>(
    broken_slug: &str,
    link_text: &str,
    target_note: &'a parser::Note,
) -> Option<&'a parser::Heading> {
    unambiguous_winner(&rank_anchor_candidates(broken_slug, link_text, target_note)).copied()
}
```

Both gain a `link_text: &str` parameter — every existing call site already
has the `MarkdownLink` in scope (`link.text`), so this is a one-line change
at each:

- `src/cli/fix.rs:95` — `handlers::suggest_link_fix(&link.target, &link.text, &note.path, idx)`
- `src/cli/fix.rs:133` — `handlers::suggest_anchor_fix(&slug(anchor), &link.text, target_note)`

### `FixSuggestion` gains `text_distance`; diagnostics gain `text_mismatch` (`src/handlers.rs`)

```rust
#[derive(serde::Serialize)]
struct FixSuggestion {
    target: String,
    distance: usize,             // unchanged: path/slug edit distance vs. the broken string
    text_distance: Option<usize>, // new: edit distance vs. the link's own visible text
}
```

`distance` keeps its existing meaning and JSON shape — no break for a
consumer reading only that field. `text_distance` is additive. The
suggestions list itself is now sorted by `combined` (via
`rank_link_candidates`/`rank_anchor_candidates`'s own sort), not raw
`distance` alone — the two can disagree on order only when a text signal
exists and pulls a candidate up or down, which is exactly the case worth
surfacing.

`compute_diagnostics_with_suggestions` passes `link.text` into both ranking
calls and attaches a top-level `text_mismatch` flag alongside `suggestions`
whenever it's true (never emits `"text_mismatch": false` — same
omit-if-nothing-to-say posture the existing "no `data` at all when zero
candidates" case already takes):

```rust
let ranked = rank_link_candidates(&link.target, &link.text, path, index);
let mismatch = text_mismatch(&ranked);
let suggestions: Vec<FixSuggestion> = ranked
    .into_iter()
    .take(top_n)
    .map(|c| FixSuggestion {
        target: c.candidate,
        distance: c.path_distance,
        text_distance: c.text_distance,
    })
    .collect();
// ...
if !suggestions.is_empty() {
    let mut data = serde_json::json!({ "suggestions": suggestions });
    if mismatch {
        data["text_mismatch"] = serde_json::json!(true);
    }
    d.data = Some(data);
}
```

Same shape change on the `broken-anchor` arm, using `rank_anchor_candidates`
and formatting each winning heading as `#{slug}` exactly as today.

---

## Interaction with the existing unambiguous-only auto-apply contract

`knap fix` and `knap lint --fix` call `suggest_link_fix`/`suggest_anchor_fix`
and apply whatever they return, unchanged — no code in `src/cli/fix.rs`
needs to know about `text_mismatch` directly, because
`unambiguous_winner` folds it into the same `None` result that "ambiguous"
or "no candidates" already produce. This means:

- A repoint that was auto-applied before this release and has no text
  signal available (empty link text) — behavior is unchanged: `combined`
  reduces to the path term alone, and there's nothing to mismatch against.
- A repoint that was auto-applied before this release, had a text signal,
  and the text signal agreed with the path signal — still auto-applied,
  now via `combined` instead of raw `path_distance`, but the winner doesn't
  change since both signals point the same way.
- A repoint that was auto-applied before this release _because_ raw
  `path_distance` had a lone winner, but the link's own text points at a
  different candidate — this is exactly Trial 4's failure mode, and it now
  declines (`None`), falling back to `knap fix`'s existing stub-creation
  path for `broken-link`, or being left alone for `broken-anchor`. This is
  the intended behavior change; the release's whole point is to make this
  case fail closed instead of applying silently.

No change to `compute_create_missing_file_fix`, `compute_link_fix`,
`compute_anchor_fix`, or anything in `src/cli/apply.rs`'s `RepointLink`/
`RepointAnchor` handling from v0.15 — those operations apply whatever
target the caller (agent or `fix`) supplies and still don't themselves
validate it, same posture as before.

---

## Skill Changes

### `skill/knap/SKILL.md`: document `text_distance` and `text_mismatch`

The `--suggest` example (currently showing `{"target": ..., "distance": ...}`
pairs) gains the `text_distance` field and, for the mismatched candidate, a
`text_mismatch: true` sibling of `suggestions` in `data`. The existing "How
to pick, not just that you must" paragraph is extended, not replaced — the
flag is a strengthened signal for exactly the case that paragraph already
warns about by hand, not a replacement for reading the link text:

> `data.text_mismatch: true` means the ranking's own two signals
> disagree — treat it as a hard stop, not a hint: don't repoint from
> `suggestions[0]` when this is set without finding the right target
> yourself (`grep`, `knap index`). Its absence doesn't guarantee the pick is
> right — it only means the two signals agreed, and both can still be wrong
> together — so the existing advice to read the link text against the
> candidate name before repointing still applies to every diagnostic, not
> just flagged ones.

No change to the loop's structure (still `lint --suggest` → pick →
`repoint-*` in an `apply` batch) — this is a change to what the ranked
candidates carry, not to when they're fetched or applied.

---

## Open Questions

- **Weighting.** `PATH_WEIGHT`/`TEXT_WEIGHT` are set to 0.5/0.5 with no
  trial evidence behind the split — a future benchmark re-run (Trial 5, on
  this branch) is what would justify moving them. Kept as named constants,
  not exposed as CLI/config options, so they're one-line to tune without a
  compatibility concern.
- **Margin vs. strict inequality.** `unambiguous_winner` still uses strict
  `<` between the top two `combined` scores, same as the pre-existing
  `path_distance`-only contract — a close-but-not-tied winner still
  auto-applies. Trial 4's Opportunity 3 (widen "unambiguous" from a
  strict-min to a margin) is explicitly out of scope for this release; if
  `text_mismatch` alone doesn't close enough of the gap in a follow-up
  trial, that's the next lever to pull.

---

## Testing

### Unit tests (`src/handlers.rs`)

| Test                                                                  | What it verifies                                                                                                                             |
| --------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `normalized_distance_divides_by_longer_string_length`                 | `normalized_distance(2, "abcd", "wxyz")` == `0.5`; a longer pair with the same raw distance normalizes lower                                 |
| `combined_distance_falls_back_to_path_term_when_no_text_signal`       | `text_distance: None` → `combined` equals the path term alone, unweighted by `TEXT_WEIGHT`                                                   |
| `combined_distance_blends_both_terms_when_text_signal_present`        | `text_distance: Some(_)` → `combined` equals `PATH_WEIGHT * path_norm + TEXT_WEIGHT * text_norm`                                             |
| `rank_link_candidates_orders_by_combined_not_raw_path_distance`       | A candidate with worse raw `path_distance` but a matching link text ranks above a same-shape decoy with better path distance                 |
| `rank_link_candidates_skips_text_term_for_empty_link_text`            | Empty `link_text` → every candidate's `text_distance` is `None`, order matches the pre-release path-only ranking                             |
| `rank_link_candidates_compares_text_against_file_stem_not_full_path`  | A deeply nested candidate's text distance is unaffected by its directory depth                                                               |
| `rank_anchor_candidates_orders_by_combined_including_link_text_term`  | Mirrors the link case: a heading matching link text outranks a same-shape decoy heading with a closer broken-slug distance                   |
| `unambiguous_winner_none_on_tied_combined_score`                      | Two candidates tied on `combined` → `None`, same contract as the pre-release tie case                                                        |
| `unambiguous_winner_none_when_text_mismatch_even_with_strict_winner`  | Combined score has a strict single winner, but `text_mismatch` is true → `None`                                                              |
| `unambiguous_winner_some_when_signals_agree`                          | Combined winner and text-only winner are the same candidate → `Some(winner)`                                                                 |
| `text_mismatch_false_when_no_candidate_has_a_text_signal`             | All `text_distance: None` → `false`, never blocks auto-apply on link text alone                                                              |
| `text_mismatch_true_when_top_combined_and_top_text_disagree`          | Top-`combined` candidate differs from the `min_by_key(text_distance)` candidate → `true`                                                     |
| `suggest_link_fix_declines_the_trial_4_sync_835_case`                 | Regression: broken target near `sync-800.md` by path distance, link text `"Sync 835"` → `suggest_link_fix` returns `None`, not `sync-800.md` |
| `compute_diagnostics_with_suggestions_includes_text_distance_field`   | Every `FixSuggestion` in `data.suggestions` carries `text_distance` (`Some` or `None`)                                                       |
| `compute_diagnostics_with_suggestions_sets_text_mismatch_on_data`     | A diagnostic whose ranking has `text_mismatch` true carries `data.text_mismatch == true`                                                     |
| `compute_diagnostics_with_suggestions_omits_text_mismatch_when_false` | A diagnostic with agreeing signals has no `text_mismatch` key in `data` at all                                                               |

### Integration tests (`tests/cli.rs`)

| Test                                                                 | What it verifies                                                                                                                                      |
| -------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lint_suggest_reports_text_mismatch_for_decoy_and_correct_candidate` | A fixture vault with a `sync-835.md`/`sync-800.md`-shaped decoy pair and a mismatched link text reports `text_mismatch: true` in `--json`             |
| `fix_declines_repoint_when_text_mismatch_leaves_stub_fallback`       | `knap fix` on the same fixture creates a stub instead of repointing to the decoy — the pre-release behavior would have repointed wrongly              |
| `lint_fix_reports_stub_fallback_not_wrong_repoint_for_mismatch_case` | `knap lint --fix --suggest --json` on the same fixture shows the stub in `fixes_applied`, and the still-open diagnostic still carries `text_mismatch` |
