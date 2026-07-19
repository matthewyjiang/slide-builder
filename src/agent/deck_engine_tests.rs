use super::{reject_part, state, DeckEngine, DeckMutation, BLANK_DECK};

#[test]
fn traversal_rejected() {
    assert!(DeckEngine::new("../bad.pptx").is_err());
    assert!(reject_part("../x").is_err());
}

#[cfg(unix)]
#[test]
fn symlink_deck_rejected() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("target.pptx");
    std::fs::write(&target, BLANK_DECK).unwrap();
    let link = directory.path().join("link.pptx");
    symlink(&target, &link).unwrap();
    assert!(DeckEngine::new(link).is_err());
}

#[tokio::test]
async fn fixture_opens_and_snapshots() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("x.pptx");
    let engine = DeckEngine::create(&path, None).await.unwrap();
    let snapshot = engine.snapshot().await.unwrap();
    assert!(snapshot.html.contains("html"));
}

#[test]
fn stable_id_resolution_tracks_reordered_paths() {
    let outline = serde_json::json!({"slides": [
        {"path":"/slide[1]", "slide_id":"300", "shapes":[
            {"path":"/slide[1]/shape[1]", "id":"7"}
        ]}
    ]});
    assert_eq!(
        state::stable_id_for_path(&outline, "/slide[1]/shape[1]").as_deref(),
        Some("slide:300/shape:7")
    );
    assert_eq!(
        state::path_for_stable_id(&outline, "slide:300/shape:7").as_deref(),
        Some("/slide[1]/shape[1]")
    );
}

#[tokio::test]
async fn malformed_xml_mutation_is_rolled_back_without_advancing_generation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("malformed.pptx");
    let engine = DeckEngine::create(&path, None).await.unwrap();
    let before = std::fs::read(&path).unwrap();
    let generation = engine.generation();

    let result = engine
        .mutate(DeckMutation::RawSet {
            part: "ppt/slides/slide1.xml".into(),
            xpath: "/sld/cSld".into(),
            action: "replace".into(),
            xml: Some("<p:cSld><unclosed></p:cSld>".into()),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(engine.generation(), generation);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    engine.snapshot().await.unwrap();
}

#[tokio::test]
async fn failed_mutation_preserves_original() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("x.pptx");
    let engine = DeckEngine::create(&path, None).await.unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(engine
        .mutate(DeckMutation::Remove {
            path: "/slide[999]".into()
        })
        .await
        .is_err());
    assert_eq!(before, std::fs::read(&path).unwrap());
}
