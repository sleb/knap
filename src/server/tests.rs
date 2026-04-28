use std::path::PathBuf;

use lsp_types::InitializeParams;
use serde_json::json;

use super::{Config, FrontmatterSchema};

fn params_with_options(options: serde_json::Value) -> InitializeParams {
    InitializeParams {
        initialization_options: Some(options),
        ..Default::default()
    }
}

#[test]
fn config_extensions_default() {
    let params = InitializeParams::default();
    let config = Config::from_params(&params);
    assert_eq!(config.extensions, vec!["md"]);
}

#[test]
fn config_extensions_from_options() {
    let params = params_with_options(json!({"extensions": ["md", "mdx"]}));
    let config = Config::from_params(&params);
    assert_eq!(config.extensions, vec!["md", "mdx"]);
}

#[test]
fn config_attachments_dir_default() {
    let params = InitializeParams::default();
    let config = Config::from_params(&params);
    assert_eq!(config.attachments_dir, None);
}

#[test]
fn config_attachments_dir_from_options() {
    let params = params_with_options(json!({"attachmentsDir": "assets"}));
    let config = Config::from_params(&params);
    assert_eq!(config.attachments_dir, Some(PathBuf::from("assets")));
}

#[test]
fn init_opts_no_schema() {
    let params = InitializeParams::default();
    let config = Config::from_params(&params);
    assert!(config.frontmatter_schema.is_none());
}

#[test]
fn init_opts_with_schema() {
    let params = params_with_options(json!({
        "frontmatterSchema": {
            "properties": {
                "status": { "enum": ["draft", "published"] },
                "author": {}
            },
            "required": ["status"]
        }
    }));
    let config = Config::from_params(&params);
    let schema = config.frontmatter_schema.expect("expected schema");
    assert_eq!(schema.required, vec!["status"]);
    assert!(schema.properties.contains_key("status"));
    assert!(schema.properties.contains_key("author"));
    let status = &schema.properties["status"];
    assert_eq!(status.enum_values, vec!["draft", "published"]);
    assert!(schema.properties["author"].enum_values.is_empty());
}

#[test]
fn init_opts_schema_defaults_empty() {
    // `frontmatterSchema: {}` → schema present but no properties or required.
    let params = params_with_options(json!({ "frontmatterSchema": {} }));
    let config = Config::from_params(&params);
    let schema: FrontmatterSchema = config.frontmatter_schema.expect("expected schema");
    assert!(schema.properties.is_empty());
    assert!(schema.required.is_empty());
}
