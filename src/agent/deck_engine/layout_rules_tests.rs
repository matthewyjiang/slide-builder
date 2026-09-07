use super::*;
use crate::agent::deck_engine::{DeckEngine, DeckMutation};
use std::collections::HashMap;

async fn add(engine: &DeckEngine, x: f64, y: f64) -> String {
    engine
        .mutate(DeckMutation::Add {
            parent: "/slide[1]".into(),
            element_type: "rectangle".into(),
            properties: HashMap::from([
                ("text".into(), "Evidence".into()),
                ("x".into(), format!("{x}in")),
                ("y".into(), format!("{y}in")),
                ("width".into(), "2in".into()),
                ("height".into(), "1in".into()),
            ]),
        })
        .await
        .unwrap()
        .affected[0]
        .clone()
}

#[tokio::test]
async fn declared_alignment_and_spacing_survive_reopen_and_detect_drift() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("relationships.pptx");
    let engine = DeckEngine::create(&path, None).await.unwrap();
    let a = add(&engine, 1.0, 1.0).await;
    let b = add(&engine, 4.0, 1.2).await;
    let c = add(&engine, 8.0, 0.9).await;
    for operation in [
        json!({"operation":"align","ids":[a,b,c],"edge":"top"}),
        json!({"operation":"distribute","ids":[a,b,c],"axis":"horizontal"}),
    ] {
        engine.layout_apply(operation).await.unwrap();
    }
    let reopened = DeckEngine::new(&path).unwrap();
    assert_eq!(
        reopened.layout_inspect().await.unwrap()["geometry_rules"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(reopened.layout_audit().await.unwrap()["issues"], json!([]));
    reopened
        .mutate(DeckMutation::Set {
            path: b.clone(),
            properties: HashMap::from([("x".into(), "4in".into()), ("y".into(), "1.1in".into())]),
        })
        .await
        .unwrap();
    let audit = reopened.layout_audit().await.unwrap();
    let kinds: Vec<_> = audit["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|issue| issue["rule"]["operation"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"align"), "{audit}");
    assert!(kinds.contains(&"distribute"), "{audit}");
    for rule in reopened.layout_inspect().await.unwrap()["geometry_rules"]
        .as_array()
        .unwrap()
    {
        reopened.layout_apply(rule.clone()).await.unwrap();
    }
    assert_eq!(reopened.layout_audit().await.unwrap()["issues"], json!([]));
}

#[tokio::test]
async fn newer_geometry_supersedes_conflicting_rules_and_release_preserves_content() {
    let directory = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(directory.path().join("release.pptx"), None)
        .await
        .unwrap();
    let a = add(&engine, 1.0, 1.0).await;
    let b = add(&engine, 4.0, 2.0).await;
    for edge in ["left", "right"] {
        engine
            .layout_apply(json!({"operation":"align","ids":[a,b],"edge":edge}))
            .await
            .unwrap();
    }
    let inspected = engine.layout_inspect().await.unwrap();
    let rules = inspected["geometry_rules"].as_array().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0]["edge"], "right");
    engine
        .layout_apply(json!({"operation":"align","ids":[a],"edge":"left"}))
        .await
        .unwrap();
    let rules = engine.layout_inspect().await.unwrap()["geometry_rules"].clone();
    assert_eq!(rules.as_array().unwrap().len(), 1);
    assert_eq!(rules[0]["ids"], json!([a]));
    let slide_xml = engine.raw("ppt/slides/slide1.xml".into()).await.unwrap();
    engine
        .layout_apply(json!({"operation":"release","ids":[a]}))
        .await
        .unwrap();
    assert_eq!(
        engine.layout_inspect().await.unwrap()["geometry_rules"],
        json!([])
    );
    assert_eq!(
        engine.raw("ppt/slides/slide1.xml".into()).await.unwrap(),
        slide_xml
    );

    engine
        .layout_apply(json!({"operation":"align","ids":[b],"reference":a,"edge":"top"}))
        .await
        .unwrap();
    engine
        .mutate(DeckMutation::Remove { path: a.clone() })
        .await
        .unwrap();
    assert_eq!(
        engine.layout_audit().await.unwrap()["issues"][0]["kind"],
        "geometry_rule_unavailable"
    );
    // A deleted reference must remain releasable, otherwise the deck can't be cleaned up.
    let released = engine
        .layout_apply(json!({"operation":"release","ids":[a]}))
        .await
        .unwrap();
    assert!(released.affected.is_empty());
    assert_eq!(engine.layout_audit().await.unwrap()["issues"], json!([]));
}

#[tokio::test]
async fn changed_named_region_is_reported_then_reapplied_without_rounding_noise() {
    let directory = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(directory.path().join("regions.pptx"), None)
        .await
        .unwrap();
    let ids = vec![
        add(&engine, 1.0, 1.0).await,
        add(&engine, 4.0, 1.0).await,
        add(&engine, 8.0, 1.0).await,
    ];
    let mut contract = json!({
        "margins":{"left":0.5,"right":0.5,"top":0.5,"bottom":0.5},
        "gutter":0.23,"regions":{"row":{"x":0.75,"y":2,"width":10.3333,"height":1}},
        "text_styles":{"body":{"font_size":24,"font_family":"Arial","color":"243746"}}
    });
    engine.layout_set(contract.clone()).await.unwrap();
    let placement = json!({"operation":"place","ids":ids,"region":"row","axis":"horizontal"});
    engine.layout_apply(placement.clone()).await.unwrap();
    engine
        .layout_apply(json!({"operation":"text_style","ids":ids,"style":"body"}))
        .await
        .unwrap();
    assert_eq!(engine.layout_audit().await.unwrap()["issues"], json!([]));
    contract["regions"]["row"]["y"] = json!(3);
    engine.layout_set(contract).await.unwrap();
    let audit = engine.layout_audit().await.unwrap();
    assert_eq!(audit["issues"].as_array().unwrap().len(), ids.len());
    engine.layout_apply(placement).await.unwrap();
    assert_eq!(engine.layout_audit().await.unwrap()["issues"], json!([]));
    engine
        .layout_apply(json!({"operation":"release","ids":ids}))
        .await
        .unwrap();
    let inspected = engine.layout_inspect().await.unwrap();
    assert_eq!(inspected["assignments"], json!({}));
    assert_eq!(inspected["geometry_rules"], json!([]));
}
