---
name: knap-design
description: >
  Create or refine a knap release design doc and implementation plan. Use this
  skill whenever the user wants to plan a new knap release, design a feature,
  write a design doc, draft an implementation plan, scope a milestone, think
  through how to implement an LSP capability, or decide what belongs in the next
  version. Invoke with /knap-design.
---

# knap Design & Plan

You are helping design and plan a knap release. Your job is to produce two
documents:

- `docs/design/releases/vX.Y/design.md` — what to build and why
- `docs/design/releases/vX.Y/plan.md` — the order to build it and how to verify each step

Work through the phases below in order. Each phase has a concrete output.

---

## Phase 1 — Understand the scope

Ask the user:

1. What version is this? (e.g. `v0.7`)
2. What user stories or issues does it deliver? (story IDs or free-form description)

If the user described the feature rather than giving story IDs, map what they
said to stories in `docs/USER_STORIES.md` before proceeding. Read only the
sections that seem relevant — don't load the entire file unless needed.

Then read these files to orient yourself:

- `docs/ROADMAP.md` — find the matching milestone and its story list
- `docs/ARCHITECTURE.md` — refresh on component contracts before writing any
  signatures

Don't read component docs yet; you'll read only the ones you need in Phase 2.

---

## Phase 2 — Research the implementation

For each component the release touches (parser, index, handlers, protocol
handler), read the relevant source file **and** the matching component doc:

| Component | Source | Doc |
|-----------|--------|-----|
| Parser | `src/parser/mod.rs` | `docs/design/components/parser.md` |
| Note Index | `src/index/mod.rs` | `docs/design/components/note-index.md` |
| Handlers | `src/handlers.rs` | `docs/design/components/handlers.md` |
| Protocol Handler | `src/server/mod.rs` | `docs/design/components/protocol-handler.md` |

Use `LSP hover` and `LSP goToDefinition` to resolve types and trait bounds
rather than guessing. Use `LSP findReferences` before proposing any rename or
refactor that touches existing symbols.

After reading, summarise for yourself (not the user):

- Which types need to change?
- Which methods are new vs. extended?
- What does the new LSP capability require?
- What are the non-obvious constraints or edge cases?

---

## Phase 3 — Draft the design doc

Write `docs/design/releases/vX.Y/design.md` following the template in
`docs/design/releases/templates/design.md`.

Guidelines for each section:

**Title and stories table** — list every story this release delivers. For bugs,
include the issue number and type (`Bug`).

**Goal** — one paragraph. Lead with user value ("A writer can…"), not
implementation detail. Explain why these stories ship together — what makes
them a coherent release.

**Config Changes** — omit if none. Show the exact Rust field additions to
`InitOptions` and `Config`, with a one-line comment describing each.

**Parser Changes** — omit if none. Show new or modified public types and any
new extraction functions. Describe the algorithm in prose — what events does
the parser watch for, what does it accumulate, what does it skip? List edge
cases explicitly (e.g., EOF without trailing newline, multi-byte characters,
headings with inline markup).

**Note Index Changes** — omit if none. Show new fields and method signatures.
Explain the lookup strategy if it's non-trivial. Reference existing methods
that the new ones are analogous to.

**Handler Changes** — one subsection per new or modified handler. Include the
full function signature. For completion handlers, list the trigger conditions
and priority order. For definition/references handlers, list the lookup
priority chain. Include a concrete code sketch (not pseudocode, actual Rust)
for non-trivial logic.

**Protocol Handler Changes** — omit if none. Show the capability advertisement
struct and any new dispatch arms.

**Testing** — separate tables for unit tests and integration tests. For each
test, state what it verifies as a single clause ("X when Y" or "returns Z for
input W"). Be specific enough that another developer can write the test from
the description alone.

After drafting, review it with fresh eyes: Is anything stated twice? Is any
section present but empty? Does the code sketch compile conceptually (correct
types, correct method names)? Fix before showing the user.

---

## Phase 4 — Draft the implementation plan

Write `docs/design/releases/vX.Y/plan.md` following the template in
`docs/design/releases/templates/plan.md`.

Guidelines:

**Step ordering** — order steps so that each one produces something testable
before the next step builds on it. The typical order is:

1. Data model changes (new types, parser fields, index fields)
2. New query methods on the index
3. New handlers (using TDD: write tests first, then implement)
4. Protocol changes (capabilities, dispatch)
5. Integration tests — always last

If a step has no unit tests (e.g., a watcher change tested end-to-end),
say so explicitly and explain why.

**TDD flag** — mark steps that use test-driven development with this exact
cycle, stated explicitly in the step body:
1. Write all unit tests for this step first — stub the function signature if needed to compile
2. Run `cargo test` and confirm the new tests **fail**
3. Implement until tests pass, then run `cargo clippy -- -D warnings`

For bug-fix plans specifically, the step that implements the fix must include a
regression test as the first deliverable item, with an explicit note that the
test should be written and confirmed failing before the fix is applied. This
ensures the test actually covers the bug and doesn't accidentally pass on the
unfixed code.

**Deliverables** — bullet points, each a concrete file+function+type to add or
change. Be specific enough that the developer knows exactly what to open. Don't
say "update the handler" — say "add `handle_foo(params: FooParams, index: &NoteIndex) -> FooResult` to `src/handlers.rs`".

**Unit test table** — per step, list each test with a one-clause description of
what it verifies. This table should be consistent with the Testing section in
the design doc.

**Manual checkpoint** — per step, describe a concrete editor action: which file
to open, which keystroke or action to perform, what the expected visual result
looks like. If the step has no observable editor behaviour yet, say "No editor
checkpoint — covered by integration tests in Step N."

**Done table** — the final table maps each story to the step that delivers it.
Every story in the stories table of the design doc must appear here.

---

## Phase 5 — Cross-check and handoff

Before presenting the documents to the user, verify:

- [ ] Every story in the ROADMAP milestone is covered by the design doc
- [ ] Every handler in the design doc has a matching entry in the unit test table
- [ ] Every unit test in the design doc appears in the plan's step tables
- [ ] The Done table accounts for every story
- [ ] No section in the design doc contradicts a section in the plan
- [ ] Code sketches reference only types and methods that exist (verify with LSP)

Then present both documents to the user. Offer a brief summary of:
- What the design introduces (components touched, new types, new capabilities)
- The implementation order and why it was chosen
- Any open questions or tradeoffs the user should weigh in on before coding starts

---

## Style and tone

Match the register of the existing docs exactly:

- Use plain, direct prose — no marketing language
- Lead with user value, follow with technical mechanism
- Rust signatures in code blocks; use real types from the codebase
- Tables for test cases, stories, step status — never prose lists for those
- One sentence per manual checkpoint action; be specific (which file, which
  keystroke, which visible result)
- The phrase "no step lays down untested code for the next step to build on" is
  the core constraint — enforce it in your step ordering
