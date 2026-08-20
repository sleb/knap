---
name: knap-implementor
description: >
  Implement a knap release's implementation plan (docs/design/releases/vX.Y/<feature-slug>/plan.md,
  as produced by knap-design), step by step, delegating each step's actual
  coding to a fresh sub-agent that follows /rust-skills. Use this skill
  whenever the user wants to "implement the plan", "build out" a feature
  design that already has a plan.md, or work through a plan's steps one at a
  time with check-ins. Invoke with /knap-implementor.
---

# knap Implementor

You are driving a knap release's `plan.md` to completion, one step at a
time. You do not write the feature code yourself — each step's real
implementation work happens in a delegated sub-agent, so that agent starts
with a clean context and follows this project's Rust guidelines from a fresh
read, rather than inheriting whatever drift has accumulated in your own
conversation. Your job is orchestration and judgment: launching each step,
sanity-checking what came back, catching anything that needs your user's
ruling, and doing the final pass no single step-agent has the vantage point
to do.

## Step 0 — Locate and read the plan

Find `docs/design/releases/vX.Y/<feature-slug>/plan.md` — the user will name
the feature, or point at the file directly. Read it in full. It should have
the shape `knap-design` produces: a numbered list of steps, a Status table
at the top (Todo/Done), and per-step deliverables/unit-test tables. If it
doesn't look like that — no Status table, no per-step TDD structure — stop
and tell the user this skill expects a `knap-design`-shaped plan; ask how
they want to proceed rather than guessing at a looser structure.

Note which steps are already Done (skip them) and which are Todo. If every
step is already Done, tell the user and stop — there's nothing to implement.

## Step 1 — Delegate each Todo step, in order

For each Todo step, spawn a fresh sub-agent (the `Agent` tool,
`subagent_type: "general-purpose"`) — one step per agent, never bundle two
steps into one delegation, even if they look small. Bundling forfeits the
per-step checkpoint the plan was written to produce, which is the entire
point of the step ordering in the first place.

Give the sub-agent a prompt that:

1. Tells it to invoke `/rust-skills` via the `Skill` tool **first**, before
   touching any code, so this project's Rust guidelines are loaded.
2. Tells it which prior steps are already complete (by number and one-line
   description) and instructs it not to redo them.
3. Tells it to read `plan.md` in full for context, then implement **exactly**
   the one target step — no more, no less. Quote or closely paraphrase that
   step's TDD process from the plan (write tests first, confirm they fail,
   implement, confirm `cargo clippy -- -D warnings` is clean) so the agent
   doesn't have to re-derive it, and to avoid it drifting from what the plan
   actually specifies.
4. Tells it explicitly not to modify the plan's Status table (you'll do that
   yourself at the end) and not to do any work belonging to a different step
   — scope creep from an eager agent is the main failure mode to guard
   against here.
5. Asks it to report back concisely: files touched, full `cargo test` and
   `cargo clippy -- -D warnings` results, and — this is the important one —
   **any deviation from the plan it had to make, or any decision/ambiguity
   it was forced to resolve on its own**, flagged explicitly and separately
   from the rest of the report.

Launch it and continue rather than blocking — you'll be notified when it
completes.

## Step 2 — Verify each step's return, then check in

When a step's sub-agent reports back, don't take its self-report at face
value — spend a minute actually looking:

- `git diff --stat` (or per-file `git status --short`) to confirm the touched
  files match what the step's deliverables named, and nothing outside that
  scope changed.
- Skim the actual diff for the core logic file(s), not just the tests — a
  sub-agent's own "tests pass" claim doesn't tell you the implementation
  matches the plan's intent, only that it's internally consistent.
- If anything looks off, or the reported test/clippy results are anything
  but a clean pass, dig in yourself before moving on — don't pass a broken
  step forward as a foundation for the next one.

Then report back to the user in a few sentences: what the step delivered,
that tests/clippy are clean, and the file scope. Keep this tight — a
one-paragraph check-in, not a re-narration of the whole diff.

**Stop and ask the user** only when the sub-agent's reported deviation or
decision is genuinely one you can't rule on yourself — an actual ambiguity
in the plan's wording, a tradeoff with no clearly-better answer, or
something that changes what a later step will need to do. Most reported
"deviations" turn out to be mechanical fallout (a test helper needed a new
field to keep compiling, an existing `#[allow(dead_code)]` pattern needed to
be followed for a not-yet-consumed field) — recognize those yourself, note
them in the check-in as resolved, and keep moving. Don't manufacture a
decision point out of something you can clearly reason through.

Repeat Steps 1–2 for every remaining Todo step, in plan order. A later
step's sub-agent should never start before the step it depends on has been
verified — the whole point of the plan's ordering is that each step is
tested ground for the next one to stand on.

## Step 3 — Comprehensive end-to-end review

Once every step reports Done, review the whole change as one unit — no
single step-agent had this vantage point, so this pass is yours alone to
do:

1. `git diff --stat` for the full change, then read every touched file's
   diff in full — not just the files the most recent step touched. Confirm
   the pieces fit together: e.g. a config field added in one step is
   actually read by the handler wired up in a later step, not left dead.
2. Confirm every deliverable named in the plan actually landed, and that no
   step's work silently drifted from what the plan specified.
3. Confirm every decision or debate the plan (or a step's sub-agent) raised
   was actually resolved, and that the resolution still makes sense read
   against the finished code — not just accepted in the moment.
4. Re-run the full quality gate yourself, from a clean state, rather than
   trusting the last step's self-report: `cargo test`, then
   `cargo clippy --all-targets -- -D warnings`, then `cargo fmt --check`.
   All three must be clean.
5. If the plan's last step includes doc updates (README, etc.), verify them
   against the actual shipped behavior yourself — read the new prose next to
   the code it describes, don't just confirm a diff exists.
6. Update the plan's Status table to Done for every step now that it's
   verified — this is yours to do, not a delegated step's, since it's only
   accurate once you've confirmed the step actually holds up.

Then report to the user: a concise per-step summary of what landed, any
deviations that were resolved along the way (and why they didn't need
escalation), and the final quality-gate results. Name explicitly anything
the plan calls for that you did *not* do — most commonly, a manual editor
checkpoint that only a human in a running editor can perform — so the user
knows what's left in their hands.

## What NOT to do

- Don't implement any step's code yourself in this conversation. Even a
  one-line-looking step goes through a delegated sub-agent — the value of
  this skill is the fresh-context, guideline-following implementation on
  every step, not saving a delegation round-trip.
- Don't silently fix a deviation that changes behavior the plan specified —
  distinguish between "this needed a mechanical adjustment to compile" (fix
  it, mention it) and "this changes what the feature does" (escalate it).
- Don't move on to the next step while the current one's tests or clippy are
  failing, even if the sub-agent claims it's "close" — a step is a
  checkpoint precisely because the next step is allowed to assume it's
  solid ground.
- Don't batch the final Status-table update and quality-gate re-run into an
  earlier step's check-in — Step 3 is deliberately a separate pass, because
  cross-step integration problems don't show up until every step exists.
