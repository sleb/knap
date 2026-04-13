# Request Handlers & Diagnostics

Covers all LSP request handlers and the diagnostic publisher for v0.1.

Each request handler receives the decoded params, a shared reference to the
`NoteIndex`, and the `Config`. Handlers are pure functions — they do not mutate
the index or send messages directly. They return a value that the Protocol
Handler serialises and sends.

---

## Helper: find_link_at_position()

Shared by Definition and References. Finds the wiki-link in a note whose range
contains a given cursor position.

```rust
fn find_link_at_position<'a>(note: &'a Note, pos: Position) -> Option<&'a WikiLink> {
    note.wiki_links.iter().find(|link| contains(link.range, pos))
}

fn contains(range: Range, pos: Position) -> bool {
    (pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character))
    && (pos.line < range.end.line
        || (pos.line == range.end.line && pos.character <= range.end.character))
}
```

---

## Completion (`textDocument/completion`)

### When it fires

The client sends a completion request when the user types `[` (registered as a
trigger character). Before building the list, the handler checks the document
content to confirm the cursor is preceded by `[[` — a single `[` should not
trigger note completions.

```rust
fn check_trigger(content: &str, pos: Position) -> bool {
    // Get the text of the line up to the cursor
    let line = content.lines().nth(pos.line as usize).unwrap_or("");
    let up_to_cursor = &line[..pos.character as usize];
    up_to_cursor.ends_with("[[")
}
```

### Response

One `CompletionItem` per note in the index.

```rust
fn handle_completion(
    params: CompletionParams,
    index: &NoteIndex,
    _config: &Config,
) -> Vec<CompletionItem> {
    let pos = params.text_document_position.position;
    let uri = params.text_document_position.text_document.uri;

    // Retrieve the current document content from the index
    let path = uri_to_path(&uri);
    let Some(current_note) = index.get_note(&path) else {
        return vec![];
    };

    if !check_trigger(&current_note.content, pos) {
        return vec![];
    }

    index.all_notes()
        .map(|note| CompletionItem {
            label: note.stem.clone(),
            kind: Some(CompletionItemKind::FILE),
            ..Default::default()
        })
        .collect()
}
```

`insertText` is left unset — the editor inserts the `label` by default. The
closing `]]` is not auto-inserted in v0.1; this can be revisited.

---

## Go to Definition (`textDocument/definition`)

```rust
fn handle_definition(
    params: GotoDefinitionParams,
    index: &NoteIndex,
    _config: &Config,
) -> Option<Location> {
    let pos  = params.text_document_position_params.position;
    let path = uri_to_path(&params.text_document_position_params.text_document.uri);

    let note = index.get_note(&path)?;
    let link = find_link_at_position(note, pos)?;

    match index.resolve(&link.stem) {
        ResolvedLink::Found(target_path) => Some(Location {
            uri:   path_to_uri(&target_path),
            range: Range::default(),  // top of file
        }),
        _ => None,
    }
}
```

Returns `None` for broken and ambiguous links — the diagnostic already flags
these, so silently returning nothing is the right behaviour.

---

## Find References (`textDocument/references`)

```rust
fn handle_references(
    params: ReferenceParams,
    index: &NoteIndex,
    _config: &Config,
) -> Vec<Location> {
    let pos  = params.text_document_position.position;
    let path = uri_to_path(&params.text_document_position.text_document.uri);

    let note = match index.get_note(&path) {
        Some(n) => n,
        None    => return vec![],
    };
    let link = match find_link_at_position(note, pos) {
        Some(l) => l,
        None    => return vec![],   // not on a wiki-link — do nothing
    };

    let target_path = match index.resolve(&link.stem) {
        ResolvedLink::Found(p) => p,
        _                      => return vec![],
    };

    index.links_to(&target_path)
        .iter()
        .map(|located| Location {
            uri:   path_to_uri(&located.source_path),
            range: located.wiki_link.range,
        })
        .collect()
}
```

---

## Diagnostics

Diagnostics are not a request handler — they are published proactively by the
Protocol Handler whenever the index changes. The Protocol Handler calls
`publish_diagnostics` with the set of affected paths returned by `IndexDelta`.

```rust
fn publish_diagnostics(
    paths: &HashSet<PathBuf>,
    index: &NoteIndex,
    sender: &Sender<Message>,
) {
    for path in paths {
        let diagnostics = compute_diagnostics(path, index);
        let params = PublishDiagnosticsParams {
            uri:         path_to_uri(path),
            diagnostics,
            version:     None,
        };
        let _ = sender.send(Message::Notification(Notification::new(
            PublishDiagnostics::METHOD.to_string(),
            params,
        )));
    }
}
```

### compute_diagnostics()

```rust
fn compute_diagnostics(path: &Path, index: &NoteIndex) -> Vec<Diagnostic> {
    let Some(note) = index.get_note(path) else {
        return vec![];
    };

    note.wiki_links.iter().filter_map(|link| {
        match index.resolve(&link.stem) {
            ResolvedLink::Broken => Some(Diagnostic {
                range:    link.inner_range,
                severity: Some(DiagnosticSeverity::WARNING),
                message:  format!("No note found for '[[{}]]'", link.stem),
                source:   Some("knap".to_string()),
                ..Default::default()
            }),
            ResolvedLink::Ambiguous(paths) => Some(Diagnostic {
                range:    link.inner_range,
                severity: Some(DiagnosticSeverity::WARNING),
                message:  format!(
                    "'[[{}]]' matches multiple notes: {}",
                    link.stem,
                    paths.iter()
                        .map(|p| p.file_name().unwrap_or_default().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                source:   Some("knap".to_string()),
                ..Default::default()
            }),
            ResolvedLink::Found(_) => None,
        }
    }).collect()
}
```

---

## Utilities

These small helpers are used across multiple handlers and are not part of any
single component.

```rust
// lsp-types 0.97 uses its own `Uri` type (backed by fluent-uri), not url::Url.
// Conversion goes through url::Url as an intermediate step.
fn uri_to_path(uri: &lsp_types::Uri) -> PathBuf {
    url::Url::parse(uri.as_str())
        .expect("invalid URI")
        .to_file_path()
        .expect("non-file URI")
}

fn path_to_uri(path: &Path) -> lsp_types::Uri {
    url::Url::from_file_path(path)
        .expect("non-absolute path")
        .as_str()
        .parse()
        .expect("file URL should parse as Uri")
}
```
