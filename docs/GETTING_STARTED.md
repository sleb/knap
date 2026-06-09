# Getting Started with knap

knap is a Markdown language server that makes standard Markdown `[text](path)`
links fully navigable in any LSP-compatible editor. Install the server binary,
connect your editor, and your notes become navigable: jump to definitions, find
backlinks, catch broken links, and rename files without breaking anything.

---

## 1. Install the server

The `knap` binary is the language server your editor connects to. How you get it
depends on your editor:

- **Zed** — the `zed-knap` extension downloads and manages the binary for you.
  Skip to [Connect your editor](#2-connect-your-editor).
- **VS Code** — the `vscode-knap` extension finds a binary you've installed
  manually (it searches `~/.cargo/bin`, `/usr/local/bin`, and `$PATH`).
- **All other editors** — install the binary manually.

### Download a pre-built binary

Grab the latest release from the [GitHub releases page](https://github.com/sleb/knap/releases/latest),
download the binary for your platform, and copy it somewhere on your `PATH`:

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
        "extensions": ["md", "mdx"]
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
    "extensions": ["md", "mdx"]
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

| Option       | Type       | Default  | Description                                                                                                                                         |
| ------------ | ---------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `extensions` | `string[]` | `["md"]` | File extensions treated as notes. Files with other extensions are treated as attachments.                                                           |
| `newNoteDir` | `string`   | —        | Folder path relative to workspace root where Quick Fix "Create note" places new files. When absent, new files are created next to the linking note. |

**Example — multi-extension vault with inbox:**

```json
{
  "extensions": ["md", "mdx"],
  "newNoteDir": "0-Inbox"
}
```

### Schema (Zed / JSON-aware editors)

A JSON Schema for `initializationOptions` is provided at
`schemas/initialization_options.json` in the repository. Reference it with
`$schema` to get inline completions and validation:

```json
{
  "lsp": {
    "knap": {
      "initialization_options": {
        "$schema": "file:///path/to/knap/schemas/initialization_options.json",
        "extensions": ["md", "txt"]
      }
    }
  }
}
```

---

## 4. What you get

Once connected, knap provides the following in any Markdown file:

### Link completions

Inside a Markdown link, type `(` to open a completion list. Directory items let
you drill down one folder at a time — select a folder and the picker re-opens
showing its contents; typing `/` re-triggers automatically. Below the directory
items, the list also shows every file in the workspace, so you can jump directly
to any note or attachment by typing part of its name or path without navigating
through folders.

### Anchor completions

Type `#` after a file path or at the start of a link destination to open a
heading picker:

- **Same-file anchor** — `[text](#` opens a picker for headings in the **current
  file**.
- **Cross-file anchor** — `[text](file.md#` opens a picker for headings in the
  **target file**.

Each item shows the heading text and inserts the GFM slug form — no leading `#`.
Example: `## My Section` inserts `my-section`.

### Go to Definition

Place your cursor on a `[text](path/to/file.md)` link and trigger Go to
Definition (`gd` in Neovim, `F12` in VS Code / Zed) to jump directly to the
target file.

Supported link forms:

| Syntax                       | Behaviour                                              |
| ---------------------------- | ------------------------------------------------------ |
| `[text](note.md)`            | Navigate to `note.md`                                  |
| `[text](note.md#my-section)` | Navigate to the matching heading line in `note.md`     |
| `[text](#my-section)`        | Navigate to the matching heading in the **current** file |
| `[text](../folder/note.md)`  | Relative paths resolved from the current file          |
| `![alt](image.png)`          | Navigate to `image.png` in the workspace               |

### Find References

Place your cursor anywhere in a file and trigger Find References to see every
file that links to the current target via a `[text](path)` link.

### Rename a file

Use your editor's file-tree rename (or the rename refactor action if your
editor supports it). knap intercepts the rename via `workspace/willRenameFiles`
and returns a workspace edit that rewrites every `[text](old-name.md)` link
before the file moves.

### Rename a heading

Place your cursor on a heading line and trigger Rename Symbol (`F2` in VS Code /
Zed, `grn` in Neovim). All `[text](note.md#old-slug)` anchor links pointing at
that heading — including self-links within the same file — are rewritten to the
new slug atomically.

### Document Symbols

Trigger the Outline / Symbols panel to see every heading in the current file as
a flat list you can jump to directly.

### Workspace Symbols

Open Workspace Symbols (usually `Cmd+T` / `Ctrl+T`) and type part of a heading
name to search headings across all indexed notes. Results also include
frontmatter tags, which appear with a distinct icon (`KEY`) so you can
distinguish them from headings at a glance. Filtering is case-insensitive.

### Broken link diagnostics

knap publishes warnings for:

- **Broken links** — `[text](target.md)` where the relative path doesn't
  resolve to an existing file in the workspace.
  Message: `Link target not found: 'target.md'`
- **Broken cross-file anchors** — `[text](note.md#heading)` where the anchor
  doesn't match any heading in the target file (compared via GFM slug).
  Message: `Heading not found: '#heading'`
- **Broken same-file anchors** — `[text](#heading)` where the anchor doesn't
  match any heading in the current file.
  Message: `Heading not found: '#heading'`

Diagnostics update as files are opened, saved, created, and deleted — no
restart needed.

**Attachment links:** `![alt](image.png)` resolves against all files in the
workspace, not just note files. If `image.png` exists anywhere under the
workspace root, the link is considered resolved and no diagnostic is emitted.

### Tag completions

Inside a frontmatter `tags:` value, trigger completions to see every tag used
across your workspace. All three YAML forms are supported:

```yaml
tags: writing          # bare scalar — cursor anywhere on the value
tags: [writing, ...]   # inline list — cursor inside the brackets
tags:
  - writing            # block list — cursor on the value after `- `
```

Tags already present in the current note's frontmatter are excluded. Typing
narrows the list by prefix.

### Backlinks code lens

When you open a note that has at least one other note linking to it, a code
lens appears above the first line:

```
↑ 3 backlinks
```

Clicking the lens opens the References panel pre-populated with every file that
links to the current note — no cursor placement needed. Notes with no incoming
links show no lens.

**Zed:** code lens is disabled by default. Enable it by adding `"code_lens": true`
to your Zed `settings.json`:

```json
{
  "code_lens": true
}
```

### Find References and Go to Definition on tags

Place your cursor on a tag value in frontmatter and trigger Find References or
Go to Definition. Both return every note in the workspace that carries that tag,
with each result pointing directly at the tag's range in the file.

When your cursor is on a broken-link diagnostic, trigger your editor's Quick Fix
command to see available repairs:

- **Broken file link** (`Link target not found`) — a **Create note** action
  creates the missing `.md` file. The new file opens immediately so you can
  start writing.
- **Broken anchor** (`Heading not found`) — a **Replace anchor** action lists
  the target file's current headings; selecting one rewrites the anchor in place.

Quick Fix keybindings by editor:

| Editor  | Keybinding                                                 |
| ------- | ---------------------------------------------------------- |
| VS Code | `Ctrl+.` / `Cmd+.`                                         |
| Zed     | `Ctrl+.` / `Cmd+.`                                         |
| Neovim  | `vim.lsp.buf.code_action()` (bind to a key of your choice) |
| Helix   | `Space+a`                                                  |

By default, **Create note** places new files next to the linking file. Set
`newNoteDir` in your configuration (see [Configuration](#3-configuration)) to
route all new notes to an inbox folder instead.

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
watches the entire workspace for file changes, so diagnostics should clear
automatically. If they don't, check that your editor is delivering
`workspace/didChangeWatchedFiles` notifications — some editors require the
workspace to be open as a folder (not a single file) for this to work.
