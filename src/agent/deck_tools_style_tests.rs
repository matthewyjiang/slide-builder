use super::*;

#[tokio::test]
async fn styled_text_is_preserved_in_saved_dark_slide() {
    let dir = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(&dir.path().join("styled.pptx"), None)
        .await
        .unwrap();
    execute(
        "shape_add",
        &engine,
        json!({
            "slide": 1, "kind": "rectangle", "x": 0, "y": 0,
            "width": 13.333, "height": 7.5, "fill": "#0B1820"
        }),
    )
    .await
    .unwrap();
    let added = execute(
        "text_add",
        &engine,
        json!({
            "slide": 1, "text": "Your terminal forgets.",
            "x": 1, "y": 1, "width": 8, "height": 2,
            "font_size": 44, "color": "#f4f7fa", "font_family": "Arial",
            "bold": true, "italic": false, "alignment": "left"
        }),
    )
    .await
    .unwrap();
    assert!(added["affected"][0].as_str().unwrap().starts_with("slide:"));
    let xml = engine.raw("ppt/slides/slide1.xml".into()).await.unwrap();
    for expected in ["F4F7FA", "Arial", "sz=\"4400\"", "b=\"1\""] {
        assert!(xml.contains(expected), "missing {expected} in {xml}");
    }
}

#[tokio::test]
async fn invalid_text_style_rejects_the_whole_batch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid-style.pptx");
    let engine = DeckEngine::create(&path, None).await.unwrap();
    let before = std::fs::read(&path).unwrap();
    for (key, value) in [
        ("color", json!("white")),
        ("bold", json!("true")),
        ("font_size", json!(0)),
        ("alignment", json!("middle")),
    ] {
        let good = json!({"slide":1,"text":"valid","x":1,"y":1,"width":3,"height":1});
        let mut bad = good.clone();
        bad[key] = value;
        let error = execute("text_add", &engine, json!({"edits":[good,bad]}))
            .await
            .unwrap_err();
        assert!(error.to_string().contains(key), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}
