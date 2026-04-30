# Getting Started with knap

knap is a Markdown language server that brings Obsidian-style `[[wiki-link]]`
navigation to any LSP-compatible editor. Install the server binary, connect
your editor, and your notes become navigable: jump to definitions, find
backlinks, catch broken links, and rename files without breaking anything.

---

## 1. Install the server

> **Zed users:** the `zed-knap` extension downloads the server automatically.
> You can skip this step and go straight to [Connect your editor](#2-connect-your-editor).

### Download a pre-built binary

Grab the latest release from the [GitHub releases page](https://github.com/sleb/knap/releases/latest), download the binary for your platform, and copy it somewhere on your `PATH`:

```bash
cp knap ~/.local/bin/
```

### Build from source

Requires Rust stable:

```bash
git clone https://github.com/sleb/knap.git
cd knap
cargo build --release
cp target/release/knap ~/.local/bin/
```

### Verify

```bash
knap check
```

Runs a built-in LSP smoke test. All checks should pass if the server is
installed correctly.

---

## 2. Connect your editor

### Zed

Install the **zed-knap** extension from the Zed extension marketplace:

1. Open the Extensions panel (`Cmd+Shift+X` on macOS, `Ctrl+Shift+X` on Linux)
2. Search for **knap**
3. Click **Install**

The extension registers knap as the language server for Markdown files and
automatically downloads the server binary from the latest GitHub release — no
manual installation needed.

To pass configuration options (see [Configuration](#3-configuration) below),
add a `lsp` block to your Zed `settings.json`. Including the `$schema` key
enables inline autocompletion and validation for all knap options:

```json
{
  "lsp": {
    "knap": {
      "initialization_options": {
        "$schema": "https://raw.githubusercontent.com/sleb/knap/main/schemas/v1/initialization_options.json",
        "extensions": ["md"],
        "attachmentsDir": "assets",
        "newNoteDir": "0-Inbox"
      }
    }
  }
}
```

### VS Code

Install the **vscode-knap** extension from the [GitHub releases page](https://github.com/sleb/vscode-knap/releases/latest):

1. Download `vscode-knap-*.vsix` from the latest release
2. In VS Code, open the Command Palette and run **Extensions: Install from VSIX…**
3. Select the downloaded file

The extension locates the `knap` binary automatically by searching
`~/.cargo/bin`, `/usr/local/bin`, `/opt/homebrew/bin`, and `$PATH`. If your
binary is elsewhere, set `knap.serverPath` in your VS Code settings to its
absolute path.

To pass configuration options (see [Configuration](#3-configuration) below),
add an `initializationOptions` block to your VS Code `settings.json`:

```json
{
  "knap.serverPath": "/path/to/knap",
  "initialization_options": {
    "extensions": ["md"],
    "attachmentsDir": "assets",
    "newNoteDir": "0-Inbox"
  }
}
```

### Other editors

For Neovim, Helix, and others, follow your editor's standard procedure for
adding a custom LSP server, pointing it at the `knap` binary. The server speaks
the standard Language Server Protocol over stdin/stdout — no special flags or
arguments are needed.

---

## 3. Configuration

knap works with zero configuration for a standard single-folder Markdown
workspace. The following options can be passed via `initializationOptions`
when you need to customise behaviour:

| Option              | Type             | Default  | Description                                                                                                                                                                                                                 |
| ------------------- | ---------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `extensions`        | `string[]`       | `["md"]` | File extensions treated as notes. Files with other extensions are treated as attachments.                                                                                                                                   |
| `attachmentsDir`    | `string \| null` | `null`   | Path to your attachments folder, relative to the workspace root. When set, knap watches this directory for new and deleted files so attachment-link diagnostics stay live.                                                  |
| `newNoteDir`        | `string \| null` | `null`   | Folder (relative to workspace root) where Quick Fix "Create note" actions create new files (e.g. `"0-Inbox"`). Defaults to the same directory as the current note.                                                          |
| `frontmatterSchema` | `object \| null` | `null`   | JSON Schema-inspired definition of allowed frontmatter keys and values. Enables key and value completions in the frontmatter block and publishes warnings for unknown keys, invalid enum values, and missing required keys. |

**Example — multi-extension vault with an assets folder and inbox:**

```json
{
  "extensions": ["md", "mdx"],
  "attachmentsDir": "assets",
  "newNoteDir": "0-Inbox"
}
```

**Example — frontmatter schema with required keys and enum values:**

```json
{
  "frontmatterSchema": {
    "properties": {
      "status": { "enum": ["draft", "review", "published"] },
      "author": {},
      "tags": {}
    },
    "required": ["status"]
  }
}
```

With this schema, knap will:

- Offer `draft`, `review`, `published` as completions after `status: `
- Offer `status`, `author`, and `tags` as key completions on blank frontmatter lines
- Warn on any frontmatter key not listed in `properties`
- Warn when `status` has a value not in the enum
- Warn when `status` is missing entirely

**Tip:** add `"$schema": "https://raw.githubusercontent.com/sleb/knap/main/schemas/v1/initialization_options.json"` to your `initialization_options` object in Zed's `settings.json` to get autocompletion and inline validation for all knap options.

---

## 4. What you get

Once connected, knap provides the following in any Markdown file:

### `[[wiki-link]]` completions

Type `[[` to get a completion list of every note in your workspace. Completions
are filtered as you type.

### Go to Definition

Place your cursor on a `[[wiki-link]]` and trigger Go to Definition
(`gd` in Neovim, `F12` in VS Code / Zed) to jump directly to the target file.

Supported link forms:

| Syntax                   | Behaviour                                          |
| ------------------------ | -------------------------------------------------- |
| `[[note]]`               | Navigate to `note.md` (or whichever extension)     |
| `[[note#Heading]]`       | Navigate to `note.md` (heading nav coming in v0.3) |
| `[[note\|display text]]` | Navigate to `note.md`; display text is ignored     |
| `[[image.png]]`          | Navigate to `image.png` in the workspace           |

### Find References

Place your cursor on a `[[wiki-link]]` (or anywhere in a file) and trigger Find
References to see every file that links to the current target.

### Rename a file

Use your editor's file-tree rename (or the rename refactor action if your
editor supports it). knap intercepts the rename via `workspace/willRenameFiles`
and returns a workspace edit that rewrites every `[[backlink]]` before the file
moves. Aliased links like `[[old-name|display text]]` are rewritten to
`[[new-name|display text]]` — the alias is preserved.

### Broken and ambiguous link diagnostics

knap publishes warnings for:

- **Broken links** — `[[target]]` where no file with that stem (or filename)
  exists in the workspace. Message: `Link target not found: '[[target]]'`
- **Ambiguous links** — `[[name]]` where two or more files share the same stem.
  Message: `'[[name]]' matches multiple files: a/name.md, b/name.md`

Diagnostics update as files are opened, saved, created, and deleted — no
restart needed.

**Attachment links:** `[[image.png]]` resolves against all files in the
workspace, not just note files. If `image.png` exists anywhere under the
workspace root (or in `attachmentsDir` if configured), the link is considered
resolved and no diagnostic is emitted.

---

## 5. Troubleshooting

**knap isn't starting.** Confirm the binary is on your `PATH` by running
`knap check` in a terminal — all checks should pass. Check your editor's LSP
log for startup errors.

**Completions show no results.** knap indexes the workspace on startup. If the
workspace folder wasn't sent in the `initialize` request (check your editor's
LSP configuration), the index will be empty. Most editors send the open folder
automatically.

**Diagnostics aren't updating after a file rename.** Your editor must send a
`workspace/willRenameFiles` request before the rename and then
`workspace/didChangeWatchedFiles` after it. Editors that go through their own
file-tree UI (VS Code Explorer, Zed project panel) do this automatically.
Terminal `mv` commands bypass the LSP and won't trigger link rewrites — reopen
the affected files to refresh diagnostics.

**Attachment links are still showing as broken after adding a file.** knap
updates attachment diagnostics live only when `attachmentsDir` is configured.
Without it, the index is built once at startup and doesn't track non-note file
changes. Set `attachmentsDir` to the folder where you keep attachments and
restart the server.
