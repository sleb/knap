use lsp_types::InitializeParams;
use serde_json::json;

use super::Config;

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
fn config_new_note_dir_parsed() {
    let params = params_with_options(json!({"newNoteDir": "0-Inbox"}));
    let config = Config::from_params(&params);
    assert_eq!(config.new_note_dir, Some("0-Inbox".to_string()));
}

#[test]
fn config_new_note_dir_absent() {
    let params = InitializeParams::default();
    let config = Config::from_params(&params);
    assert_eq!(config.new_note_dir, None);
}

#[test]
fn config_schema_fields_parsed() {
    let params = params_with_options(json!({
        "frontmatterSchema": {
            "fields": {
                "status": { "values": ["draft", "published"], "required": true },
                "type": { "values": ["note", "meeting"] }
            }
        }
    }));
    let config = Config::from_params(&params);
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
fn config_schema_fields_sorted() {
    let params = params_with_options(json!({
        "frontmatterSchema": {
            "fields": { "z": {}, "a": {}, "m": {} }
        }
    }));
    let config = Config::from_params(&params);
    let keys: Vec<&str> = config.frontmatter_schema.fields.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, vec!["a", "m", "z"]);
}

#[test]
fn config_schema_flags_default_false() {
    let params = params_with_options(json!({
        "frontmatterSchema": { "fields": { "status": {} } }
    }));
    let config = Config::from_params(&params);
    assert!(!config.frontmatter_schema.require_frontmatter);
    assert!(!config.frontmatter_schema.warn_unknown_keys);
}

#[test]
fn config_schema_flags_set() {
    let params = params_with_options(json!({
        "frontmatterSchema": {
            "fields": {},
            "requireFrontmatter": true,
            "warnOnUnknownKeys": true
        }
    }));
    let config = Config::from_params(&params);
    assert!(config.frontmatter_schema.require_frontmatter);
    assert!(config.frontmatter_schema.warn_unknown_keys);
}

#[test]
fn config_schema_absent_uses_default() {
    let params = InitializeParams::default();
    let config = Config::from_params(&params);
    assert!(config.frontmatter_schema.fields.is_empty());
    assert!(!config.frontmatter_schema.require_frontmatter);
    assert!(!config.frontmatter_schema.warn_unknown_keys);
}
