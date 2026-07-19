use super::{DeckEngine, DeckMutation};
use std::collections::HashMap;

#[tokio::test]
async fn image_added_after_shape_returns_the_picture_id() {
    let directory = tempfile::tempdir().unwrap();
    let image_path = directory.path().join("pixel.png");
    image::RgbImage::from_pixel(2, 2, image::Rgb([0, 48, 87]))
        .save(&image_path)
        .unwrap();
    let path = directory.path().join("image-after-shape.pptx");
    let engine = DeckEngine::create(&path, None).await.unwrap();
    let shape = engine
        .mutate(DeckMutation::Add {
            parent: "/slide[1]".into(),
            element_type: "rectangle".into(),
            properties: HashMap::from([
                ("x".into(), "1in".into()),
                ("y".into(), "1in".into()),
                ("width".into(), "2in".into()),
                ("height".into(), "1in".into()),
            ]),
        })
        .await
        .unwrap();

    let picture = engine
        .mutate(DeckMutation::Add {
            parent: "/slide[1]".into(),
            element_type: "image".into(),
            properties: HashMap::from([
                ("path".into(), image_path.display().to_string()),
                ("x".into(), "4in".into()),
                ("y".into(), "1in".into()),
                ("width".into(), "2in".into()),
                ("height".into(), "2in".into()),
            ]),
        })
        .await
        .unwrap();

    assert_eq!(picture.affected.len(), 1);
    assert!(picture.affected[0].contains("/picture:"), "{picture:?}");
    assert_ne!(picture.affected, shape.affected);
    assert!(engine
        .resolve_element(picture.affected[0].clone())
        .await
        .unwrap()
        .contains("/picture["));
}

#[tokio::test]
async fn slide_scoped_inspection_includes_pictures() {
    let directory = tempfile::tempdir().unwrap();
    let image_path = directory.path().join("pixel.png");
    image::RgbImage::from_pixel(2, 2, image::Rgb([0, 48, 87]))
        .save(&image_path)
        .unwrap();
    let path = directory.path().join("scoped-picture.pptx");
    let engine = DeckEngine::create(&path, None).await.unwrap();
    engine
        .mutate(DeckMutation::Add {
            parent: "/slide[1]".into(),
            element_type: "image".into(),
            properties: HashMap::from([
                ("path".into(), image_path.display().to_string()),
                ("x".into(), "1in".into()),
                ("y".into(), "1in".into()),
                ("width".into(), "2in".into()),
                ("height".into(), "2in".into()),
            ]),
        })
        .await
        .unwrap();

    let inspected = engine.inspect(Some("/slide[1]".into())).await.unwrap();
    assert!(
        inspected["children"]
            .as_array()
            .unwrap()
            .iter()
            .any(|element| element["type"] == "image"),
        "{inspected}"
    );
}

#[tokio::test]
async fn observed_file_changes_advance_render_generation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("generation.pptx");
    let engine = DeckEngine::create(&path, None).await.unwrap();
    let previous = engine.generation();

    assert_eq!(engine.record_file_change(), previous + 1);
    assert_eq!(engine.snapshot().await.unwrap().generation, previous + 1);
}

#[tokio::test]
async fn whole_deck_inspection_includes_added_shape_geometry() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("geometry.pptx");
    let engine = DeckEngine::create(&path, None).await.unwrap();
    engine
        .mutate(DeckMutation::Add {
            parent: "/slide[1]".into(),
            element_type: "rectangle".into(),
            properties: HashMap::from([
                ("x".into(), "1in".into()),
                ("y".into(), "0.5in".into()),
                ("width".into(), "2in".into()),
                ("height".into(), "1.5in".into()),
                ("text".into(), "Geometry target".into()),
            ]),
        })
        .await
        .unwrap();

    let inspected = engine.inspect(None).await.unwrap();
    let shape = inspected["slides"][0]["shapes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|shape| shape["text_preview"] == "Geometry target")
        .expect("added shape in inspection");
    assert_eq!(
        shape["geometry"],
        serde_json::json!({
            "x": 1.0,
            "y": 0.5,
            "width": 2.0,
            "height": 1.5,
            "unit": "in"
        })
    );
    assert!(inspected["slide_size"]["width"].as_f64().unwrap() > 0.0);
}
