use std::path::Path;

use serde_json::json;

use super::{for_lsp, for_path};

fn write_knap_toml(dir: &Path, content: &str) {
    std::fs::write(dir.join("knap.toml"), content).unwrap();
}

fn params_with(root: Option<&Path>, init_options: Option<serde_json::Value>) -> lsp_types::InitializeParams {
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
    let config = for_path(dir.path(), None).unwrap();
    assert_eq!(config.extensions, vec!["md"]);
    assert_eq!(config.new_note_dir, None);
}

#[test]
fn for_path_loads_knap_toml() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"extensions = ["mdx"]"#);
    let config = for_path(dir.path(), None).unwrap();
    assert_eq!(config.extensions, vec!["mdx"]);
}

#[test]
fn for_path_malformed_knap_toml_errors() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), "extensions = [");
    assert!(for_path(dir.path(), None).is_err());
}

#[test]
fn for_path_knap_toml_wrong_type_errors() {
    let dir = tempfile::tempdir().unwrap();
    write_knap_toml(dir.path(), r#"extensions = "md""#);
    assert!(for_path(dir.path(), None).is_err());
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
    let params = params_with(Some(dir.path()), Some(json!({ "extensions": ["md", "markdown"] })));
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
    let params = params_with(None, Some(json!({
        "frontmatterSchema": {
            "fields": {
                "status": { "values": ["draft", "published"], "required": true },
                "type": { "values": ["note", "meeting"] }
            }
        }
    })));
    let config = for_lsp(&params).unwrap();
    let fields = &config.frontmatter_schema.fields;
    assert_eq!(fields.len(), 2);
    let status = fields.iter().find(|(k, _)| k == "status").unwrap();
    assert_eq!(status.1.values, Some(vec!["draft".to_string(), "published".to_string()]));
    assert!(status.1.required);
    let type_field = fields.iter().find(|(k, _)| k == "type").unwrap();
    assert_eq!(type_field.1.values, Some(vec!["note".to_string(), "meeting".to_string()]));
    assert!(!type_field.1.required);
}

#[test]
fn for_lsp_schema_fields_sorted() {
    let params = params_with(None, Some(json!({
        "frontmatterSchema": {
            "fields": { "z": {}, "a": {}, "m": {} }
        }
    })));
    let config = for_lsp(&params).unwrap();
    let keys: Vec<&str> = config.frontmatter_schema.fields.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, vec!["a", "m", "z"]);
}

#[test]
fn for_lsp_schema_flags_default_false() {
    let params = params_with(None, Some(json!({
        "frontmatterSchema": { "fields": { "status": {} } }
    })));
    let config = for_lsp(&params).unwrap();
    assert!(!config.frontmatter_schema.require_frontmatter);
    assert!(!config.frontmatter_schema.warn_unknown_keys);
}

#[test]
fn for_lsp_schema_flags_set() {
    let params = params_with(None, Some(json!({
        "frontmatterSchema": {
            "fields": {},
            "requireFrontmatter": true,
            "warnOnUnknownKeys": true
        }
    })));
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
