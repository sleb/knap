// Regression tests for drift between `schemas/v1/initialization_options.json`
// and the real `initializationOptions` wire contract implemented by
// `InitOptions` in `src/config/mod.rs`. See GitHub issue #72 and
// docs/design/releases/v0.22/schema-sync/{design,plan}.md.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

fn schema_json() -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("schemas/v1/initialization_options.json");
    let content =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&content).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

fn knap_toml_schema_json() -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("schemas/v1/knap_toml.json");
    let content =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&content).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

/// Returns `value`'s object keys, panicking with `path` (a human-readable
/// description of where `value` came from) if `value` isn't a JSON object —
/// e.g. because an expected branch doesn't exist yet in the schema file.
fn object_keys<'a>(value: &'a serde_json::Value, path: &str) -> HashSet<&'a str> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("{path} is not a JSON object (got {value:?})"))
        .keys()
        .map(String::as_str)
        .collect()
}

#[test]
fn schema_is_valid_json() {
    let _ = schema_json();
}

#[test]
fn schema_top_level_properties_match_wire_contract() {
    let schema = schema_json();
    let actual = object_keys(&schema["properties"], "properties");
    // `$schema` isn't an `InitOptions` field — it's the standard JSON Schema
    // self-reference key editors set in their settings file — but it must
    // stay an allowed top-level property alongside `InitOptions`' real wire
    // fields, or `additionalProperties: false` would reject it.
    let expected: HashSet<&str> = [
        "$schema",
        "extensions",
        "newNoteDir",
        "frontmatterSchema",
        "exclude",
        "skipDirs",
        "ignoreLinkTargets",
    ]
    .into_iter()
    .collect();
    assert_eq!(actual, expected);
}

#[test]
fn schema_frontmatter_schema_properties_match_wire_contract() {
    let schema = schema_json();
    let actual = object_keys(
        &schema["properties"]["frontmatterSchema"]["properties"],
        "properties.frontmatterSchema.properties",
    );
    let expected: HashSet<&str> = ["requireFrontmatter", "warnOnUnknownKeys", "fields"]
        .into_iter()
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn schema_frontmatter_schema_fields_entry_properties_match_wire_contract() {
    let schema = schema_json();
    let actual = object_keys(
        &schema["properties"]["frontmatterSchema"]["properties"]["fields"]["additionalProperties"]
            ["properties"],
        "properties.frontmatterSchema.properties.fields.additionalProperties.properties",
    );
    let expected: HashSet<&str> = ["required", "values"].into_iter().collect();
    assert_eq!(actual, expected);
}

#[test]
fn knap_toml_schema_is_valid_json() {
    let _ = knap_toml_schema_json();
}

#[test]
fn knap_toml_schema_top_level_properties_match_wire_contract() {
    let schema = knap_toml_schema_json();
    let actual = object_keys(&schema["properties"], "properties");
    let expected: HashSet<&str> = [
        "extensions",
        "new_note_dir",
        "exclude",
        "skip_dirs",
        "ignore_link_targets",
        "frontmatter_schema",
    ]
    .into_iter()
    .collect();
    assert_eq!(actual, expected);
}

#[test]
fn knap_toml_schema_frontmatter_schema_properties_match_wire_contract() {
    let schema = knap_toml_schema_json();
    let actual = object_keys(
        &schema["properties"]["frontmatter_schema"]["properties"],
        "properties.frontmatter_schema.properties",
    );
    let expected: HashSet<&str> = ["require_frontmatter", "warn_unknown_keys", "fields"]
        .into_iter()
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn knap_toml_schema_frontmatter_schema_fields_entry_properties_match_wire_contract() {
    let schema = knap_toml_schema_json();
    let actual = object_keys(
        &schema["properties"]["frontmatter_schema"]["properties"]["fields"]["additionalProperties"]
            ["properties"],
        "properties.frontmatter_schema.properties.fields.additionalProperties.properties",
    );
    let expected: HashSet<&str> = ["required", "values"].into_iter().collect();
    assert_eq!(actual, expected);
}
