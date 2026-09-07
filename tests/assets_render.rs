use rho_sdk::{
    model::{ContentBlock, Message, ModelIdentity, ModelResponse, ToolCall},
    provider::{ScriptedProvider, ScriptedTurn},
    Rho, Workspace,
};
use serde_json::{json, Value};
use slide_builder::{
    agent::{
        deck_engine::DeckEngine,
        policy::{PermissionMode, SlidePolicy},
        runtime::{register_deck_tools, AgentHandle},
    },
    render::{
        browser::{Browser, CaptureOptions},
        pipeline::build_capture_html,
    },
};
use std::{fs, path::Path};
use tokio::sync::mpsc;

async fn call(
    engine: &DeckEngine,
    name: &str,
    arguments: Value,
    mode: PermissionMode,
) -> Vec<Message> {
    let provider = ScriptedProvider::new(
        ModelIdentity::new("scripted", "test", "model"),
        [
            ScriptedTurn::completed(ModelResponse::Assistant(vec![ContentBlock::ToolCall(
                ToolCall {
                    id: "asset-call".into(),
                    name: name.into(),
                    arguments,
                },
            )])),
            ScriptedTurn::completed(ModelResponse::Assistant(vec![ContentBlock::Text(
                "done".into(),
            )])),
        ],
    );
    let root = engine.path().parent().unwrap();
    let builder = Rho::builder()
        .provider(provider.clone())
        .workspace(Workspace::new(root).unwrap())
        .workspace_policy(SlidePolicy::new(mode, root, root));
    let agent = AgentHandle::new(
        register_deck_tools(builder, engine.clone())
            .build()
            .unwrap(),
    )
    .await
    .unwrap();
    let (events, _) = mpsc::unbounded_channel();
    agent
        .send("execute the asset tool".into(), None, events)
        .await
        .unwrap();
    let requests = provider.recorded_requests();
    assert_eq!(requests.len(), 2);
    requests[1].messages.clone()
}

fn candidate() -> Value {
    json!({"name":"blue marker","brief":{"purpose":"Conceptual marker","style":"Blue silhouette, transparent background","alt_text":"Conceptual blue marker"},
        "svg":r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><rect x="10" y="10" width="80" height="30" fill="#1256ab"/></svg>"##})
}

async fn create_asset(engine: &DeckEngine) -> Value {
    let messages = call(
        engine,
        "asset_create_svg",
        candidate(),
        PermissionMode::Supervised,
    )
    .await;
    assert!(messages.iter().any(|message| matches!(message, Message::User(blocks) if blocks.iter().any(|block| matches!(block, ContentBlock::Image(_))))), "asset preview did not reach the next model request: {messages:?}");
    let store = engine.path().with_extension("pptx.assets");
    let record = fs::read_dir(store).unwrap().next().unwrap().unwrap().path();
    serde_json::from_slice::<Value>(&fs::read(record).unwrap()).unwrap()["id"].clone()
}

#[tokio::test]
async fn asset_tools_respect_plan_mode_and_attach_preview_in_normal_mode() {
    let directory = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(directory.path().join("assets.pptx"), None)
        .await
        .unwrap();
    let original = fs::read(engine.path()).unwrap();
    call(
        &engine,
        "asset_create_svg",
        candidate(),
        PermissionMode::Plan,
    )
    .await;
    assert!(!engine.path().with_extension("pptx.assets").exists());
    let id = create_asset(&engine).await;
    let placement = json!({"id":id,"slide":1,"x":1,"y":1,"width":4,"height":4});
    call(
        &engine,
        "asset_place",
        placement.clone(),
        PermissionMode::Plan,
    )
    .await;
    assert_eq!(fs::read(engine.path()).unwrap(), original);
    call(
        &engine,
        "asset_place",
        placement,
        PermissionMode::Supervised,
    )
    .await;
    assert_ne!(fs::read(engine.path()).unwrap(), original);
    assert!(engine
        .snapshot()
        .await
        .unwrap()
        .html
        .contains("data:image/png;base64,"));
}

#[tokio::test]
#[ignore = "requires a qualified native Obscura sandbox"]
async fn authored_asset_reaches_composed_slide_pixels() {
    let directory = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(directory.path().join("assets.pptx"), None)
        .await
        .unwrap();
    let id = create_asset(&engine).await;
    call(
        &engine,
        "asset_place",
        json!({"id":id,"slide":1,"x":1,"y":1,"width":4,"height":4}),
        PermissionMode::Supervised,
    )
    .await;
    let snapshot = engine.snapshot().await.unwrap();
    let options = CaptureOptions::default();
    let html = directory.path().join("capture.html");
    fs::write(
        &html,
        build_capture_html(&snapshot.html, 1, &options).unwrap(),
    )
    .unwrap();
    let output = directory.path().join("asset.png");
    Browser::with_embedded_worker(
        Path::new(env!("CARGO_BIN_EXE_slide-builder")),
        Path::new("auto"),
    )
    .unwrap()
    .capture(&html, &output, &directory.path().join("profile"), &options)
    .await
    .unwrap();
    let image = image::open(&output).unwrap().to_rgb8();
    // The 2:1 SVG is contained in (1,1,4,4) inches at (1,2,4,2).
    // Its center is (3,3); its transparent top-left is (1.1,2.1).
    let pixel = |x: f64, y: f64| {
        image
            .get_pixel(
                (x / snapshot.size_inches.0 * f64::from(options.width)) as u32,
                (y / snapshot.size_inches.1 * f64::from(options.height)) as u32,
            )
            .0
    };
    assert_eq!(pixel(3.0, 3.0), [18, 86, 171]);
    assert_eq!(pixel(1.1, 2.1), [255, 255, 255]);
}
