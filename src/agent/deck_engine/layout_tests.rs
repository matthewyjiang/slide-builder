use super::*;

fn contract() -> Value {
    json!({"margins":{"left":0.25,"right":0.25,"top":0.25,"bottom":0.25},"gutter":0.2,"regions":{"body":{"x":1.0,"y":1.0,"width":10.0,"height":4.0}},"text_styles":{"body":{"font_size":23.5,"font_family":"Aptos","color":"#aabbcc"}}})
}
async fn add(engine: &DeckEngine, slide: &str, x: f64, y: f64, w: f64) -> String {
    engine
        .mutate(DeckMutation::Add {
            parent: slide.into(),
            element_type: "rectangle".into(),
            properties: [
                ("text", "hello".to_owned()),
                ("x", format!("{x}in")),
                ("y", format!("{y}in")),
                ("width", format!("{w}in")),
                ("height", "1in".into()),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
        })
        .await
        .unwrap()
        .affected[0]
        .clone()
}
fn state(engine: &DeckEngine) -> State {
    State::read(&open(engine.path(), false).unwrap()).unwrap()
}

#[test]
fn metadata_declares_its_prefix_without_replacing_other_extensions() {
    let root = layout_xml::parse("<q:presentation xmlns:q=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><q:extLst><q:ext uri=\"other\"><foreign/></q:ext></q:extLst></q:presentation>").unwrap();
    let mutation = layout_xml::metadata_mutation(root, &Metadata::default()).unwrap();
    let DeckMutation::RawSet { xml: Some(xml), .. } = mutation else {
        panic!("expected XML replacement")
    };
    let result = layout_xml::parse(&xml).unwrap();
    assert_eq!(
        result.attr("xmlns:p").as_deref(),
        Some("http://schemas.openxmlformats.org/presentationml/2006/main")
    );
    assert!(xml.contains("uri=\"other\""));
    assert!(layout_xml::metadata(&result).unwrap().contract.is_none());
}

#[test]
fn bounds_check_uses_independently_rounded_emu_attributes() {
    let rect = Rect {
        x: 0.6 / 914_400.0,
        y: 0.0,
        width: 9.6 / 914_400.0,
        height: 1.0,
    };
    assert!(rect.validate((10.0 / 914_400.0, 1.0)).is_err());
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 0.1 / 914_400.0,
        height: 1.0,
    };
    assert!(rect.validate((1.0, 1.0)).is_err());
}

#[tokio::test]
async fn layout_round_trip_geometry_styles_and_audit() {
    let dir = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(dir.path().join("deck.pptx"), None)
        .await
        .unwrap();
    let a = add(&engine, "/slide[1]", 1.0, 1.0, 1.0).await;
    let b = add(&engine, "/slide[1]", 4.0, 2.0, 2.0).await;
    let c = add(&engine, "/slide[1]", 9.0, 3.0, 1.0).await;
    let normalized = engine.layout_set(contract()).await.unwrap();
    assert_eq!(
        normalized["contract"]["text_styles"]["body"],
        json!({"font_size":23.5,"font_family":"Aptos","color":"AABBCC","bold":false,"italic":false,"alignment":"left"})
    );
    engine
        .layout_apply(json!({"operation":"align","ids":[c,a,b],"edge":"top"}))
        .await
        .unwrap();
    engine
        .layout_apply(json!({"operation":"distribute","ids":[c,a,b],"axis":"horizontal"}))
        .await
        .unwrap();
    let s = state(&engine);
    assert_eq!(s.items[&b].rect().unwrap().x, 4.5);
    assert_eq!(s.items[&c].rect().unwrap().y, 1.0);
    engine
        .layout_apply(
            json!({"operation":"match_size","ids":[a,c],"reference":b,"dimension":"width"}),
        )
        .await
        .unwrap();
    assert_eq!(state(&engine).items[&a].rect().unwrap().width, 2.0);
    engine
        .layout_apply(
            json!({"operation":"place","ids":[c,a,b],"region":"body","axis":"horizontal"}),
        )
        .await
        .unwrap();
    assert_eq!(state(&engine).items[&c].rect().unwrap().x, 1.0);
    engine
        .layout_apply(json!({"operation":"text_style","ids":[a,b,c],"style":"body"}))
        .await
        .unwrap();
    let reopened = DeckEngine::new(engine.path()).unwrap();
    assert_eq!(
        reopened.layout_inspect().await.unwrap()["contract"],
        normalized["contract"]
    );
    let audit = reopened.layout_audit().await.unwrap();
    assert_eq!(audit["issues"], json!([]), "{audit:#}");
    reopened
        .mutate(DeckMutation::Set {
            path: b.clone(),
            properties: std::collections::HashMap::from([("fontSize".into(), "30".into())]),
        })
        .await
        .unwrap();
    let audit = reopened.layout_audit().await.unwrap();
    assert!(audit["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["kind"] == "text_style_departure" && i["id"] == b));
    assert_eq!(
        reopened.layout_inspect().await.unwrap()["assignments"][&a],
        "body"
    );
    assert_eq!(reopened.validate().await.unwrap()["valid"], true);
}

#[tokio::test]
async fn invalid_plans_and_contracts_are_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(dir.path().join("deck.pptx"), None)
        .await
        .unwrap();
    let a = add(&engine, "/slide[1]", 1.0, 1.0, 3.0).await;
    let b = add(&engine, "/slide[1]", 2.0, 2.0, 3.0).await;
    engine.layout_set(contract()).await.unwrap();
    let before = std::fs::read(engine.path()).unwrap();
    let generation = engine.generation();
    for args in [
        json!({"operation":"align","ids":[a,a],"edge":"left"}),
        json!({"operation":"align","ids":[a,"missing"],"edge":"left"}),
        json!({"operation":"distribute","ids":[a,b],"axis":"horizontal"}),
        json!({"operation":"distribute","ids":[a,b],"axis":"horizontal","gap":100}),
        json!({"operation":"place","ids":[a,b],"region":"body","axis":"horizontal","gap":20}),
        json!({"operation":"text_style","ids":[a],"style":"missing"}),
    ] {
        assert!(engine.layout_apply(args.clone()).await.is_err(), "{args}");
        assert_eq!(std::fs::read(engine.path()).unwrap(), before);
        assert_eq!(engine.generation(), generation);
    }
    let mut bad = contract();
    bad["text_styles"]["body"]
        .as_object_mut()
        .unwrap()
        .remove("font_family");
    assert!(engine.layout_set(bad).await.is_err());
    let mut bad = contract();
    bad["regions"]["body"]["width"] = 100.into();
    assert!(engine.layout_set(bad).await.is_err());
    assert_eq!(std::fs::read(engine.path()).unwrap(), before);
}

#[tokio::test]
async fn custom_dimensions_reference_and_cross_slide_style() {
    let dir = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(dir.path().join("deck.pptx"), None)
        .await
        .unwrap();
    engine
        .mutate(DeckMutation::RawSet {
            part: "ppt/presentation.xml".into(),
            xpath: "/presentation/sldSz".into(),
            action: "replace".into(),
            xml: Some("<p:sldSz cx=\"14630400\" cy=\"8229600\"/>".into()),
        })
        .await
        .unwrap();
    assert_eq!(engine.slide_size().await.unwrap(), (16.0, 9.0));
    let a = add(&engine, "/slide[1]", 13.0, 7.0, 1.0).await;
    let b = add(&engine, "/slide[1]", 14.0, 7.0, 2.0).await;
    engine.layout_set(contract()).await.unwrap();
    engine
        .layout_apply(json!({"operation":"align","ids":[a],"reference":b,"edge":"right"}))
        .await
        .unwrap();
    assert_eq!(state(&engine).items[&a].rect().unwrap().x, 15.0);
    engine
        .mutate(DeckMutation::Add {
            parent: "/".into(),
            element_type: "slide".into(),
            properties: Default::default(),
        })
        .await
        .unwrap();
    let c = add(&engine, "/slide[2]", 1.0, 1.0, 2.0).await;
    assert!(engine
        .layout_apply(json!({"operation":"align","ids":[a,c],"edge":"top"}))
        .await
        .is_err());
    assert!(engine
        .layout_apply(json!({"operation":"match_size","ids":[a],"reference":c,"dimension":"both"}))
        .await
        .is_err());
    engine
        .layout_apply(json!({"operation":"text_style","ids":[a,c],"style":"body"}))
        .await
        .unwrap();
    engine
        .mutate(DeckMutation::Move {
            source: "/slide[2]".into(),
            target_parent: None,
            index: Some(1),
        })
        .await
        .unwrap();
    engine
        .layout_apply(json!({"operation":"align","ids":[c],"edge":"left"}))
        .await
        .unwrap();
    let audit = engine.layout_audit().await.unwrap();
    assert!(
        !audit["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["kind"] == "text_style_departure"),
        "{audit:#}"
    );
}

#[tokio::test]
async fn bounds_audit_without_contract_and_repair_after_resize() {
    let dir = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(dir.path().join("deck.pptx"), None)
        .await
        .unwrap();
    let id = add(&engine, "/slide[1]", 12.0, 1.0, 3.0).await;
    let audit = engine.layout_audit().await.unwrap();
    assert_eq!(audit["contract_present"], false);
    assert!(audit["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["kind"] == "slide_bounds_departure" && issue["id"] == id));
    engine.layout_set(contract()).await.unwrap();
    engine
        .mutate(DeckMutation::RawSet {
            part: "ppt/presentation.xml".into(),
            xpath: "/presentation/sldSz".into(),
            action: "replace".into(),
            xml: Some("<p:sldSz cx=\"9144000\" cy=\"6400800\"/>".into()),
        })
        .await
        .unwrap();
    assert!(engine.layout_inspect().await.unwrap()["contract"].is_object());
    let audit = engine.layout_audit().await.unwrap();
    assert!(audit["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["kind"] == "invalid_contract"));
    // A stale region must not block unrelated alignment or text-style repairs.
    let anchor = add(&engine, "/slide[1]", 1.0, 1.0, 3.0).await;
    engine
        .layout_apply(json!({"operation":"align","ids":[id],"reference":anchor,"edge":"left"}))
        .await
        .unwrap();
    engine
        .layout_apply(json!({"operation":"text_style","ids":[id],"style":"body"}))
        .await
        .unwrap();
    assert_eq!(state(&engine).items[&id].rect().unwrap().x, 1.0);
    let mut repaired = contract();
    repaired["regions"]["body"]["width"] = 8.into();
    engine.layout_set(repaired).await.unwrap();
    engine
        .layout_apply(json!({"operation":"place","ids":[id],"region":"body","axis":"vertical"}))
        .await
        .unwrap();
    assert_eq!(engine.layout_audit().await.unwrap()["valid"], true);
}

#[tokio::test]
async fn picture_geometry_and_mixed_style_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(dir.path().join("deck.pptx"), None)
        .await
        .unwrap();
    let image_path = dir.path().join("image.png");
    image::RgbImage::from_pixel(2, 2, image::Rgb([0, 48, 87]))
        .save(&image_path)
        .unwrap();
    let shape = add(&engine, "/slide[1]", 1.0, 1.0, 2.0).await;
    let picture = engine
        .mutate(DeckMutation::Add {
            parent: "/slide[1]".into(),
            element_type: "image".into(),
            properties: std::collections::HashMap::from([
                ("path".into(), image_path.display().to_string()),
                ("x".into(), "4in".into()),
                ("y".into(), "3in".into()),
                ("width".into(), "1in".into()),
                ("height".into(), "2in".into()),
            ]),
        })
        .await
        .unwrap()
        .affected[0]
        .clone();
    engine.layout_set(contract()).await.unwrap();
    engine
        .layout_apply(json!({"operation":"align","ids":[picture],"reference":shape,"edge":"top"}))
        .await
        .unwrap();
    assert_eq!(state(&engine).items[&picture].rect().unwrap().y, 1.0);
    engine
        .layout_apply(
            json!({"operation":"match_size","ids":[picture],"reference":shape,"dimension":"both"}),
        )
        .await
        .unwrap();
    assert_eq!(state(&engine).items[&picture].rect().unwrap().width, 2.0);
    let before = std::fs::read(engine.path()).unwrap();
    assert!(engine
        .layout_apply(json!({"operation":"text_style","ids":[shape,picture],"style":"body"}))
        .await
        .is_err());
    assert_eq!(std::fs::read(engine.path()).unwrap(), before);
}

#[tokio::test]
async fn independently_opened_handles_serialize_layout_and_ordinary_edits() {
    let dir = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(dir.path().join("deck.pptx"), None)
        .await
        .unwrap();
    let id = add(&engine, "/slide[1]", 1.0, 1.0, 2.0).await;
    engine.layout_set(contract()).await.unwrap();
    let other = DeckEngine::new(engine.path()).unwrap();
    let generation = engine.generation();
    let (layout, edit) = tokio::join!(
        engine.layout_apply(json!({"operation":"text_style","ids":[id],"style":"body"})),
        other.mutate(DeckMutation::Set {
            path: id.clone(),
            properties: std::collections::HashMap::from([("text".into(), "changed".into())])
        })
    );
    layout.unwrap();
    edit.unwrap();
    assert_eq!(engine.generation(), generation + 2);
    assert_eq!(other.generation(), engine.generation());
    assert_eq!(
        other.layout_inspect().await.unwrap()["assignments"][&id],
        "body"
    );
    assert!(other.snapshot().await.unwrap().outline.contains("changed"));
}

#[tokio::test]
async fn structural_validation_parses_unrendered_xml_parts() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(dir.path().join("deck.pptx"), None)
        .await
        .unwrap();
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(engine.path())
        .unwrap();
    let mut archive = zip::ZipWriter::new_append(file).unwrap();
    archive
        .start_file(
            "customXml/broken.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    archive.write_all(b"<root><unclosed></root>").unwrap();
    archive.finish().unwrap();
    let validation = engine.validate().await.unwrap();
    assert_eq!(validation["valid"], false);
    assert!(validation["errors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|error| error["part"] == "customXml/broken.xml"));
}
