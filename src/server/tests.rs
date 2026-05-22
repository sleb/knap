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
