use super::{description, schema};

#[test]
fn advanced_schema_requires_fields_inside_mutation() {
    let schema = schema("deck_advanced");
    assert_eq!(schema["required"], serde_json::json!(["mutation"]));

    let variants = schema["properties"]["mutation"]["oneOf"]
        .as_array()
        .expect("advanced mutation variants");
    let set = variants
        .iter()
        .find(|variant| variant["properties"]["operation"]["const"] == "set")
        .expect("set mutation schema");
    assert_eq!(
        set["required"],
        serde_json::json!(["operation", "path", "properties"])
    );
    assert_eq!(set["additionalProperties"], false);
}

#[test]
fn semantic_descriptions_explain_when_to_avoid_advanced_mutations() {
    assert!(description("element_update").contains("stable ID"));
    assert!(description("deck_advanced").contains("Use semantic tools"));
}
