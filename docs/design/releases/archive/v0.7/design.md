# v0.7 Design — Same-file Anchor Links

Covers the stories in the v0.7 release:

| Story | Feature                                                                                          |
| ----- | ------------------------------------------------------------------------------------------------ |
| US-48 | Go to Definition on `[text](#slug)` — navigates to the matching heading in the current file      |
| US-49 | Find References on a heading — includes same-file bare anchor links alongside cross-file results |
| US-50 | Diagnostic when a bare anchor doesn't match any heading in the current file                      |
| US-51 | Anchor completions for `[text](#` — heading list scoped to the current file                      |

---

## Goal

A writer can use `[see Appendix A](#appendix-a)` to navigate within the current
note, and knap treats those links with the same intelligence it already applies
to cross-file anchors. Go to Definition jumps to the heading, Find References on
a heading surfaces all inbound anchor links (same-file and cross-file together),
broken bare anchors get a diagnostic warning, and typing `[text](#` triggers the
same heading completion list as `[text](note.md#`. These four stories ship
together because they are four facets of the same feature — same-file anchor
awareness — and delivering any one of them alone would leave the feature half-
finished.

---

## Parser Changes

No changes. The parser already extracts bare anchor links correctly. For
`[text](#my-section)`, it produces:

```rust
MarkdownLink {
    target: "".to_string(),        // empty string — the anchor-only signal
    anchor: Some("my-section".to_string()),
    anchor_range: Some(/* span covering "my-section" */),
    ..
}
```

This is documented in the parser component doc under "Edge cases handled". The
Note Index and handlers already relied on `target.is_empty()` as the sentinel
for same-file anchors — v0.7 extends the handlers to act on it rather than skip
it.

---

## Note Index Changes

No changes. All four features are implemented purely in the handlers:

- `get_note(path)` already returns the current note's headings.
- `links_to(path)` already returns all cross-file links pointing to the current
  file; filtering by anchor is done in the handler, not in the index.
- Same-file bare anchor links (`target = ""`) are not added to `links_to` — they
  are resolved by scanning the current note's `md_links` directly in the handler.

---

## Handler Changes

### `check_anchor_trigger` (internal)

Remove the early return that suppresses same-file anchor completions. Currently
the function returns `None` when the path segment before `#` is empty:

```rust
// Before:
let path = &after_open[..hash_pos];
if path.is_empty() {
    return None;         // ← remove this guard
}
Some(path.to_string())

// After:
Some(after_open[..hash_pos].to_string())  // returns "" for [text](#
```

`Some("")` is the new signal meaning "cursor is at a same-file anchor trigger".

---

### `handle_completion` (`textDocument/completion`) — US-51

Extends the anchor completion branch to handle `Some("")` returned by
`check_anchor_trigger`. When the path is empty, headings are sourced from the
current note rather than a resolved target.

```rust
pub(crate) fn handle_completion(
    params: CompletionParams,
    index: &NoteIndex,
) -> Vec<CompletionItem>
```

Updated anchor branch logic:

```rust
if let Some(target_rel) = check_anchor_trigger(&note.content, pos) {
    let headings = if target_rel.is_empty() {
        // [text](# → completions from the current file
        &note.headings
    } else {
        let ResolvedLink::Found(target_path) = index.resolve(&path, &target_rel) else {
            return vec![];
        };
        let Some(target_note) = index.get_note(&target_path) else {
            return vec![];
        };
        &target_note.headings
    };
    return headings
        .iter()
        .map(|h| {
            let s = slug(&h.text);
            CompletionItem {
                label: h.text.clone(),
                kind: Some(CompletionItemKind::REFERENCE),
                filter_text: Some(h.text.clone()),
                insert_text: Some(s.clone()),
                detail: Some(format!("#{s}")),
                ..Default::default()
            }
        })
        .collect();
}
```

The completion items are identical in shape to cross-file anchor completions —
`label` is the heading text, `insert_text` is the GFM slug.

---

### `handle_definition` (`textDocument/definition`) — US-48

Adds a same-file anchor path before the existing `index.resolve` call.

```rust
pub(crate) fn handle_definition(
    params: GotoDefinitionParams,
    index: &NoteIndex,
) -> Option<GotoDefinitionResponse>
```

Updated logic, inserted after the tag check and before `index.resolve`:

```rust
let link = find_md_link_at_position(note, pos)?;

if link.target.is_empty() {
    // Same-file anchor: [text](#slug) → navigate to heading in this file.
    let anchor = link.anchor.as_deref()?;
    let range = note
        .headings
        .iter()
        .find(|h| slug(&h.text) == slug(anchor))
        .map(|h| h.range)
        .unwrap_or_default();  // missing anchor → top of file, consistent with cross-file behaviour
    return Some(GotoDefinitionResponse::Scalar(Location {
        uri: path_to_uri(&path),
        range,
    }));
}

// Existing cross-file path follows unchanged.
let ResolvedLink::Found(target_path) = index.resolve(&path, &link.target) else {
    return None;
};
```

A bare anchor whose slug matches no heading falls back to `Range::default()` (top
of file), matching the existing behaviour for cross-file links with unrecognised
anchors.

---

### `handle_references` (`textDocument/references`) — US-49

Adds a new priority step: when the cursor is on a heading line, return all links
that reference that heading — both same-file bare anchors and cross-file anchors.
This step runs before the existing link-at-cursor check.

```rust
pub(crate) fn handle_references(params: ReferenceParams, index: &NoteIndex) -> Vec<Location>
```

New helper:

```rust
fn find_heading_at_position(note: &parser::Note, pos: Position) -> Option<&parser::Heading> {
    note.headings
        .iter()
        .find(|h| h.range.start.line <= pos.line && pos.line <= h.range.end.line)
}
```

Updated priority chain:

```rust
// 1. Tag at cursor → tag locations (unchanged)
if let Some(tag) = find_tag_at_position(note, pos) { ... }

// 2. Link at cursor → backlinks to resolved target (unchanged)
let target_path = if let Some(link) = find_md_link_at_position(note, pos) {
    match index.resolve(&path, &link.target) {
        ResolvedLink::Found(p) => p,
        ResolvedLink::Broken => return vec![],
    }
} else {
    // 3. Heading at cursor → all links to that heading (new)
    if let Some(heading) = find_heading_at_position(note, pos) {
        let heading_slug = slug(&heading.text);
        let mut locations: Vec<Location> = Vec::new();

        // Same-file bare anchors: [text](#slug) in this note
        for link in &note.md_links {
            if link.target.is_empty() {
                if link.anchor.as_deref().map(slug).as_deref() == Some(heading_slug.as_str()) {
                    locations.push(Location {
                        uri: path_to_uri(&path),
                        range: link.range,
                    });
                }
            }
        }

        // Cross-file anchors: [text](this-file.md#slug) from other notes
        for located in index.links_to(&path) {
            if located.md_link.anchor.as_deref().map(slug).as_deref()
                == Some(heading_slug.as_str())
            {
                locations.push(Location {
                    uri: path_to_uri(&located.source_path),
                    range: located.md_link.range,
                });
            }
        }

        return locations;
    }

    // 4. No link or heading at cursor → backlinks to self (unchanged)
    path.clone()
};

index.links_to(&target_path).iter().map(...).collect()
```

The heading check sits between the link check and the fallback-to-self branch.
This means a cursor on a link embedded inside a heading line (e.g.
`## See [this](other.md)`) returns backlinks to `other.md` — the link wins.
Only when the cursor is on the heading prefix or plain heading text, where no
link matches, does the heading branch fire.

---

### `compute_diagnostics` — US-50

Replaces the unconditional `continue` on `link.target.is_empty()` with anchor
validation against the current note's headings.

```rust
pub(crate) fn compute_diagnostics(path: &Path, index: &NoteIndex) -> Vec<Diagnostic>
```

Updated loop body:

```rust
for link in &note.md_links {
    if link.target.is_empty() {
        // Bare anchor: validate against current file's headings.
        if let Some(anchor) = &link.anchor {
            let found = note.headings.iter().any(|h| slug(&h.text) == slug(anchor));
            if !found {
                let range = link.anchor_range.unwrap_or(link.range);
                diagnostics.push(Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!("Heading not found: '#{anchor}'"),
                    source: Some("knap".to_string()),
                    ..Default::default()
                });
            }
        }
        continue;
    }
    // Existing cross-file link validation unchanged.
    ...
}
```

The diagnostic message and severity match the existing anchor diagnostic for
cross-file links (`Heading not found: '#anchor'`, `WARNING`), so editors apply
consistent styling.

Bare anchors with no anchor text (`[text](#)` — `link.anchor = None`) produce no
diagnostic, matching the behaviour for cross-file links with an empty anchor.

---

## Protocol Handler Changes

No changes. The capabilities required by this release are already advertised:

- `completion_provider` with `"#"` as a trigger character — already present since v0.3
- `definition_provider` — already present since v0.1
- `references_provider` — already present since v0.1
- `textDocument/publishDiagnostics` — already present since v0.1

---

## Testing

### Unit tests (`src/handlers.rs`)

| Test | What it verifies |
| ---- | ---------------- |
| `completion_bare_anchor_returns_current_file_headings` | `[text](#` in a note with two headings → items for each heading, `insert_text` is GFM slug |
| `completion_bare_anchor_empty_headings` | `[text](#` in a note with no headings → empty vec |
| `completion_bare_anchor_does_not_include_other_notes` | `[text](#` → completions come from current file only, not workspace |
| `definition_same_file_anchor_navigates_to_heading` | `[text](#section)` in note with `## Section` → `Location` in same file at heading range |
| `definition_same_file_anchor_missing_falls_back_to_top` | `[text](#missing)` in note with `## Section` → `Location` in same file at `Range::default()` |
| `references_heading_includes_same_file_bare_anchors` | Note with `## Section` and `[link](#section)` → references includes that link location |
| `references_heading_includes_cross_file_anchors` | Note with `## Section`; second note links `[text](a.md#section)` → references includes that location |
| `references_heading_excludes_non_matching_anchors` | Note with `## Section`; second note links `[text](a.md#other)` → that link not in results |
| `references_heading_no_refs_returns_empty` | Heading with zero inbound anchor links → empty vec |
| `diagnostics_bare_anchor_valid` | `[text](#existing)` in note with `## Existing` → no diagnostic |
| `diagnostics_bare_anchor_broken` | `[text](#missing)` in note with `## Existing` → warning `Heading not found: '#missing'` |
| `diagnostics_bare_anchor_no_headings` | `[text](#anything)` in note with no headings → warning |
| `diagnostics_bare_anchor_empty_slug_no_diagnostic` | `[text](#)` → no diagnostic (empty anchor, nothing to validate) |

### Integration tests (`tests/lsp.rs`)

| Test | What it verifies |
| ---- | ---------------- |
| `test_same_file_anchor_definition` | `textDocument/definition` on `[text](#section)` → jumps to heading line in same file |
| `test_same_file_anchor_definition_missing` | `textDocument/definition` on `[text](#missing)` in note with `## Section` → returns top of file |
| `test_same_file_anchor_broken_diagnostic` | `textDocument/didOpen` with `[text](#missing)` → diagnostic published for that anchor |
| `test_same_file_anchor_valid_no_diagnostic` | `textDocument/didOpen` with `[text](#existing)` and matching heading → no diagnostic |
| `test_same_file_anchor_completion` | `textDocument/completion` at `[text](#` → heading items returned for current file |
| `test_same_file_anchor_references_on_heading` | `textDocument/references` on heading line → includes bare anchor links from same file |
