use super::*;
use std::{fs, io::Read};

fn request() -> Value {
    json!({"name":"rover & wheels <icon>", "brief":{"purpose":"Conceptual rover marker","style":"Solid blue, transparent background","alt_text":"Conceptual rover body & wheels <illustration>"},
        "svg":r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><rect x="10" y="10" width="80" height="30" fill="#1256ab"/></svg>"##})
}

#[tokio::test]
async fn create_inspect_place_reopen_keeps_vector_fallback_and_geometry() {
    let directory = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(directory.path().join("assets.pptx"), None)
        .await
        .unwrap();
    let original = fs::read(engine.path()).unwrap();
    let (created, preview) = execute(Action::Create, &engine, request()).await.unwrap();
    assert!(image::load_from_memory(&preview.unwrap()).is_ok());
    assert_eq!(fs::read(engine.path()).unwrap(), original);
    let id = created["id"].clone();
    let (listed, _) = execute(Action::List, &engine, json!({})).await.unwrap();
    assert_eq!(listed["assets"][0]["id"], id);
    let (inspected, preview) = execute(Action::Inspect, &engine, json!({"id":id}))
        .await
        .unwrap();
    assert_eq!(inspected["svg"], request()["svg"]);
    assert!(preview.is_some());

    let (placed, _) = execute(
        Action::Place,
        &engine,
        json!({"id":id,"slide":1,"x":1,"y":1,"width":4,"height":4}),
    )
    .await
    .unwrap();
    assert_eq!(
        placed["placement_inches"],
        json!({"x":1.0,"y":2.0,"width":4.0,"height":2.0})
    );
    assert_eq!(placed["visual_review"], "required");
    assert_eq!(placed["mutation"]["affected"].as_array().unwrap().len(), 1);
    assert!(placed["mutation"]["affected"][0]
        .as_str()
        .unwrap()
        .contains("/picture:"));

    let reopened = DeckEngine::new(engine.path()).unwrap();
    let snapshot = reopened.snapshot().await.unwrap();
    assert!(snapshot.html.contains("data:image/png;base64,"));
    let mut zip = zip::ZipArchive::new(fs::File::open(engine.path()).unwrap()).unwrap();
    let mut png = Vec::new();
    let mut svg = String::new();
    let mut slide = String::new();
    let names: Vec<_> = zip.file_names().map(str::to_owned).collect();
    for name in names {
        if name.starts_with("ppt/media/") && name.ends_with(".png") {
            zip.by_name(&name).unwrap().read_to_end(&mut png).unwrap();
        }
        if name.starts_with("ppt/media/") && name.ends_with(".svg") {
            zip.by_name(&name)
                .unwrap()
                .read_to_string(&mut svg)
                .unwrap();
        }
    }
    zip.by_name("ppt/slides/slide1.xml")
        .unwrap()
        .read_to_string(&mut slide)
        .unwrap();
    assert_eq!(svg, request()["svg"].as_str().unwrap());
    assert!(image::load_from_memory(&png).is_ok());
    assert!(slide.contains("svgBlip"));
    let document = resvg::usvg::roxmltree::Document::parse(&slide).unwrap();
    assert!(document.descendants().any(
        |node| node.attribute("descr") == Some("Conceptual rover body & wheels <illustration>")
    ));
    let name = format!("rover & wheels <icon> [asset:{}]", id.as_str().unwrap());
    assert!(document
        .descendants()
        .any(|node| node.attribute("name") == Some(name.as_str())));
    assert_eq!(slide.matches("<p:pic>").count(), 1);
}

#[tokio::test]
async fn source_larger_than_text_budget_places_without_changing_media_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(directory.path().join("large-source.pptx"), None)
        .await
        .unwrap();
    let mut args = request();
    let source = args["svg"].as_str().unwrap().replace(
        "</svg>",
        &format!(
            "<!--{}--></svg>",
            "x".repeat(crate::agent::deck_engine::MAX_TEXT_BYTES)
        ),
    );
    args["svg"] = json!(source);
    let (created, _) = execute(Action::Create, &engine, args).await.unwrap();
    execute(
        Action::Place,
        &engine,
        json!({"id":created["id"],"slide":1,"x":1,"y":1,"width":2,"height":2}),
    )
    .await
    .unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(engine.path()).unwrap()).unwrap();
    let name = zip
        .file_names()
        .find(|name| name.ends_with(".svg"))
        .unwrap()
        .to_owned();
    let mut actual = String::new();
    zip.by_name(&name)
        .unwrap()
        .read_to_string(&mut actual)
        .unwrap();
    assert_eq!(actual, source);
    assert!(DeckEngine::new(engine.path())
        .unwrap()
        .snapshot()
        .await
        .unwrap()
        .html
        .contains("data:image/png;base64,"));
}

#[tokio::test]
async fn advanced_add_rejects_unvalidated_svg_attachment_without_committing() {
    use crate::agent::deck_engine::DeckMutation;
    let directory = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(directory.path().join("unsafe-svg.pptx"), None)
        .await
        .unwrap();
    let original = fs::read(engine.path()).unwrap();
    let error = engine
        .mutate(DeckMutation::Add {
            parent: "/slide[1]".into(),
            element_type: "image".into(),
            properties: HashMap::from([(
                "svgSource".into(),
                "<svg><script>alert(1)</script></svg>".into(),
            )]),
        })
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("unvalidated media property"),
        "{error:#}"
    );
    assert_eq!(engine.generation(), 0);
    assert_eq!(fs::read(engine.path()).unwrap(), original);
}

#[tokio::test]
async fn invalid_placement_and_tampered_asset_leave_deck_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(directory.path().join("assets.pptx"), None)
        .await
        .unwrap();
    let (created, _) = execute(Action::Create, &engine, request()).await.unwrap();
    let id = created["id"].clone();
    let original = fs::read(engine.path()).unwrap();
    for args in [
        json!({"id":id,"slide":1,"x":-1,"y":1,"width":2,"height":2}),
        json!({"id":id,"slide":999,"x":1,"y":1,"width":2,"height":2}),
        json!({"id":id,"slide":1,"x":100,"y":1,"width":2,"height":2}),
        json!({"id":"../../escape","slide":1,"x":1,"y":1,"width":2,"height":2}),
    ] {
        assert!(execute(Action::Place, &engine, args).await.is_err());
        assert_eq!(fs::read(engine.path()).unwrap(), original);
    }
    let store = AssetStore::for_deck(engine.path());
    let path = store.root().join(format!("{}.json", id.as_str().unwrap()));
    let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    record["svg"] = json!("<svg><script/></svg>");
    fs::write(path, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(execute(
        Action::Place,
        &engine,
        json!({"id":id,"slide":1,"x":1,"y":1,"width":2,"height":2})
    )
    .await
    .is_err());
    assert_eq!(fs::read(engine.path()).unwrap(), original);
}

#[tokio::test]
async fn native_asset_tool_attaches_preview_to_output() {
    use rho_sdk::{tool::tool_progress_channel, CancellationToken, ToolCallId};
    let directory = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(directory.path().join("assets.pptx"), None)
        .await
        .unwrap();
    let (created, _) = execute(Action::Create, &engine, request()).await.unwrap();
    let tool = AssetTool {
        action: Action::Inspect,
        engine,
    };
    let invocation = ToolInvocation::new(
        ToolCallId::from_string("inspect-asset").unwrap(),
        json!({"id":created["id"]}),
    );
    let (progress, _) = tool_progress_channel(std::num::NonZeroUsize::new(1).unwrap());
    let output = tool
        .call(
            invocation,
            ToolContext::new(None, CancellationToken::new(), progress),
        )
        .await
        .unwrap();
    let assets = output.presentation().assets();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].media_type(), "image/png");
    assert!(image::load_from_memory(assets[0].bytes()).is_ok());
}
