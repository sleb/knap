// Shared config loading for `lsp`/`lint`/`index`, so headless commands see
// the same `Config` the LSP would build for the same workspace.
// See docs/design/releases/v0.11/design.md ("Config Changes").

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

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
    /// Note-file extensions (e.g. `["md"]`). Raw, unparsed form — kept for
    /// existing tests that inspect it directly; `path_filter` is the
    /// compiled authority derived from this field (and `exclude`), consulted
    /// by `Config::is_note`/`should_index` instead of this list directly.
    ///
    /// `allow(dead_code)`: only read by `config/tests.rs`'s merge-precedence
    /// assertions now that `index::build` and the three LSP handlers consult
    /// `path_filter` instead.
    #[allow(dead_code)]
    pub(crate) extensions: Vec<String>,
    pub(crate) new_note_dir: Option<String>,
    pub(crate) frontmatter_schema: FrontmatterSchema,
    /// Glob patterns to exclude from indexing, matched against each entry's
    /// path relative to its index root. Raw, unparsed form — kept for
    /// existing tests/consumers that inspect it directly. `path_filter` is
    /// the compiled authority derived from this field (and `extensions`);
    /// consult that instead of re-deriving indexing decisions from this
    /// list. Populated by unioning `knap.toml`'s `exclude` field with the
    /// CLI's `--exclude` flag (`lint`/`index`) or the LSP's
    /// `initializationOptions.exclude` field, whichever applies.
    ///
    /// `allow(dead_code)`: only read by `config/tests.rs`'s merge-precedence
    /// assertions now that `index::build` and callers consult `path_filter`
    /// instead.
    #[allow(dead_code)]
    pub(crate) exclude: Vec<String>,
    /// Directory-name glob patterns pruned from the crawl, matched against
    /// bare directory names (never paths). Raw, unparsed form — kept for
    /// existing tests/consumers that inspect it directly. `path_filter` is
    /// the compiled authority derived from this field; consult that instead
    /// of re-deriving indexing decisions from this list. Defaults to
    /// `default_skip_dirs()` when neither `knap.toml` nor
    /// `initializationOptions` set it.
    ///
    /// `allow(dead_code)`: only read by `config/tests.rs`'s merge-precedence
    /// assertions now that `index::build` and callers consult `path_filter`
    /// instead.
    #[allow(dead_code)]
    pub(crate) skip_dirs: Vec<String>,
    /// The compiled `PathFilter` authority — see its doc comment. Built once
    /// by `finalize` from `exclude`, `extensions`, and `skip_dirs`.
    pub(crate) path_filter: PathFilter,
}

impl Config {
    /// The authoritative "does this path belong in the index" check for
    /// multi-root configs: finds the longest-matching `index_roots` prefix
    /// for `path` and delegates to `path_filter.should_index` relative to
    /// that root. A path outside every index root is never excluded — it's
    /// simply not this config's concern.
    pub(crate) fn should_index(&self, path: &Path) -> bool {
        let root = self
            .index_roots
            .iter()
            .filter(|root| path.starts_with(root))
            .max_by_key(|root| root.as_os_str().len());
        match root {
            Some(root) => self.path_filter.should_index(root, path),
            None => true,
        }
    }

    /// Should `path` be parsed as a note (vs. registered as an attachment)?
    pub(crate) fn is_note(&self, path: &Path) -> bool {
        self.path_filter.is_note(path)
    }
}

/// The single authority for "does this path belong in the index." Compiled
/// once per `Config` build, from the same two config values every current
/// mechanism (the crawl's hardcoded skip-dir check, `index::build`'s
/// `exclude` glob matching, and the watched-files handler's ad hoc extension
/// filter) was previously deriving its own partial answer from.
pub(crate) struct PathFilter {
    /// Compiled `exclude` patterns, plus the `/**`-stripped directory-equivalent
    /// form for each (see `compile`'s doc comment) — moved here verbatim from
    /// `index::build`.
    excludes: Vec<glob::Pattern>,
    /// Note file extensions (e.g. `["md"]`), for the note-vs-attachment split.
    extensions: Vec<String>,
    /// Compiled `skip_dirs` patterns, matched against bare directory names
    /// (never full paths) — see `matches_skip_dir`.
    skip_dirs: Vec<glob::Pattern>,
}

/// The built-in skip-dir patterns applied when no `skip_dirs` config is set:
/// dotfiles/dot-directories, `node_modules`, and `target`.
pub(crate) fn default_skip_dirs() -> Vec<String> {
    vec![
        ".*".to_string(),
        "node_modules".to_string(),
        "target".to_string(),
    ]
}

impl Default for PathFilter {
    fn default() -> Self {
        // `default_skip_dirs()` only ever produces valid glob patterns, so
        // this can't fail in practice.
        Self::compile(&[], &[], &default_skip_dirs())
            .expect("default_skip_dirs() patterns are always valid globs")
    }
}

impl PathFilter {
    /// Compiles `exclude`'s and `skip_dirs`'s glob patterns once, validated
    /// eagerly (`Err` on a malformed pattern, never silently ignored). For
    /// each `exclude` pattern ending in `/**`, also compiles the
    /// suffix-stripped directory-equivalent form, so `dir` itself is
    /// recognized as excluded (matching `dir/**`'s intent) without ever
    /// being `read_dir`'d — logic moved verbatim from `index::build`.
    pub(crate) fn compile(
        exclude: &[String],
        extensions: &[String],
        skip_dirs: &[String],
    ) -> anyhow::Result<Self> {
        let mut excludes: Vec<glob::Pattern> = exclude
            .iter()
            .map(|pattern| glob::Pattern::new(pattern).map_err(anyhow::Error::from))
            .collect::<anyhow::Result<_>>()?;

        // The glob crate's `**` doesn't match the zero-extra-segment case, so
        // a pattern like `dir/**` matches everything *inside* `dir` but not
        // `dir` itself — without this, `dir` would still get `read_dir`'d
        // (and its direct children individually filtered) before recursion
        // stopped one level down, instead of never being opened at all.
        // Compile the directory-equivalent form (the `/**` suffix stripped)
        // once, alongside the original patterns, so `dir` itself is
        // recognized as excluded too.
        for pattern in exclude {
            if let Some(dir_form) = pattern.strip_suffix("/**") {
                excludes.push(glob::Pattern::new(dir_form)?);
            }
        }

        let skip_dirs: Vec<glob::Pattern> = skip_dirs
            .iter()
            .map(|pattern| glob::Pattern::new(pattern).map_err(anyhow::Error::from))
            .collect::<anyhow::Result<_>>()?;

        Ok(PathFilter {
            excludes,
            extensions: extensions.to_vec(),
            skip_dirs,
        })
    }

    /// Does `name` (a bare directory name, never a path) match one of the
    /// compiled `skip_dirs` patterns? Replaces the previously hardcoded
    /// `.`-prefixed/`node_modules`/`target` check — now data-driven, with
    /// `default_skip_dirs()` reproducing the old hardcoded set.
    fn matches_skip_dir(&self, name: &str) -> bool {
        self.skip_dirs.iter().any(|pattern| pattern.matches(name))
    }

    // `require_literal_separator: true` keeps `*` from crossing `/` (so a
    // bare `*.md` only matches top-level files, matching gitignore/VS Code
    // semantics), while `**` is still allowed to cross separators per the
    // glob crate's docs.
    fn matches_exclude(&self, relative: &Path) -> bool {
        let match_options = glob::MatchOptions {
            require_literal_separator: true,
            ..Default::default()
        };
        self.excludes
            .iter()
            .any(|pattern| pattern.matches_path_with(relative, match_options))
    }

    /// Crawl-only: should this directory be pruned (never `read_dir`'d)? Used
    /// by `index::walk_dir` on directory entries, where `dir_path` is the
    /// entry's full path and `dir_name` its file name — and by `apply.rs`'s
    /// scratch-copy walkers, which prune the same way so the batch's staging
    /// copy matches the shape of the index.
    pub(crate) fn should_skip_dir(&self, root: &Path, dir_path: &Path, dir_name: &str) -> bool {
        self.matches_skip_dir(dir_name)
            || self.matches_exclude(dir_path.strip_prefix(root).unwrap_or(dir_path))
    }

    /// The authoritative check: does `path` (under `root`) belong in the
    /// index? True unless some ancestor directory component is a hardcoded
    /// skip-dir, or the path itself matches an `exclude` pattern. Used by the
    /// crawl's file-handling branch *and* every live-index handler — the same
    /// question, asked the same way, everywhere a path is considered for
    /// indexing.
    pub(crate) fn should_index(&self, root: &Path, path: &Path) -> bool {
        let relative = path.strip_prefix(root).unwrap_or(path);

        // Only ancestor directory components are checked against the
        // hardcoded skip-dir names — never the leaf itself. This matches the
        // crawl's existing behaviour: `should_skip_dir` is only ever applied
        // to directory entries, never file entries, so a dotfile like
        // `.hidden.md` sitting directly under an included root has always
        // been indexed.
        let under_skip_dir = relative
            .parent()
            .into_iter()
            .flat_map(Path::components)
            .any(|c| match c {
                Component::Normal(name) => self.matches_skip_dir(&name.to_string_lossy()),
                _ => false,
            });

        !under_skip_dir && !self.matches_exclude(relative)
    }

    /// Should `path` be parsed as a note (vs. registered as an attachment)?
    pub(crate) fn is_note(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|ext| self.extensions.iter().any(|e| e == ext))
            .unwrap_or(false)
    }
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
    exclude: Option<Vec<String>>,
    skip_dirs: Option<Vec<String>>,
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
    exclude: Option<Vec<String>>,
    skip_dirs: Option<Vec<String>>,
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
    exclude: Option<Vec<String>>,
    skip_dirs: Option<Vec<String>>,
}

impl From<InitOptions> for RawConfig {
    fn from(opts: InitOptions) -> Self {
        RawConfig {
            extensions: opts.extensions,
            new_note_dir: opts.new_note_dir,
            frontmatter_schema: opts
                .frontmatter_schema
                .map(|s| (s.fields, s.require_frontmatter, s.warn_on_unknown_keys)),
            exclude: opts.exclude,
            skip_dirs: opts.skip_dirs,
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
            exclude: toml.exclude,
            skip_dirs: toml.skip_dirs,
        }
    }
}

/// `primary` wins field-by-field; `fallback` fills in what `primary` left
/// unset. `exclude` is unioned instead: both sources' patterns apply.
/// `skip_dirs` uses override precedence, not `exclude`'s union: a source
/// that sets it fully replaces the other's list rather than merging with it.
fn merge(primary: RawConfig, fallback: RawConfig) -> RawConfig {
    let exclude = match (primary.exclude, fallback.exclude) {
        (Some(mut p), Some(f)) => {
            p.extend(f);
            Some(p)
        }
        (p, f) => p.or(f),
    };
    RawConfig {
        extensions: primary.extensions.or(fallback.extensions),
        new_note_dir: primary.new_note_dir.or(fallback.new_note_dir),
        frontmatter_schema: primary.frontmatter_schema.or(fallback.frontmatter_schema),
        exclude,
        skip_dirs: primary.skip_dirs.or(fallback.skip_dirs),
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

fn finalize(raw: RawConfig, index_roots: Vec<PathBuf>) -> Result<Config> {
    let frontmatter_schema = match raw.frontmatter_schema {
        Some((fields, require_frontmatter, warn_unknown_keys)) => {
            build_frontmatter_schema(fields, require_frontmatter, warn_unknown_keys)
        }
        None => FrontmatterSchema::default(),
    };

    let extensions = raw.extensions.unwrap_or_else(|| vec!["md".to_string()]);
    let exclude = raw.exclude.unwrap_or_default();
    let skip_dirs = raw.skip_dirs.unwrap_or_else(default_skip_dirs);
    let path_filter = PathFilter::compile(&exclude, &extensions, &skip_dirs)?;

    Ok(Config {
        index_roots,
        extensions,
        new_note_dir: raw.new_note_dir,
        frontmatter_schema,
        exclude,
        skip_dirs,
        path_filter,
    })
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
    finalize(raw, index_roots)
}

/// Loader for `knap lint`/`knap index`: `knap.toml` only, no editor
/// involved. If `path` is a file, its parent directory is the root.
/// `extensions_override` is unused today — reserved for a future `--ext`
/// flag. `exclude_additions` (the `lint`/`index` `--exclude` flag's values)
/// are appended to `knap.toml`'s `exclude` list.
pub(crate) fn for_path(
    path: &Path,
    extensions_override: Option<Vec<String>>,
    exclude_additions: &[String],
) -> Result<Config> {
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
    if !exclude_additions.is_empty() {
        raw.exclude
            .get_or_insert_with(Vec::new)
            .extend(exclude_additions.iter().cloned());
    }

    finalize(raw, vec![root])
}

#[cfg(test)]
mod tests;
