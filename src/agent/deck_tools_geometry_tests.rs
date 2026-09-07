use super::{execute, DeckEngine, DeckMutation};
use serde_json::json;

async fn resize(engine: &DeckEngine, width: u64, height: u64) {
    engine
        .mutate(DeckMutation::RawSet {
            part: "ppt/presentation.xml".into(),
            xpath: "/presentation/sldSz".into(),
            action: "replace".into(),
            xml: Some(format!("<p:sldSz cx=\"{width}\" cy=\"{height}\"/>")),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn add_elements_respects_actual_slide_dimensions() {
    let directory = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(directory.path().join("custom-size.pptx"), None)
        .await
        .unwrap();
    // Sixteen by nine inches, deliberately larger than the bundled starter.
    resize(&engine, 16 * 914_400, 9 * 914_400).await;
    execute(
        "text_add",
        &engine,
        json!({"slide":1,"text":"Right edge","x":14,"y":8,"width":2,"height":1}),
    )
    .await
    .expect("content inside a custom slide must not be rejected by widescreen bounds");
    let inspected = engine.inspect(None).await.unwrap();
    assert_eq!(
        inspected["slides"][0]["shapes"][0]["geometry"],
        json!({
            "x":14.0,"y":8.0,"width":2.0,"height":1.0,"unit":"in"
        })
    );
}

#[tokio::test]
async fn add_elements_rejects_overflow_on_smaller_slides_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("small-size.pptx");
    let engine = DeckEngine::create(&path, None).await.unwrap();
    resize(&engine, 10 * 914_400, 7 * 914_400).await;
    let before = std::fs::read(&path).unwrap();
    let generation = engine.generation();
    let error = execute(
        "text_add",
        &engine,
        json!({"edits":[
            {"slide":1,"text":"Inside","x":1,"y":1,"width":2,"height":1},
            {"slide":1,"text":"Outside","x":9,"y":1,"width":2,"height":1}
        ]}),
    )
    .await
    .expect_err("content outside a smaller slide must fail before committing");
    assert!(error.to_string().contains("10"), "{error}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(engine.generation(), generation);
}
