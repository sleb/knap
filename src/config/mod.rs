// Shared config loading for `lsp`/`lint`/`index`, so headless commands see
// the same `Config` the LSP would build for the same workspace.
// See docs/design/releases/v0.11/design.md ("Config Changes").

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use log::warn;
use lsp_types::InitializeParams;

pub(crate) struct SchemaField {
    pub(crate) values: Option<Vec<String>>,
    pub(crate) required: bool,
}

#[derive(Default)]
pub(crate) struct FrontmatterSchema {
    pub(crate) fields: Vec<(String, SchemaField)>,
    pub(crate) require_frontmatter: bool,
    pub(crate) warn_unknown_keys: bool,
}

#[derive(Default)]
pub(crate) struct Config {
    pub(crate) index_roots: Vec<PathBuf>,
    pub(crate) extensions: Vec<String>,
    pub(crate) new_note_dir: Option<String>,
    pub(crate) frontmatter_schema: FrontmatterSchema,
}

// Shared by both wire formats below: the `values`/`required` field names
// don't differ between camelCase JSON and snake_case TOML, so this one type
// covers both without a rename attribute.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct SchemaFieldOpts {
    values: Option<Vec<String>>,
    required: bool,
}

/// Mirrors the shape of `initializationOptions` sent by the editor. This is
/// a wire contract editor extensions depend on — do not change its shape or
/// casing.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct InitOptions {
    extensions: Option<Vec<String>>,
    new_note_dir: Option<String>,
    frontmatter_schema: Option<FrontmatterSchemaJsonOpts>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct FrontmatterSchemaJsonOpts {
    fields: HashMap<String, SchemaFieldOpts>,
    require_frontmatter: bool,
    warn_on_unknown_keys: bool,
}

/// Mirrors `knap.toml`'s shape — idiomatic snake_case, independent of
/// `InitOptions`' camelCase wire contract.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
pub(crate) struct KnapToml {
    extensions: Option<Vec<String>>,
    new_note_dir: Option<String>,
    frontmatter_schema: Option<FrontmatterSchemaTomlOpts>,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct FrontmatterSchemaTomlOpts {
    fields: HashMap<String, SchemaFieldOpts>,
    require_frontmatter: bool,
    warn_unknown_keys: bool,
}

/// Config as parsed from a single source (`knap.toml` or
/// `initializationOptions`), before layering and defaulting. `None` means
/// "this source didn't say" — distinct from an explicit empty value.
#[derive(Default)]
struct RawConfig {
    extensions: Option<Vec<String>>,
    new_note_dir: Option<String>,
    frontmatter_schema: Option<(HashMap<String, SchemaFieldOpts>, bool, bool)>,
}

impl From<InitOptions> for RawConfig {
    fn from(opts: InitOptions) -> Self {
        RawConfig {
            extensions: opts.extensions,
            new_note_dir: opts.new_note_dir,
            frontmatter_schema: opts
                .frontmatter_schema
                .map(|s| (s.fields, s.require_frontmatter, s.warn_on_unknown_keys)),
        }
    }
}

impl From<KnapToml> for RawConfig {
    fn from(toml: KnapToml) -> Self {
        RawConfig {
            extensions: toml.extensions,
            new_note_dir: toml.new_note_dir,
            frontmatter_schema: toml
                .frontmatter_schema
                .map(|s| (s.fields, s.require_frontmatter, s.warn_unknown_keys)),
        }
    }
}

/// `primary` wins field-by-field; `fallback` fills in what `primary` left
/// unset.
fn merge(primary: RawConfig, fallback: RawConfig) -> RawConfig {
    RawConfig {
        extensions: primary.extensions.or(fallback.extensions),
        new_note_dir: primary.new_note_dir.or(fallback.new_note_dir),
        frontmatter_schema: primary.frontmatter_schema.or(fallback.frontmatter_schema),
    }
}

fn build_frontmatter_schema(
    fields: HashMap<String, SchemaFieldOpts>,
    require_frontmatter: bool,
    warn_unknown_keys: bool,
) -> FrontmatterSchema {
    let mut fields: Vec<(String, SchemaField)> = fields
        .into_iter()
        .map(|(k, v)| {
            (
                k,
                SchemaField {
                    values: v.values,
                    required: v.required,
                },
            )
        })
        .collect();
    fields.sort_by(|a, b| a.0.cmp(&b.0));
    FrontmatterSchema {
        fields,
        require_frontmatter,
        warn_unknown_keys,
    }
}

fn finalize(raw: RawConfig, index_roots: Vec<PathBuf>) -> Config {
    let frontmatter_schema = match raw.frontmatter_schema {
        Some((fields, require_frontmatter, warn_unknown_keys)) => {
            build_frontmatter_schema(fields, require_frontmatter, warn_unknown_keys)
        }
        None => FrontmatterSchema::default(),
    };

    Config {
        index_roots,
        extensions: raw.extensions.unwrap_or_else(|| vec!["md".to_string()]),
        new_note_dir: raw.new_note_dir,
        frontmatter_schema,
    }
}

/// Looks for `knap.toml` directly in `start` — no ancestor-directory search.
pub(crate) fn find_knap_toml(start: &Path) -> Option<PathBuf> {
    let candidate = start.join("knap.toml");
    candidate.is_file().then_some(candidate)
}

/// `Ok(None)` if no `knap.toml` exists under `root` (defaults apply).
/// `Err` if it exists but is malformed — fail loud, never silently default.
pub(crate) fn load_knap_toml(root: &Path) -> Result<Option<KnapToml>> {
    let Some(path) = find_knap_toml(root) else {
        return Ok(None);
    };
    let content =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let parsed: KnapToml =
        toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
    Ok(Some(parsed))
}

/// Loader for `knap lsp`: `index_roots` from `workspaceFolders`,
/// `initializationOptions` layered over `knap.toml` from the first
/// workspace root (editor value wins where present).
pub(crate) fn for_lsp(params: &InitializeParams) -> Result<Config> {
    let index_roots: Vec<PathBuf> = params
        .workspace_folders
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .filter_map(|folder| {
            url::Url::parse(folder.uri.as_str())
                .ok()?
                .to_file_path()
                .ok()
        })
        .collect();

    let knap_toml_raw = match index_roots.first() {
        Some(root) => load_knap_toml(root)?
            .map(RawConfig::from)
            .unwrap_or_default(),
        None => RawConfig::default(),
    };

    let init_opts: InitOptions = params
        .initialization_options
        .as_ref()
        .map(|v| {
            serde_json::from_value(v.clone()).unwrap_or_else(|e| {
                warn!("initializationOptions parse error: {e}; using defaults");
                InitOptions::default()
            })
        })
        .unwrap_or_default();

    let raw = merge(RawConfig::from(init_opts), knap_toml_raw);
    Ok(finalize(raw, index_roots))
}

/// Loader for `knap lint`/`knap index`: `knap.toml` only, no editor
/// involved. If `path` is a file, its parent directory is the root.
/// `extensions_override` is unused today — reserved for a future `--ext`
/// flag.
pub(crate) fn for_path(path: &Path, extensions_override: Option<Vec<String>>) -> Result<Config> {
    let root = if path.is_file() {
        path.parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        path.to_path_buf()
    };

    let mut raw = load_knap_toml(&root)?
        .map(RawConfig::from)
        .unwrap_or_default();
    if extensions_override.is_some() {
        raw.extensions = extensions_override;
    }

    Ok(finalize(raw, vec![root]))
}

#[cfg(test)]
mod tests;
