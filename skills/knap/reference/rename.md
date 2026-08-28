# `knap rename-*` reference

Deliberate restructures — moving a file, renaming a heading, renaming a
tag — go through one of these three subcommands instead of a hand edit.
Each rewrites every affected link atomically, in one step: there's nothing
a lint check immediately after one of these would catch that the command's
own contract didn't already guarantee. Chain as many `rename-*` calls as
the task needs back to back, with no lint in between — verify once at the
end of the batch, not after each call.

## Argument shapes

- `knap rename-file <old> <new> [--json]` (alias: `knap move-file <old>
  <new> [--json]`) — moves the note at `<old>` to `<new>` and rewrites
  every incoming and outgoing link the move affects. Errors if `<old>`
  doesn't exist or `<new>` already does.
- `knap rename-heading <file> <old> <new> [--json]` — renames a heading in
  `<file>` from `<old>` (its text or GFM slug) to `<new>` text, rewriting
  every anchor link that targets it, same-file and cross-file alike.
  Errors if `<file>` doesn't exist or has no heading matching `<old>`.
- `knap rename-tag <old> <new> [--json]` — renames a frontmatter tag from
  `<old>` to `<new>` across every note in the workspace that uses it.
  Errors if no note uses `<old>`.

## What "atomic rewrite" covers

Every affected link or heading-anchor is rewritten in one pass,
all-or-nothing: `rename-file` rewrites the moved file's own outgoing links
(if their targets are relative to the file's location) and every other
note's incoming links to it in the same operation; `rename-heading`
rewrites the heading text and every same-file or cross-file anchor link
pointing at it; `rename-tag` rewrites every note's frontmatter occurrence
of the tag. If the rewrite can't complete cleanly, nothing is left
partially renamed.

## No dedicated `--json` shape

None of the three `rename-*` subcommands has a `--json` *report* shape of
its own beyond the plain-text `"<old> → <new> (N file(s) touched)"`
summary that `--json` turns into a small JSON object
(`{"op", "summary", "files_touched", "hint"}`). The only JSON connected to
a rename with a richer shape is `apply`'s `ApplyReport`, when a rename op
rides in a `knap apply` batch — see `reference/apply.md`.

```
$ knap rename-file docs/mission.md docs/mission2.md --json
{
  "op": "rename-file",
  "summary": "docs/mission.md → docs/mission2.md",
  "files_touched": 1,
  "hint": "chain more rename-* calls if this restructure isn't done yet; run `knap lint` once at the end to verify."
}
```

Without `--json`, the same run prints:

```
docs/mission.md → docs/mission2.md (1 file(s) touched)
hint: chain more rename-* calls if this restructure isn't done yet; run `knap lint` once at the end to verify.
```

`hint` (or the trailing `hint: ...` text line) is always present for
`rename-*` — unlike `lint`/`apply`/`index`, it isn't conditional on what
the run found.

## Failure-mode examples

Error text is asserted verbatim against `src/cli/rename.rs`'s
`anyhow::ensure!`/`anyhow::anyhow!` calls — match it exactly when checking
a failure programmatically.

```
$ knap rename-file nope.md new.md
Error: nope.md: no such file

$ knap rename-file docs/mission.md docs/missions.md
Error: docs/missions.md: already exists

$ knap rename-heading docs/mission.md "Nonexistent Heading" "New Heading"
Error: docs/mission.md: no heading matching "Nonexistent Heading"

$ knap rename-tag nosuchtag newtag
Error: no note uses tag "nosuchtag"
```

All four exit non-zero and touch nothing on disk.
