# vX.Y Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step            | Status | Notes |
| --------------- | ------ | ----- |
| 1 — [Step name] | Todo   |       |
| 2 — [Step name] | Todo   |       |

---

## Step 1 — [Step name]

What this step accomplishes and why it comes first. Keep it to one or two
sentences.

**Deliverables:**

- Concrete file, function, or type to add or change
- Another deliverable

**Unit tests:**

| Test        | What it verifies  |
| ----------- | ----------------- |
| `test_name` | Short description |

> **Manual checkpoint:** What to open in the editor and what to verify. Be
> specific: which file, which action, what the expected result looks like.

---

## Step 2 — [Step name]

[Repeat the Step 1 pattern for each subsequent step.]

**Deliverables:**

- ...

**Unit tests:**

| Test        | What it verifies  |
| ----------- | ----------------- |
| `test_name` | Short description |

> **Manual checkpoint:** ...

---

## Step N — Integration tests

End-to-end tests over the full LSP message loop. Always the last step.

**Deliverables:**

- `tests/xxx.rs` with all integration tests
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                   | What it verifies  |
| ---------------------- | ----------------- |
| `test_name_round_trip` | Short description |

> **Manual checkpoint (full session):** Open the editor on a real vault. Walk
> the golden path for this release end-to-end. Confirm earlier releases are
> unaffected.

---

## Done — vX.Y complete

| Story | Feature | Delivered in step |
| ----- | ------- | ----------------- |
| US-XX | ...     | Step N            |
