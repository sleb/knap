# Benchmark: does knap make agentic Markdown editing faster and more accurate?

A manual A/B protocol for comparing a coding agent editing a linked Markdown
vault **with** knap's headless CLI (`lint`/`index`/`fix`/`rename-*` +
`skill/knap/SKILL.md`) against the same agent editing the same vault with
only generic tools (`grep`/`sed`/`Read`/`Edit`). This is the evidence behind
the README's "efficiently and accurately" claim — it should be re-runnable
whenever the claim needs re-checking (new model, new knap release).

## Hypothesis

For a task that requires touching cross-linked notes (rename, restructure,
retag), the knap-assisted agent:

1. **finishes in fewer tokens and less wall time**, because it doesn't have
   to `grep` the whole vault to enumerate backlinks/anchors by hand, and
2. **leaves fewer broken links/anchors behind**, because it verifies with
   `knap lint` instead of trusting its own `grep` coverage.

Both must hold — a faster agent that leaves broken links doesn't support the
claim, and neither does a slower-but-accurate one.

## Corpus

Real public vaults were tried first and rejected — see
[Real-vault candidates considered](#real-vault-candidates-considered) below.
The consistent failure mode was that a public single-author vault is almost
always a curated excerpt of a private whole, so a large share of its
internal links dangle outside what's public, which kills exactly the
density this benchmark needs. Docs-site repos solve density but use link
formats (absolute site paths, not relative `.md` paths) that knap doesn't
resolve without a rewrite pass big enough that it's simpler to generate.

**Use `examples/gen_bench_vault.rs`** (`cargo run --example gen_bench_vault
-- --out <dir> --seed <n>`) instead. It builds a closed, self-contained
graph deterministically from a seed — no dangling links, exact known
density — with:

- **50 notes** (configurable via `--notes`) across 4 subdirectories
  (`topics/`, `reference/`, `projects/`, `notes/`).
- **A hub/leaf link shape**: ~⅓ of notes are "hubs" that other notes
  preferentially link to (60% of outgoing links target a hub), so renames
  and retags have real, uneven blast radius instead of a flat graph —
  verified on a `--seed 1` run: hub notes carry 8–12 backlinks, leaf notes
  0–2.
- **Frontmatter tags** in all three YAML forms (bare scalar, inline list,
  block list), drawn from a 5-tag pool, each landing on roughly 7–18 notes.
- **Anchor links** to specific headings in the target note (~25% of
  planted links), using the same GFM slug algorithm as `handlers::slug`.
- **Seeded defects** (`--broken-links`, `--broken-anchors`, default 5 and
  3): existing links/anchors mangled into a known-broken form _before_
  rendering, so the "fix" tasks have a known-correct answer. A
  `BENCH_MANIFEST.json` in the output directory records exactly which
  files and targets were broken.

Verified end to end: `knap lint --json` on a generated vault reports
exactly `problem_count: 8` (5 `broken-link` + 3 `broken-anchor`) with zero
false positives on the other ~170 clean links — confirming the generator's
relative-path and slug logic matches knap's actual resolver, not just its
own idea of what should resolve.

Commit a generated snapshot (or regenerate with a pinned `--seed` per
run) as the commit everything resets to, so every trial starts from
byte-identical `git reset --hard`.

### Real-vault candidates considered

| Repo                                                                    | Link format                                     | Verdict                                                                                                                                                                                              |
| ----------------------------------------------------------------------- | ----------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [rust-lang/book](https://github.com/rust-lang/book)                     | plain `.md` relative links                      | compatible, but too sparse — only 6 in-content cross-links across 478 files (linking lives in `SUMMARY.md`'s linear TOC, not chapter content), no frontmatter/tags                                   |
| [kepano/kepano-obsidian](https://github.com/kepano/kepano-obsidian)     | `[[wikilinks]]`, converts via `obsidian-export` | too sparse post-conversion — only 4 real cross-file links survived across 51 notes; most of the vault's wikilinks point to notes outside this public excerpt and got silently stripped to plain text |
| [mdn/content](https://github.com/mdn/content)                           | absolute `/en-US/docs/...` site paths           | incompatible outright — 7,719 links in the `web/css` subtree use absolute URLs, only 3 use a relative `.md` path; would read as ~100% broken to knap without a full path rewrite                     |
| [nikitavoloboev/knowledge](https://github.com/nikitavoloboev/knowledge) | n/a                                             | repo has been repurposed into a Go CLI project; no longer a markdown vault                                                                                                                           |

If a better real candidate turns up later (a large, _complete_ public vault
rather than an excerpt), swap it in — the seeded-defect approach the
generator uses (mangle N existing links/anchors with a fixed RNG seed
before the benchmark's seed commit) applies just as well on top of a real
corpus.

## Task script ("typical agentic editing session")

Run the _same_ ordered list of instructions in both conditions, phrased
identically, with no mention of `knap` in the baseline condition's prompt.
Because the generator's file/heading/tag names are randomized per seed,
resolve the concrete targets once per generated vault (via
`knap index --json`, picking the top-backlink note for steps 1–2, and
`BENCH_MANIFEST.json` for step 6) and hard-code those resolved names into
the actual prompt text used for a run — the instructions must be concrete,
not "rename whichever file has the most links," or the two conditions
aren't doing literally the same task:

1. Rename the note with the most backlinks (a hub, e.g. `router.md` on
   `--seed 1`) to a new, related filename.
2. Rename one of its headings to something else; confirm every anchor
   link to it updates.
3. Rename one of the 5 tags to a new name across the vault.
4. Add two new notes and link them into the existing structure from at
   least one existing note each, including one anchor link.
5. Split one of the hub notes into two notes, updating every backlink
   that pointed at the sections that moved.
6. Fix the 5 seeded broken links and 3 seeded broken anchors listed in
   `BENCH_MANIFEST.json`.
7. Final instruction, identical in both conditions: "confirm there are no
   broken links or anchors left."

Step 7 is the important one: it's what forces the baseline condition to
either trust its own `grep` or spend extra effort re-deriving a check knap
gets for free.

## Conditions

|                               | Baseline                                               | knap-assisted                                                             |
| ----------------------------- | ------------------------------------------------------ | ------------------------------------------------------------------------- |
| Tools available               | `Read`, `Edit`, `Write`, `Bash` (`grep`, `sed`, `git`) | same, plus `knap` binary on `PATH` and `skill/knap/SKILL.md` installed    |
| Prompt                        | task list only                                         | task list only — no mention of knap; the agent discovers it via the skill |
| Verification method available | manual (`grep -rn`, reading files)                     | `knap lint --json`, `knap index <file> --json`                            |

Everything else — model, system prompt, starting repo state — held constant.

## Metrics

Capture these per run:

| Metric                                                     | How to capture                                                                                                                                                                                             |
| ---------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Wall-clock time                                            | timestamp at first tool call → timestamp at final "done" message                                                                                                                                           |
| Total tokens (input + output, cache read/write broken out) | Claude Code's `/cost` or session summary at end of run                                                                                                                                                     |
| Tool-call count                                            | count of tool invocations in the transcript                                                                                                                                                                |
| Files read                                                 | distinct files opened via `Read`/`grep`, whether or not edited (proxy for exploration cost)                                                                                                                |
| **Correctness (primary)**                                  | run `knap lint --json .` **out-of-band**, after the session ends, regardless of condition — count `problem_count` and specifically new broken-link/broken-anchor diagnostics not present in the seeded set |
| Task completion                                            | did all 7 steps actually get applied? (diff against an expected end-state fixture)                                                                                                                         |
| Self-corrections                                           | count of turns where the agent redoes or patches its own prior edit in the same session                                                                                                                    |

Correctness must be measured with the **same external tool** (`knap lint`)
for both conditions — that's what makes it a fair ground truth rather than
"did the agent believe it was done."

## Procedure

1. `git reset --hard bench-vault-seed` before every run (both conditions).
2. Start a **fresh** agent session per run — no conversation carried over
   between runs, and never reuse a session across conditions (avoids
   contamination/learning effects).
3. Run **N ≥ 3 trials per condition** (agent behavior is stochastic; a
   single run each is not enough to trust a comparison). More trials on
   whichever metric shows the widest spread.
4. After each run, independently of the agent's own claims:
   - `knap lint --json .` → correctness metrics.
   - `git diff --stat` against the seed → files-touched / lines-changed,
     as a cross-check on "efficiency" that isn't just token count.
5. Record all metrics in a table per run, then take median (not mean —
   small N, and token/time outliers from one bad tool call shouldn't
   dominate) per condition.

## Reporting

Present as a single comparison table, median across trials:

| Metric                                  | Baseline | knap-assisted | Δ   |
| --------------------------------------- | -------- | ------------- | --- |
| Wall time                               |          |               |     |
| Tokens                                  |          |               |     |
| Tool calls                              |          |               |     |
| Files read                              |          |               |     |
| Broken links/anchors left (`knap lint`) |          |               |     |
| Tasks fully completed (of 7)            |          |               |     |

Call the hypothesis supported only if knap-assisted wins (or ties) on both
tokens/time **and** correctness — a token win that trades away correctness
isn't the claim the README makes.

## Results

### Trial 1 — 2026-08-07, N=1 per condition (smoke test, not the official run)

A single trial per condition, run to validate the protocol itself (corpus,
task script, tooling setup) before spending on the full N≥3 run. **Treat
this as directional only — do not cite it as "knap wins/loses" anywhere.**

Setup: `examples/gen_bench_vault.rs --seed 1` (50 notes, 175 links, 8
seeded defects), copied into two git repos at an identical seed commit.
The `knap` binary was put on `PATH` and `skill/knap/SKILL.md` was
installed at `.claude/skills/knap/SKILL.md` in the knap-assisted repo
only; the baseline repo had neither. Both agents got the exact same task
text (see [Task script](#task-script-typical-agentic-editing-session)),
with no mention of knap in either prompt. Correctness was checked
independently after each run with `knap lint --json` against the vault,
not the agent's self-report.

| Metric                                  | Baseline | knap-assisted | Δ                 |
| --------------------------------------- | -------- | ------------- | ----------------- |
| Wall time                               | 226.2s   | 234.1s        | knap +3.5% slower |
| Tokens                                  | 43,696   | 46,795        | knap +7.1% more   |
| Tool calls                              | 45       | 49            | knap +4 more      |
| Broken links/anchors left (`knap lint`) | 0        | 0             | tie               |
| Tasks fully completed (of 7)            | 7/7      | 7/7           | tie               |

**This trial does not support the hypothesis.** Both vaults ended fully
correct (`problem_count: 0`, independently verified), so correctness tied
— but the knap-assisted agent used more tokens, more tool calls, and
slightly more wall time, not less.

Reading both agents' own accounts of what they did points to two specific
causes, not random noise — see
[Opportunities for improvement](#opportunities-for-improvement-surfaced-by-trial-1)
below:

1. The knap-assisted agent ran `knap lint --json` after each of the three
   `rename-*` steps (1–3) in addition to the manual steps (4–6) and the
   final check — verifying operations that are atomic and correct by
   construction bought nothing and cost 3 extra tool calls.
2. `knap fix` only auto-resolves `broken-anchor` diagnostics (via a
   fuzzy nearest-heading match) and creates a stub file for every
   `broken-link` diagnostic — it has no fuzzy nearest-_file_ match. Step 6
   asked both agents to repoint 5 broken links and 3 broken anchors at
   specific existing targets; `knap fix` couldn't do that for any of the
   5 links or 2 of the 3 anchors (the third was a same-file rename the
   agent had already tracked), so the knap-assisted agent fell back to
   the exact same hand-`Edit` work the baseline agent did for all of
   step 6 — with the added overhead of having tried and rejected `knap
fix` first.

Both are protocol/tooling gaps to fix before the official run, not
evidence the approach doesn't work — a single trial with a task this
small can't distinguish "knap doesn't help" from "knap's current loop and
`fix` command don't yet cover this task's failure modes."

## Opportunities for improvement surfaced by Trial 1

**Status: both items below are implemented** (this branch, ahead of
v0.13.0's release) — see the code/doc citations in each subsection for what
actually landed vs. what was originally proposed. (Item 2 ended up growing
into three separate shipped changes as it was rethought twice — see below.)

### 1. Tighten the skill's edit → verify loop

`skill/knap/SKILL.md` currently prescribes lint-after-every-edit
uniformly. That's the right default for hand-edits, but wasted for
`rename-file`/`rename-heading`/`rename-tag`: each rewrites every affected
link in one atomic operation, so an immediately-following lint can't
catch anything the command's own contract didn't already guarantee.

Proposed change (doc-only): split the loop in two —

- **`rename-*` commands** — chain them without an intermediate lint;
  verify once after the whole batch, not after each call.
- **Manual edits** (new links, hand-restructured sections, hand-fixed
  targets) — keep lint-after-each; this is where mistakes actually
  happen.
- **Always** a final `knap lint --json` before declaring the task done,
  regardless of which path was taken.

Expected effect: removes the 3 rename-triggered lint calls seen in
Trial 1 with no loss of correctness, since `rename-*`'s atomicity is
exactly why skipping the intermediate check is safe.

**Implemented in `skill/knap/SKILL.md`**, exactly as proposed.

### 2. Give `knap fix` a fuzzy nearest-file match for `broken-link` — and rethink how ambiguous cases surface

Read `src/cli/fix.rs` and `handlers::suggest_anchor_fix` — the two
diagnostic codes `knap fix` handles were asymmetric:

- `broken-anchor`: `suggest_anchor_fix` already does fuzzy resolution —
  Levenshtein distance between the broken slug and every heading _in the
  already-resolved target note_, auto-fixing only when there's a unique
  closest match (a tie is left alone).
- `broken-link`: unconditionally calls
  `compute_create_missing_file_fix` — i.e., always "create a stub,"
  never "maybe this is a near-miss to a file that already exists."

That asymmetry is exactly what made 5 of Trial 1's 8 seeded defects
unreachable by `knap fix`: `../topics/schema-removed.md` has an
unambiguous, one-edit-away existing match, `../topics/schema.md`, but
`knap fix` would instead create an empty `schema-removed.md` stub.

The first cut of this proposal (below, for the record) stopped at
mirroring `suggest_anchor_fix`'s shape for links. Revisiting it raised a
second, more agent-shaped problem: `knap fix` is an analog of an editor's
interactive Quick Fix — trigger, see ranked candidates, pick one — which
is a fine interaction for a human with arrow keys, but for an agent it's
an extra tool round-trip (`fix --dry-run` to see candidates, a second call
to apply one) _every time a case is ambiguous enough that auto-apply
declines to act_. The agent is already running `knap lint --json` to
verify the edit that produced the diagnostic in the first place — that
call is the natural place for ranked candidates to live.

**What actually shipped:**

- `knap fix` gained the originally-proposed repoint capability:
  `handlers::suggest_link_fix` + `rank_link_candidates` rank every note in
  the vault by edit distance between the broken target string and that
  note's path relative to the linking note (excluding the linking note
  itself), and `cli/fix.rs`'s `ResolvedLink::Broken` arm repoints when
  there's a single unambiguous closest match, falling back to today's
  create-stub behavior otherwise (tie, or zero candidates). Same
  unambiguous-only contract as anchors; never regresses today's behavior.
- **New**, beyond the original sketch: `knap lint --suggest [N]` (default
  N=3). Every `broken-link`/`broken-anchor` diagnostic's `data` field
  (the standard LSP diagnostic extension point, already
  `Option<serde_json::Value>` on `lsp_types::Diagnostic` — no schema
  break) carries `{"suggestions": [{"target", "distance"}, ...]}`, the
  same ranking `fix` uses to decide, sorted closest-first and capped at
  `N`. An agent that's ambiguous-blocked from `knap fix` gets the ranked
  candidates in the same `lint --json` call it already makes to verify,
  and applies the right one by hand — no second tool invocation to
  discover what the options even were.
- `rank_anchor_candidates` and `rank_link_candidates` are the one shared
  ranking implementation behind both `fix`'s auto-apply decision and
  `lint --suggest`'s full list, so the two can't drift: whatever `fix`
  would silently apply is always `suggestions[0]` in `lint`'s output.
- **New again**, one more round of rethinking after `--suggest` landed:
  `knap lint --fix`. Even with `--suggest` in place, the loop for a
  hand-edit was still "lint to see it, fix to apply it, lint again to
  confirm" — three calls, two of them redundant once `--suggest` exists.
  `--fix` plans and applies every safe fix (`cli/fix::plan_fixes`/`apply`,
  the exact same code `knap fix` runs — no separate implementation to
  drift) _before_ computing the diagnostics report, then rebuilds the
  index from disk so the report reflects the post-fix state. Combined
  with `--suggest`, `knap lint --fix --suggest --json` in one call: fixes
  everything unambiguous, and shows ranked candidates for whatever's left.
  Verified end-to-end against the Trial 1 vault: this single call fixed
  all 7 unambiguous seeded defects, fell back to a stub for the one
  genuinely 3-way-tied broken link, and exited `0` with an empty
  `diagnostics` array — the entire fix-and-verify phase of the task in
  one tool call instead of the three (or more) hand-`Edit` plus `fix`
  plus `lint` calls Trial 1 needed.

Expected effect: would have auto-fixed all 5 broken links and 1 of the 3
broken anchors (`storage.md`'s notifications-overview typo) in Trial 1
without any hand-`Edit` calls; the remaining 2 anchors depend on the
step-1 rename and are out of scope for a link/anchor-target fuzzy match.
`--suggest` doesn't change Trial 1's specific defects (none of them were
ambiguous) but is expected to matter for the official N≥3 run, where some
seeded defects are designed to tie. `--fix` is the bigger lever of the
three: it's what actually collapses tool-call count, which was Trial 1's
headline cost overrun.

<details>
<summary>First-cut proposal (superseded by the above)</summary>

Add a `suggest_link_fix`-style function mirroring `suggest_anchor_fix`'s
shape — Levenshtein distance between the broken link's file stem and
candidate notes' file stems, scoped to the broken link's own target
directory (not the whole vault). Wire it into `cli/fix.rs`'s
`ResolvedLink::Broken` arm: try repoint-if-unambiguous first, fall back to
today's create-stub behavior otherwise. What shipped ranks against every
note's full relative path rather than same-directory file stems only —
broader candidate pool, same unambiguous-only safety net — and adds the
`lint --suggest` half that this first cut didn't consider at all.

</details>

All three changes have landed; the official N≥3 run is next.

## Threats to validity (call these out alongside results, don't bury them)

- **N is small.** This is a manual protocol for a directional check, not a
  statistically powered study — report it as such.
- **Skill discovery bias.** If the knap-assisted agent doesn't reliably use
  the skill/CLI unprompted, that's a real finding (skill discoverability),
  not a benchmark failure — note whether it needed a nudge.
- **Corpus size sensitivity.** Effects should grow with vault size/link
  density; a 50-note corpus is a reasonable floor, but if results are
  marginal, rerunning at 150–200 notes will show whether the gap widens as
  expected. If it doesn't, that itself is worth reporting.
- **Model version drift.** Pin and record the exact model used per run;
  re-running after a model upgrade is a new experiment, not a continuation.
