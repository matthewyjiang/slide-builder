use super::{execute, is_mutation_tool, semantic_tools, DeckEngine};
use serde_json::{json, Value};

fn contract() -> Value {
    // Fixture values intentionally provide enough room for three short labels.
    json!({
        "margins":{"left":0.75,"right":0.75,"top":0.5,"bottom":0.5},
        "gutter":0.5,
        "regions":{"evidence":{"x":0.75,"y":2.0,"width":11.0,"height":1.0}},
        "text_styles":{
            "headline":{"font_size":40,"font_family":"Arial","color":"#142837","bold":true},
            "body":{"font_size":24,"font_family":"Arial","color":"#243746"}
        }
    })
}

#[tokio::test]
async fn uneven_slide_can_be_composed_through_native_tool_payloads() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.pptx");
    let engine = DeckEngine::create(&path, None).await.unwrap();
    let added = execute(
        "text_add",
        &engine,
        json!({"edits":[
            {"slide":1,"text":"Evidence should line up","x":0.9,"y":0.5,"width":11,"height":0.8},
            {"slide":1,"text":"First result","x":1,"y":2,"width":2,"height":0.5},
            {"slide":1,"text":"Second result","x":4.2,"y":2.15,"width":3,"height":0.6},
            {"slide":1,"text":"Third result","x":8,"y":1.9,"width":2.5,"height":0.7}
        ]}),
    )
    .await
    .unwrap();
    let ids = added["affected"].as_array().unwrap();
    let headline = vec![ids[0].clone()];
    let labels = ids[1..].to_vec();
    let before = engine.inspect(None).await.unwrap();
    assert_ne!(
        before["slides"][0]["shapes"][1]["geometry"]["y"],
        before["slides"][0]["shapes"][2]["geometry"]["y"]
    );
    assert!(execute("deck_layout_inspect", &engine, json!({}))
        .await
        .unwrap()["contract"]
        .is_null());
    execute("deck_layout_set", &engine, contract())
        .await
        .unwrap();
    execute(
        "elements_layout",
        &engine,
        json!({"operation":"place","ids":labels,"region":"evidence","axis":"horizontal"}),
    )
    .await
    .unwrap();
    execute(
        "elements_layout",
        &engine,
        json!({"operation":"align","ids":headline,"reference":labels[0],"edge":"left"}),
    )
    .await
    .unwrap();
    for (ids, style) in [(headline, "headline"), (labels, "body")] {
        execute(
            "elements_layout",
            &engine,
            json!({"operation":"text_style","ids":ids,"style":style}),
        )
        .await
        .unwrap();
    }
    let reopened = DeckEngine::new(&path).unwrap();
    let state = reopened.inspect(None).await.unwrap();
    let shapes = state["slides"][0]["shapes"].as_array().unwrap();
    assert_eq!(
        shapes
            .iter()
            .map(|shape| shape["geometry"]["y"].as_f64().unwrap())
            .collect::<Vec<_>>(),
        vec![0.5, 2.0, 2.0, 2.0]
    );
    // (11 inches - two 0.5-inch gutters) / 3 gives 3.3333-inch slots
    // in the inspection API's four-decimal representation.
    assert_eq!(
        shapes[1..]
            .iter()
            .map(|shape| shape["geometry"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!({"x":0.75,"y":2.0,"width":3.3333,"height":1.0,"unit":"in"}),
            json!({"x":4.5833,"y":2.0,"width":3.3333,"height":1.0,"unit":"in"}),
            json!({"x":8.4167,"y":2.0,"width":3.3333,"height":1.0,"unit":"in"}),
        ]
    );
    assert_eq!(shapes[0]["geometry"]["x"], 0.75);
    let saved = execute("deck_layout_inspect", &reopened, json!({}))
        .await
        .unwrap();
    assert_eq!(
        saved["contract"]["text_styles"]["headline"]["font_size"],
        40.0
    );
    assert_eq!(saved["assignments"].as_object().unwrap().len(), 4);
    let audit = execute("deck_layout_audit", &reopened, json!({}))
        .await
        .unwrap();
    assert_eq!(audit["issues"], json!([]), "{audit}");
    assert_eq!(
        execute("deck_validate", &reopened, json!({}))
            .await
            .unwrap()["valid"],
        true
    );
    let rendered_html = reopened.snapshot().await.unwrap().html;
    assert!(rendered_html.contains("Evidence should line up"));
}

#[tokio::test]
async fn layout_tool_registration_and_failed_selection_preserve_the_deck() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout-rollback.pptx");
    let engine = DeckEngine::create(&path, None).await.unwrap();
    let tools = semantic_tools(engine.clone());
    for (name, writes) in [
        ("deck_layout_inspect", false),
        ("deck_layout_audit", false),
        ("deck_layout_set", true),
        ("elements_layout", true),
    ] {
        assert!(tools.iter().any(|tool| tool.spec().name == name));
        assert_eq!(is_mutation_tool(name), writes);
    }
    execute("deck_layout_set", &engine, contract())
        .await
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    let generation = engine.generation();
    let result = execute(
        "elements_layout",
        &engine,
        json!({
            "operation":"text_style","ids":["slide:256/shape:missing"],"style":"headline"
        }),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(engine.generation(), generation);
}
