use super::{description, schema};

#[test]
fn advanced_schema_requires_fields_inside_mutation() {
    let schema = schema("deck_advanced");
    let single = &schema["oneOf"][0];
    assert_eq!(single["required"], serde_json::json!(["mutation"]));

    let variants = single["properties"]["mutation"]["oneOf"]
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
fn mutation_schemas_accept_atomic_edit_batches() {
    for name in [
        "slide_create",
        "slide_duplicate",
        "slide_delete",
        "slide_reorder",
        "text_add",
        "image_add",
        "shape_add",
        "element_update",
        "deck_advanced",
    ] {
        let schema = schema(name);
        let variants = schema["oneOf"]
            .as_array()
            .unwrap_or_else(|| panic!("{name} has no single-or-batch variants"));
        assert_eq!(variants.len(), 2, "{name}");
        assert_eq!(variants[1]["required"], serde_json::json!(["edits"]));
        assert_eq!(variants[1]["properties"]["edits"]["minItems"], 1);
        assert_eq!(
            variants[1]["properties"]["edits"]["items"], variants[0],
            "{name} batch item differs from single schema"
        );
    }
}

#[test]
fn read_tool_schemas_do_not_advertise_edit_batches() {
    for name in ["deck_inspect", "deck_validate"] {
        assert!(schema(name).get("oneOf").is_none(), "{name}");
    }
}

#[test]
fn semantic_descriptions_explain_when_to_avoid_advanced_mutations() {
    assert!(description("element_update").contains("stable ID"));
    assert!(description("deck_advanced").contains("Use semantic tools"));
}
