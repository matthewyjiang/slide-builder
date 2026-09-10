use super::super::{
    layout::State,
    layout_contract::Metadata,
    layout_plan::Item,
    layout_xml::{self, Element},
};
use super::audit;
use crate::agent::deck_engine::{DeckEngine, DeckMutation};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};

fn emu(inches: f64) -> i64 {
    (inches * 914_400.0).round() as i64
}

#[allow(clippy::too_many_arguments)]
fn item(
    id: &str,
    part: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    body: &str,
    extras: &str,
) -> Item {
    let xml = format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{id}\"/></p:nvSpPr>\
         <p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>{extras}</p:spPr>\
         {body}</p:sp>",
        emu(x),
        emu(y),
        emu(width),
        emu(height)
    );
    Item {
        id: id.into(),
        part: part.into(),
        xml_id: id.into(),
        xml: layout_xml::parse(&xml).unwrap(),
    }
}

fn text_body(size: u32, paragraphs: &[&str]) -> String {
    let runs = paragraphs
        .iter()
        .map(|line| format!("<a:p><a:r><a:rPr sz=\"{size}\"/><a:t>{line}</a:t></a:r></a:p>"))
        .collect::<String>();
    format!("<p:txBody><a:bodyPr/>{runs}</p:txBody>")
}

fn broken_lines(size: u32, lines: &[&str]) -> String {
    let mut inner = String::from("<a:p>");
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            inner.push_str("<a:br/>");
        }
        inner.push_str(&format!(
            "<a:r><a:rPr sz=\"{size}\"/><a:t>{line}</a:t></a:r>"
        ));
    }
    inner.push_str("</a:p>");
    format!("<p:txBody><a:bodyPr/>{inner}</p:txBody>")
}

fn state(items: Vec<Item>) -> State {
    State {
        presentation: Element::new("presentation"),
        metadata: Metadata::default(),
        size: (13.333_333_333_3, 7.5),
        items: items
            .into_iter()
            .map(|item| (item.id.clone(), item))
            .collect::<BTreeMap<_, _>>(),
        slides: BTreeMap::new(),
    }
}

#[test]
fn partial_text_overlap_is_a_warning_and_skips_other_slides() {
    let audit = audit(&state(vec![
        item(
            "a",
            "ppt/slides/slide1.xml",
            1.0,
            1.0,
            4.0,
            1.5,
            &text_body(2400, &["Title"]),
            "",
        ),
        item(
            "b",
            "ppt/slides/slide1.xml",
            3.0,
            1.5,
            4.0,
            1.5,
            &text_body(1800, &["Body"]),
            "",
        ),
        item(
            "c",
            "ppt/slides/slide2.xml",
            1.0,
            1.0,
            4.0,
            1.5,
            &text_body(2400, &["Other"]),
            "",
        ),
    ]))
    .unwrap();
    assert_eq!(audit["issues"], json!([]));
    assert_eq!(audit["valid"], true);
    assert_eq!(
        audit["warnings"],
        json!([{
            "kind":"possible_overlap",
            "reason":"partial_text_intersection",
            "ids":["a","b"],
            "geometries":[
                {"x":1.0,"y":1.0,"width":4.0,"height":1.5},
                {"x":3.0,"y":1.5,"width":4.0,"height":1.5}
            ]
        }])
    );
}

#[test]
fn background_containment_and_touching_peers_are_not_overlaps() {
    let audit = audit(&state(vec![
        item(
            "bg",
            "ppt/slides/slide1.xml",
            0.0,
            0.0,
            13.333_333_333_3,
            7.5,
            "",
            "<a:solidFill><a:srgbClr val=\"112233\"/></a:solidFill>",
        ),
        item(
            "card",
            "ppt/slides/slide1.xml",
            0.75,
            2.0,
            5.5,
            4.0,
            "",
            "<a:solidFill><a:srgbClr val=\"334455\"/></a:solidFill>",
        ),
        item(
            "caption",
            "ppt/slides/slide1.xml",
            1.0,
            5.0,
            5.0,
            0.6,
            &text_body(1800, &["Caption"]),
            "<a:noFill/>",
        ),
        item(
            "left",
            "ppt/slides/slide1.xml",
            0.75,
            0.5,
            5.5,
            1.0,
            &text_body(2400, &["Left"]),
            "<a:noFill/>",
        ),
        item(
            "right",
            "ppt/slides/slide1.xml",
            6.25,
            0.5,
            5.5,
            1.0,
            &text_body(2400, &["Right"]),
            "<a:noFill/>",
        ),
    ]))
    .unwrap();
    assert_eq!(audit["issues"], json!([]));
    assert_eq!(audit["warnings"], json!([]));
    assert_eq!(audit["valid"], true);
}

#[test]
fn painted_partial_hits_warn_and_rotated_bounds_are_skipped() {
    let mut rotated = item(
        "badge",
        "ppt/slides/slide1.xml",
        2.0,
        2.0,
        1.0,
        1.0,
        "",
        "<a:solidFill><a:srgbClr val=\"990000\"/></a:solidFill>",
    );
    rotated.xml = layout_xml::parse(
        &rotated
            .xml
            .xml()
            .unwrap()
            .replace("<a:xfrm>", "<a:xfrm rot=\"2700000\">"),
    )
    .unwrap();
    let audit = audit(&state(vec![
        item(
            "card",
            "ppt/slides/slide1.xml",
            1.0,
            1.0,
            4.0,
            3.0,
            "",
            "<a:solidFill><a:srgbClr val=\"334455\"/></a:solidFill>",
        ),
        item(
            "neighbor",
            "ppt/slides/slide1.xml",
            4.5,
            2.0,
            3.0,
            2.0,
            "",
            "<a:solidFill><a:srgbClr val=\"445566\"/></a:solidFill>",
        ),
        rotated,
    ]))
    .unwrap();
    assert_eq!(audit["issues"], json!([]));
    assert_eq!(audit["valid"], true);
    assert_eq!(
        audit["warnings"],
        json!([{
            "kind":"possible_overlap",
            "reason":"partial_intersection",
            "ids":["card","neighbor"],
            "geometries":[
                {"x":1.0,"y":1.0,"width":4.0,"height":3.0},
                {"x":4.5,"y":2.0,"width":3.0,"height":2.0}
            ]
        }])
    );
}

#[test]
fn explicit_breaks_warn_and_unmeasured_or_unsupported_layouts_do_not() {
    let overflowing = item(
        "stack",
        "ppt/slides/slide1.xml",
        1.0,
        1.0,
        6.0,
        0.4,
        &broken_lines(4000, &["One", "Two", "Three"]),
        "",
    );
    let wrapped = item(
        "wrap",
        "ppt/slides/slide1.xml",
        1.0,
        3.0,
        1.0,
        0.6,
        &text_body(1800, &["A long headline that would wrap in a narrow box"]),
        "",
    );
    let autofit = item(
        "fit",
        "ppt/slides/slide1.xml",
        8.0,
        1.0,
        4.0,
        0.3,
        "<p:txBody><a:bodyPr><a:normAutofit/></a:bodyPr><a:p><a:r><a:rPr sz=\"4000\"/><a:t>One</a:t></a:r><a:br/><a:r><a:rPr sz=\"4000\"/><a:t>Two</a:t></a:r></a:p></p:txBody>",
        "",
    );
    let columns = item(
        "cols",
        "ppt/slides/slide1.xml",
        8.0,
        3.0,
        4.0,
        0.3,
        "<p:txBody><a:bodyPr numCol=\"2\"/><a:p><a:r><a:rPr sz=\"4000\"/><a:t>One</a:t></a:r></a:p><a:p><a:r><a:rPr sz=\"4000\"/><a:t>Two</a:t></a:r></a:p></p:txBody>",
        "",
    );
    let vertical = item(
        "vert",
        "ppt/slides/slide1.xml",
        8.0,
        4.0,
        0.4,
        2.0,
        "<p:txBody><a:bodyPr vert=\"vert\"/><a:p><a:r><a:rPr sz=\"4000\"/><a:t>Tall</a:t></a:r></a:p></p:txBody>",
        "",
    );
    let spaced = item(
        "spaced",
        "ppt/slides/slide1.xml",
        1.0,
        5.0,
        6.0,
        0.3,
        "<p:txBody><a:bodyPr/><a:p><a:pPr><a:lnSpc><a:spcPct val=\"150000\"/></a:lnSpc></a:pPr><a:r><a:rPr sz=\"4000\"/><a:t>One</a:t></a:r><a:br/><a:r><a:rPr sz=\"4000\"/><a:t>Two</a:t></a:r></a:p></p:txBody>",
        "",
    );
    let audit = audit(&state(vec![
        overflowing,
        wrapped,
        autofit,
        columns,
        vertical,
        spaced,
    ]))
    .unwrap();
    assert_eq!(audit["issues"], json!([]));
    assert_eq!(audit["valid"], true);
    assert_eq!(audit["warnings"].as_array().unwrap().len(), 1, "{audit}");
    assert_eq!(audit["warnings"][0]["kind"], "possible_text_overflow");
    assert_eq!(audit["warnings"][0]["id"], "stack");
    assert_eq!(audit["warnings"][0]["explicit_lines"], 3);
    assert_eq!(
        audit["warnings"][0]["basis"],
        "declared_font_size_and_explicit_breaks"
    );
}

#[allow(clippy::too_many_arguments)]
async fn add(
    engine: &DeckEngine,
    slide: &str,
    text: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    font_size: &str,
) -> String {
    engine
        .mutate(DeckMutation::Add {
            parent: slide.into(),
            element_type: "rectangle".into(),
            properties: HashMap::from([
                ("text".into(), text.into()),
                ("x".into(), format!("{x}in")),
                ("y".into(), format!("{y}in")),
                ("width".into(), format!("{width}in")),
                ("height".into(), format!("{height}in")),
                ("fontSize".into(), font_size.into()),
                ("font".into(), "Arial".into()),
            ]),
        })
        .await
        .unwrap()
        .affected[0]
        .clone()
}

#[tokio::test]
async fn deck_fixture_reports_poster_defects_then_clears_after_repair() {
    let directory = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(directory.path().join("poster.pptx"), None)
        .await
        .unwrap();
    let title = add(
        &engine,
        "/slide[1]",
        "Broken poster title",
        0.7,
        0.4,
        12.0,
        1.2,
        "40",
    )
    .await;
    let body = add(
        &engine,
        "/slide[1]",
        "Supporting copy that collides with the title",
        0.7,
        1.1,
        12.0,
        1.2,
        "24",
    )
    .await;
    let left = add(&engine, "/slide[1]", "Peer left", 0.75, 3.0, 5.5, 1.0, "24").await;
    let right = add(
        &engine,
        "/slide[1]",
        "Peer right",
        6.75,
        3.0,
        5.5,
        1.0,
        "24",
    )
    .await;
    let overflow = add(
        &engine,
        "/slide[1]",
        "Line one\nLine two\nLine three\nLine four",
        0.75,
        4.5,
        6.0,
        0.6,
        "32",
    )
    .await;
    engine
        .mutate(DeckMutation::Add {
            parent: "/".into(),
            element_type: "slide".into(),
            properties: HashMap::new(),
        })
        .await
        .unwrap();
    let other_title = add(
        &engine,
        "/slide[2]",
        "Second slide title",
        0.7,
        0.4,
        12.0,
        1.2,
        "40",
    )
    .await;
    let other_body = add(
        &engine,
        "/slide[2]",
        "Second slide collision",
        0.7,
        1.1,
        12.0,
        1.2,
        "24",
    )
    .await;

    let before = engine.layout_audit().await.unwrap();
    assert_eq!(before["issues"], json!([]), "{before}");
    assert_eq!(before["valid"], true);
    let overlap_pairs: Vec<_> = before["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|warning| warning["kind"] == "possible_overlap")
        .map(|warning| warning["ids"].clone())
        .collect();
    assert!(
        overlap_pairs.contains(&json!([body, title]))
            || overlap_pairs.contains(&json!([title, body])),
        "{before}"
    );
    assert!(
        overlap_pairs.contains(&json!([other_body, other_title]))
            || overlap_pairs.contains(&json!([other_title, other_body])),
        "{before}"
    );
    assert_eq!(overlap_pairs.len(), 2, "{before}");
    assert!(
        before["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["kind"] == "possible_text_overflow" && warning["id"] == overflow),
        "{before}"
    );
    assert!(
        !before["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["kind"] == "possible_overlap"
                && warning["ids"].as_array().unwrap().iter().any(|id| {
                    id.as_str() == Some(left.as_str()) || id.as_str() == Some(right.as_str())
                })),
        "{before}"
    );

    engine
        .mutate(DeckMutation::Set {
            path: body.clone(),
            properties: HashMap::from([("y".into(), "1.7in".into())]),
        })
        .await
        .unwrap();
    engine
        .mutate(DeckMutation::Set {
            path: overflow.clone(),
            properties: HashMap::from([("height".into(), "2.2in".into())]),
        })
        .await
        .unwrap();
    engine
        .mutate(DeckMutation::Set {
            path: other_body.clone(),
            properties: HashMap::from([("y".into(), "1.7in".into())]),
        })
        .await
        .unwrap();
    let after = engine.layout_audit().await.unwrap();
    assert_eq!(after["issues"], json!([]), "{after}");
    assert_eq!(after["warnings"], json!([]), "{after}");
    assert_eq!(after["valid"], true);
}
