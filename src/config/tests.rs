use std::path::Path;

use serde_json::json;

use super::{InitOptions, KnapToml, default_skip_dirs, for_lsp, for_path};

fn write_knap_toml(dir: &Path, content: &str) {
    std::fs::write(dir.join("knap.toml"), content).unwrap();
}

fn params_with(
    root: Option<&Path>,
    init_options: Option<serde_json::Value>,
) -> lsp_types::InitializeParams {
    let workspace_folders = root.map(|root| {
        let uri = url::Url::from_file_path(root).unwrap().to_string();
        json!([{ "uri": uri, "name": "root" }])
    });
    serde_json::from_value(json!({
        "processId": null,
        "rootUri": null,
        "capabilities": {},
        "workspaceFolders": workspace_folders,
        "initializationOptions": init_options,
    }))
    .unwrap()
}

#[test]
fn for_path_absent_knap_toml_uses_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let config = for_path(dir.path(), None, &[], &[]).unwrap();
    assert_eq!(config.extensions, vec!["md"]);
    assert_eq!(config.new_note_dir, None);
}

#[test]
fn for_path_loads_knap_toml() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"extensions = ["mdx"]"#);
    let config = for_path(dir.path(), None, &[], &[]).unwrap();
    assert_eq!(config.extensions, vec!["mdx"]);
}

#[test]
fn for_path_malformed_knap_toml_errors() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), "extensions = [");
    assert!(for_path(dir.path(), None, &[], &[]).is_err());
}

#[test]
fn for_path_knap_toml_wrong_type_errors() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"extensions = "md""#);
    assert!(for_path(dir.path(), None, &[], &[]).is_err());
}

#[test]
fn for_lsp_knap_toml_and_init_options_merge() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"extensions = ["mdx"]"#);
    let params = params_with(Some(dir.path()), Some(json!({ "newNoteDir": "inbox" })));
    let config = for_lsp(&params).unwrap();
    assert_eq!(config.extensions, vec!["mdx"]);
    assert_eq!(config.new_note_dir, Some("inbox".to_string()));
}

#[test]
fn for_lsp_init_options_overrides_knap_toml() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"extensions = ["mdx"]"#);
    let params = params_with(
        Some(dir.path()),
        Some(json!({ "extensions": ["md", "markdown"] })),
    );
    let config = for_lsp(&params).unwrap();
    assert_eq!(config.extensions, vec!["md", "markdown"]);
}

#[test]
fn for_lsp_malformed_init_options_falls_back() {
    let dir = tempfile::tempdir().unwrap();
    let params = params_with(Some(dir.path()), Some(json!("not an object")));
    let config = for_lsp(&params).unwrap();
    assert_eq!(config.extensions, vec!["md"]);
}

#[test]
fn for_lsp_malformed_knap_toml_errors() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), "extensions = [");
    let params = params_with(Some(dir.path()), None);
    assert!(for_lsp(&params).is_err());
}

#[test]
fn for_lsp_extensions_default() {
    let params = params_with(None, None);
    let config = for_lsp(&params).unwrap();
    assert_eq!(config.extensions, vec!["md"]);
}

#[test]
fn for_lsp_extensions_from_init_options() {
    let params = params_with(None, Some(json!({"extensions": ["md", "mdx"]})));
    let config = for_lsp(&params).unwrap();
    assert_eq!(config.extensions, vec!["md", "mdx"]);
}

#[test]
fn for_lsp_new_note_dir_parsed() {
    let params = params_with(None, Some(json!({"newNoteDir": "0-Inbox"})));
    let config = for_lsp(&params).unwrap();
    assert_eq!(config.new_note_dir, Some("0-Inbox".to_string()));
}

#[test]
fn for_lsp_new_note_dir_absent() {
    let params = params_with(None, None);
    let config = for_lsp(&params).unwrap();
    assert_eq!(config.new_note_dir, None);
}

#[test]
fn for_lsp_schema_fields_parsed() {
    let params = params_with(
        None,
        Some(json!({
            "frontmatterSchema": {
                "fields": {
                    "status": { "values": ["draft", "published"], "required": true },
                    "type": { "values": ["note", "meeting"] }
                }
            }
        })),
    );
    let config = for_lsp(&params).unwrap();
    let fields = &config.frontmatter_schema.fields;
    assert_eq!(fields.len(), 2);
    let status = fields.iter().find(|(k, _)| k == "status").unwrap();
    assert_eq!(
        status.1.values,
        Some(vec!["draft".to_string(), "published".to_string()])
    );
    assert!(status.1.required);
    let type_field = fields.iter().find(|(k, _)| k == "type").unwrap();
    assert_eq!(
        type_field.1.values,
        Some(vec!["note".to_string(), "meeting".to_string()])
    );
    assert!(!type_field.1.required);
}

#[test]
fn for_lsp_schema_fields_sorted() {
    let params = params_with(
        None,
        Some(json!({
            "frontmatterSchema": {
                "fields": { "z": {}, "a": {}, "m": {} }
            }
        })),
    );
    let config = for_lsp(&params).unwrap();
    let keys: Vec<&str> = config
        .frontmatter_schema
        .fields
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(keys, vec!["a", "m", "z"]);
}

#[test]
fn for_lsp_schema_flags_default_false() {
    let params = params_with(
        None,
        Some(json!({
            "frontmatterSchema": { "fields": { "status": {} } }
        })),
    );
    let config = for_lsp(&params).unwrap();
    assert!(!config.frontmatter_schema.require_frontmatter);
    assert!(!config.frontmatter_schema.warn_unknown_keys);
}

#[test]
fn for_lsp_schema_flags_set() {
    let params = params_with(
        None,
        Some(json!({
            "frontmatterSchema": {
                "fields": {},
                "requireFrontmatter": true,
                "warnOnUnknownKeys": true
            }
        })),
    );
    let config = for_lsp(&params).unwrap();
    assert!(config.frontmatter_schema.require_frontmatter);
    assert!(config.frontmatter_schema.warn_unknown_keys);
}

#[test]
fn for_lsp_schema_absent_uses_default() {
    let params = params_with(None, None);
    let config = for_lsp(&params).unwrap();
    assert!(config.frontmatter_schema.fields.is_empty());
    assert!(!config.frontmatter_schema.require_frontmatter);
    assert!(!config.frontmatter_schema.warn_unknown_keys);
}

#[test]
fn for_path_absent_knap_toml_exclude_defaults_empty() {
    let dir = tempfile::tempdir().unwrap();
    let config = for_path(dir.path(), None, &[], &[]).unwrap();
    assert_eq!(config.exclude, Vec::<String>::new());
}

#[test]
fn for_path_loads_knap_toml_exclude() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"exclude = ["a/**"]"#);
    let config = for_path(dir.path(), None, &[], &[]).unwrap();
    assert_eq!(config.exclude, vec!["a/**".to_string()]);
}

#[test]
fn for_path_exclude_additions_appended() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"exclude = ["a/**"]"#);
    let config = for_path(dir.path(), None, &["b/**".to_string()], &[]).unwrap();
    assert_eq!(config.exclude, vec!["a/**".to_string(), "b/**".to_string()]);
}

#[test]
fn for_lsp_exclude_unions_knap_toml_and_init_options() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"exclude = ["a/**"]"#);
    let params = params_with(Some(dir.path()), Some(json!({ "exclude": ["b/**"] })));
    let config = for_lsp(&params).unwrap();
    assert!(config.exclude.contains(&"a/**".to_string()));
    assert!(config.exclude.contains(&"b/**".to_string()));
}

#[test]
fn for_lsp_exclude_default_empty() {
    let params = params_with(None, None);
    let config = for_lsp(&params).unwrap();
    assert_eq!(config.exclude, Vec::<String>::new());
}

#[test]
fn for_path_absent_knap_toml_skip_dirs_defaults_to_builtin() {
    let dir = tempfile::tempdir().unwrap();
    let config = for_path(dir.path(), None, &[], &[]).unwrap();
    assert_eq!(config.skip_dirs, default_skip_dirs());
}

#[test]
fn for_path_loads_knap_toml_skip_dirs() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"skip_dirs = ["vendor"]"#);
    let config = for_path(dir.path(), None, &[], &[]).unwrap();
    assert_eq!(config.skip_dirs, vec!["vendor".to_string()]);
}

#[test]
fn for_lsp_skip_dirs_init_options_overrides_knap_toml() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"skip_dirs = ["vendor"]"#);
    let params = params_with(Some(dir.path()), Some(json!({ "skipDirs": ["build"] })));
    let config = for_lsp(&params).unwrap();
    assert_eq!(config.skip_dirs, vec!["build".to_string()]);
}

#[test]
fn for_lsp_skip_dirs_default_when_unset() {
    let params = params_with(None, None);
    let config = for_lsp(&params).unwrap();
    assert_eq!(config.skip_dirs, default_skip_dirs());
}

mod path_filter {
    use std::path::Path;

    use super::super::{PathFilter, default_skip_dirs};

    #[test]
    fn path_filter_should_index_true_for_plain_file() {
        let filter = PathFilter::compile(&[], &["md".to_string()], &default_skip_dirs()).unwrap();
        let root = Path::new("/vault");
        assert!(filter.should_index(root, &root.join("notes/todo.md")));
    }

    #[test]
    fn path_filter_should_index_false_for_excluded_glob_match() {
        let filter = PathFilter::compile(
            &["fixtures/**".to_string()],
            &["md".to_string()],
            &default_skip_dirs(),
        )
        .unwrap();
        let root = Path::new("/vault");
        assert!(!filter.should_index(root, &root.join("fixtures/broken.md")));
    }

    #[test]
    fn path_filter_should_index_false_under_hardcoded_skip_dir() {
        let filter = PathFilter::compile(&[], &["md".to_string()], &default_skip_dirs()).unwrap();
        let root = Path::new("/vault");
        assert!(!filter.should_index(root, &root.join(".git/HEAD")));
        assert!(!filter.should_index(root, &root.join("node_modules/pkg/index.md")));
        assert!(!filter.should_index(root, &root.join("target/debug/note.md")));
    }

    #[test]
    fn path_filter_should_index_true_for_leaf_dotfile() {
        let filter = PathFilter::compile(&[], &["md".to_string()], &default_skip_dirs()).unwrap();
        let root = Path::new("/vault");
        assert!(filter.should_index(root, &root.join(".hidden.md")));
    }

    #[test]
    fn path_filter_should_skip_dir_true_for_hardcoded_name() {
        let filter = PathFilter::compile(&[], &["md".to_string()], &default_skip_dirs()).unwrap();
        let root = Path::new("/vault");
        assert!(filter.should_skip_dir(root, &root.join(".git"), ".git"));
        assert!(filter.should_skip_dir(root, &root.join("node_modules"), "node_modules"));
        assert!(filter.should_skip_dir(root, &root.join("target"), "target"));
    }

    #[test]
    fn path_filter_should_skip_dir_true_for_exclude_match() {
        // `skip_dirs: &[]` isolates the `exclude`-pattern path from skip-dir
        // defaults — this test is about `exclude`, not the default skip list.
        let filter =
            PathFilter::compile(&["fixtures/**".to_string()], &["md".to_string()], &[]).unwrap();
        let root = Path::new("/vault");
        assert!(filter.should_skip_dir(root, &root.join("fixtures"), "fixtures"));
    }

    #[test]
    fn path_filter_is_note_true_for_configured_extension() {
        let filter = PathFilter::compile(&[], &["md".to_string()], &default_skip_dirs()).unwrap();
        assert!(filter.is_note(Path::new("/vault/notes/todo.md")));
    }

    #[test]
    fn path_filter_is_note_false_for_other_extension() {
        let filter = PathFilter::compile(&[], &["md".to_string()], &default_skip_dirs()).unwrap();
        assert!(!filter.is_note(Path::new("/vault/assets/image.png")));
    }

    #[test]
    fn path_filter_compile_dir_form_from_glob_star_star_suffix() {
        let filter = PathFilter::compile(
            &["fixtures/**".to_string()],
            &["md".to_string()],
            &default_skip_dirs(),
        )
        .unwrap();
        let root = Path::new("/vault");
        // `fixtures` itself (not just its contents) must be recognized as
        // excluded, so the crawl never `read_dir`s it.
        assert!(filter.should_skip_dir(root, &root.join("fixtures"), "fixtures"));
    }

    #[test]
    fn path_filter_compile_rejects_malformed_pattern() {
        assert!(
            PathFilter::compile(
                &["[".to_string()],
                &["md".to_string()],
                &default_skip_dirs()
            )
            .is_err()
        );
    }

    #[test]
    fn path_filter_default_skip_dirs_matches_hardcoded_names() {
        let root = Path::new("/vault");
        let default_filter = PathFilter::default();
        let compiled_filter =
            PathFilter::compile(&[], &["md".to_string()], &default_skip_dirs()).unwrap();
        for name in [".git", ".obsidian", "node_modules", "target"] {
            assert!(
                default_filter.should_skip_dir(root, &root.join(name), name),
                "PathFilter::default() should skip {name}"
            );
            assert!(
                compiled_filter.should_skip_dir(root, &root.join(name), name),
                "filter compiled with default_skip_dirs() should skip {name}"
            );
        }
    }

    #[test]
    fn path_filter_skip_dirs_empty_disables_pruning() {
        let filter = PathFilter::compile(&[], &["md".to_string()], &[]).unwrap();
        let root = Path::new("/vault");
        assert!(!filter.should_skip_dir(root, &root.join(".git"), ".git"));
    }

    #[test]
    fn path_filter_skip_dirs_custom_pattern() {
        let filter =
            PathFilter::compile(&[], &["md".to_string()], &["vendor".to_string()]).unwrap();
        let root = Path::new("/vault");
        assert!(filter.should_skip_dir(root, &root.join("vendor"), "vendor"));
        assert!(!filter.should_skip_dir(root, &root.join("node_modules"), "node_modules"));
    }

    #[test]
    fn path_filter_skip_dirs_malformed_pattern_errors() {
        assert!(PathFilter::compile(&[], &["md".to_string()], &["[".to_string()]).is_err());
    }
}

#[test]
fn path_filter_should_index_true_for_path_outside_all_roots() {
    let dir = tempfile::tempdir().unwrap();
    let config = for_path(dir.path(), None, &[], &[]).unwrap();
    let outside = tempfile::tempdir().unwrap();
    assert!(config.should_index(&outside.path().join("note.md")));
}

#[test]
fn config_finalize_propagates_path_filter_compile_error() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"exclude = ["["]"#);
    assert!(for_path(dir.path(), None, &[], &[]).is_err());
}

#[test]
fn for_path_absent_knap_toml_ignore_link_targets_defaults_empty() {
    let dir = tempfile::tempdir().unwrap();
    let config = for_path(dir.path(), None, &[], &[]).unwrap();
    assert_eq!(config.ignore_link_targets, Vec::<String>::new());
}

#[test]
fn for_path_loads_knap_toml_ignore_link_targets() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"ignore_link_targets = ["../a/**"]"#);
    let config = for_path(dir.path(), None, &[], &[]).unwrap();
    assert_eq!(config.ignore_link_targets, vec!["../a/**".to_string()]);
}

#[test]
fn for_path_ignore_link_target_additions_appended() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"ignore_link_targets = ["../a/**"]"#);
    let config = for_path(dir.path(), None, &[], &["../b/**".to_string()]).unwrap();
    assert_eq!(
        config.ignore_link_targets,
        vec!["../a/**".to_string(), "../b/**".to_string()]
    );
}

#[test]
fn for_lsp_ignore_link_targets_unions_knap_toml_and_init_options() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"ignore_link_targets = ["../a/**"]"#);
    let params = params_with(
        Some(dir.path()),
        Some(json!({ "ignoreLinkTargets": ["../b/**"] })),
    );
    let config = for_lsp(&params).unwrap();
    assert!(config.ignore_link_targets.contains(&"../a/**".to_string()));
    assert!(config.ignore_link_targets.contains(&"../b/**".to_string()));
}

#[test]
fn finalize_malformed_ignore_link_targets_pattern_errors() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"ignore_link_targets = ["["]"#);
    assert!(for_path(dir.path(), None, &[], &[]).is_err());
}

/// Regression test for the `frontmatterSchema` example in
/// `docs/GETTING_STARTED.md`'s "Frontmatter schema" section — guards it
/// against drifting from `InitOptions` in the future. `InitOptions` itself
/// is already correct; only `schemas/v1/initialization_options.json` and the
/// docs were stale (GitHub issue #72), so this test is expected to pass
/// already.
#[test]
fn init_options_accepts_getting_started_frontmatter_schema_example() {
    let value = json!({
        "frontmatterSchema": {
            "requireFrontmatter": false,
            "warnOnUnknownKeys": false,
            "fields": {
                "status": {
                    "required": true,
                    "values": ["draft", "published", "archived"]
                },
                "type": {
                    "values": ["doc", "project", "reference"]
                }
            }
        }
    });
    let opts: InitOptions = serde_json::from_value(value).unwrap();
    let schema = opts
        .frontmatter_schema
        .expect("frontmatterSchema should deserialize");

    assert!(!schema.require_frontmatter);
    assert!(!schema.warn_on_unknown_keys);
    assert_eq!(schema.fields.len(), 2);

    let status = schema.fields.get("status").expect("status field");
    assert!(status.required);
    assert_eq!(
        status.values,
        Some(vec![
            "draft".to_string(),
            "published".to_string(),
            "archived".to_string()
        ])
    );

    let doc_type = schema.fields.get("type").expect("type field");
    assert!(!doc_type.required);
    assert_eq!(
        doc_type.values,
        Some(vec![
            "doc".to_string(),
            "project".to_string(),
            "reference".to_string()
        ])
    );
}

/// Regression test for the `knap.toml` example in `README.md`'s `###
/// knap.toml` section — guards it against drifting from `KnapToml` in the
/// future. `KnapToml` is already correct; this test is expected to pass
/// already.
#[test]
fn knap_toml_accepts_readme_example() {
    let toml_str = r#"
extensions = ["md"]
new_note_dir = "inbox"
# Glob patterns matched against each entry's path relative to the index
# root; matching directories are never crawled and matching files are
# skipped entirely. Don't exclude the root itself (e.g. `"."` or `"**"`) —
# that produces an empty index rather than an error.
exclude = ["tests/fixtures/**"]
# Glob patterns matched against bare directory names (not paths) during the
# crawl; a matching directory is pruned outright. Replaces rather than
# unions with the built-in default (`.*`, `node_modules`, `target`) — list
# everything you want skipped, including any of the defaults you still want.
skip_dirs = ["vendor"]
# Glob patterns matched against a link's raw target text. A matching link is
# still indexed and still resolved normally — this only suppresses the
# `broken-link` diagnostic knap would otherwise report for it. Unioned with
# any `--ignore-link-target` flag values and with each file's own
# `ignore-link-targets:` frontmatter key.
ignore_link_targets = ["../sibling-workspace/**"]

[frontmatter_schema]
require_frontmatter = false
warn_unknown_keys = false

[frontmatter_schema.fields.title]
required = true

[frontmatter_schema.fields.status]
values = ["draft", "published"]
"#;

    let knap_toml: KnapToml = toml::from_str(toml_str).unwrap();

    assert_eq!(knap_toml.extensions, Some(vec!["md".to_string()]));
    assert_eq!(knap_toml.skip_dirs, Some(vec!["vendor".to_string()]));

    let schema = knap_toml
        .frontmatter_schema
        .expect("frontmatter_schema should deserialize");
    assert!(!schema.require_frontmatter);
    assert!(!schema.warn_unknown_keys);
    assert_eq!(schema.fields.len(), 2);

    let title = schema.fields.get("title").expect("title field");
    assert!(title.required);
    assert_eq!(title.values, None);

    let status = schema.fields.get("status").expect("status field");
    assert!(!status.required);
    assert_eq!(
        status.values,
        Some(vec!["draft".to_string(), "published".to_string()])
    );
}
