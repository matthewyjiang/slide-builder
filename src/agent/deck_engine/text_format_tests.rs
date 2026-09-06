use super::*;

fn elements<'a>(nodes: &'a [Node], name: &str) -> Vec<&'a Element> {
    let mut found = Vec::new();
    for node in nodes {
        if let Node::Element(element) = node {
            if element.named(name) {
                found.push(element);
            }
            found.extend(elements(&element.children, name));
        }
    }
    found
}

fn attribute(element: &Element, name: &str) -> Option<String> {
    element
        .start
        .try_get_attribute(name)
        .unwrap()
        .map(|attribute| attribute.unescape_value().unwrap().into_owned())
}

#[test]
fn formats_every_run_and_paragraph_without_discarding_other_properties() {
    let xml = r#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><p:sp><p:spPr><a:solidFill><a:srgbClr val="111111"/></a:solidFill></p:spPr><p:txBody><a:p><a:pPr marL="12"><a:buNone/></a:pPr><a:r><a:rPr lang="en-US" u="sng"><a:ln w="42"/><a:solidFill><a:schemeClr val="accent1"/></a:solidFill><a:latin typeface="Old" pitchFamily="34"/><a:hlinkClick r:id="rId7"/></a:rPr><a:t>One &amp; two</a:t></a:r><a:r><a:rPr i="1"/><a:t>Three</a:t></a:r><a:fld id="field"><a:t>4</a:t></a:fld><a:endParaRPr lang="fr-FR"/></a:p><a:p><a:r><a:t>Five</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:txBody><a:p><a:r><a:t>Untouched</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#;
    let properties = HashMap::from([
        ("color".to_owned(), "#F4F7FA".to_owned()),
        ("font".to_owned(), "A & B \"Sans\"".to_owned()),
        ("fontSize".to_owned(), "44".to_owned()),
        ("bold".to_owned(), "true".to_owned()),
        ("alignment".to_owned(), "center".to_owned()),
    ]);
    let formatted = format_shape(xml, 1, &properties).unwrap();
    let formatted = format_shape(
        &formatted,
        1,
        &HashMap::from([("color".to_owned(), "ABCDEF".to_owned())]),
    )
    .unwrap();
    let nodes = parse(&mut Reader::from_str(&formatted)).unwrap();
    let shapes = elements(&nodes, "p:sp");
    let first = shapes[0];
    let runs = elements(&first.children, "a:rPr");
    assert_eq!(runs.len(), 4);
    for run in &runs {
        assert_eq!(attribute(run, "b").as_deref(), Some("1"));
        assert_eq!(attribute(run, "sz").as_deref(), Some("4400"));
        let fills = elements(&run.children, "a:solidFill");
        assert_eq!(fills.len(), 1);
        assert_eq!(
            attribute(elements(&fills[0].children, "a:srgbClr")[0], "val").as_deref(),
            Some("ABCDEF")
        );
        for name in ["a:latin", "a:ea", "a:cs"] {
            assert_eq!(
                attribute(elements(&run.children, name)[0], "typeface").as_deref(),
                Some("A & B \"Sans\"")
            );
        }
    }
    assert_eq!(attribute(runs[0], "u").as_deref(), Some("sng"));
    assert_eq!(attribute(runs[1], "i").as_deref(), Some("1"));
    assert_eq!(
        attribute(elements(&runs[0].children, "a:latin")[0], "pitchFamily").as_deref(),
        Some("34")
    );
    assert_eq!(elements(&runs[0].children, "a:hlinkClick").len(), 1);
    let paragraphs = elements(&first.children, "a:pPr");
    assert_eq!(paragraphs.len(), 2);
    for paragraph in &paragraphs {
        assert_eq!(attribute(paragraph, "algn").as_deref(), Some("ctr"));
    }
    assert_eq!(attribute(paragraphs[0], "marL").as_deref(), Some("12"));
    assert!(formatted.contains("<a:t>One &amp; two</a:t>"));
    assert!(formatted
        .contains("<p:spPr><a:solidFill><a:srgbClr val=\"111111\"/></a:solidFill></p:spPr>"));
    let mut untouched = Writer::new(Vec::new());
    shapes[1].write(&mut untouched).unwrap();
    assert_eq!(
        String::from_utf8(untouched.into_inner()).unwrap(),
        "<p:sp><p:txBody><a:p><a:r><a:t>Untouched</a:t></a:r></a:p></p:txBody></p:sp>"
    );
    // Every text fill belongs to run properties, never beside rPr under a:r.
    for run in elements(&first.children, "a:r") {
        assert!(!run
            .children
            .iter()
            .any(|node| matches!(node, Node::Element(element) if element.named("a:solidFill"))));
    }
}

#[test]
fn repeated_formatting_is_idempotent() {
    let xml = "<p:sld><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:rPr/><a:t>Text</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>";
    let properties = HashMap::from([
        ("fontName".to_owned(), "Arial".to_owned()),
        ("fontColor".to_owned(), "FFFFFF".to_owned()),
        ("italic".to_owned(), "false".to_owned()),
        ("alignment".to_owned(), "right".to_owned()),
    ]);
    let first = format_shape(xml, 1, &properties).unwrap();
    assert_eq!(format_shape(&first, 1, &properties).unwrap(), first);
}

#[test]
fn repairs_legacy_misplaced_color_and_rejects_invalid_sizes() {
    let xml = "<p:sld><p:cSld><p:spTree><p:sp><a:p><a:r><a:rPr/><a:solidFill><a:srgbClr val=\"000000\"/></a:solidFill><a:t>Text</a:t></a:r></a:p></p:sp></p:spTree></p:cSld></p:sld>";
    let properties = HashMap::from([("color".to_owned(), "FFFFFF".to_owned())]);
    let formatted = format_shape(xml, 1, &properties).unwrap();
    assert!(formatted
        .contains("<a:rPr><a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill></a:rPr><a:t>"));
    assert!(!formatted.contains("000000"));
    for size in ["NaN", "inf", "-1", "0", "4001"] {
        let error = format_shape(
            xml,
            1,
            &HashMap::from([("fontSize".to_owned(), size.to_owned())]),
        )
        .unwrap_err();
        assert!(error.to_string().contains("fontSize"), "{error}");
    }
}

#[tokio::test]
async fn saved_add_and_set_render_requested_text_styles() {
    use crate::agent::deck_engine::{DeckEngine, DeckMutation};
    let directory = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(&directory.path().join("text.pptx"), None)
        .await
        .unwrap();
    engine
        .mutate(DeckMutation::Add {
            parent: "/slide[1]".into(),
            element_type: "shape".into(),
            properties: HashMap::from([
                ("text".into(), "Visible title".into()),
                ("font".into(), "Arial".into()),
                ("fontSize".into(), "44".into()),
                ("color".into(), "F4F7FA".into()),
                ("bold".into(), "true".into()),
                ("alignment".into(), "center".into()),
            ]),
        })
        .await
        .unwrap();
    let html = engine.snapshot().await.unwrap().html;
    for expected in [
        "font-family:'Arial',sans-serif",
        "font-size:44.00pt",
        "color:#F4F7FA",
        "font-weight:bold",
        "text-align:center",
    ] {
        assert!(html.contains(expected), "missing {expected}: {html}");
    }
    engine
        .mutate(DeckMutation::Set {
            path: "/slide[1]/shape[1]".into(),
            properties: HashMap::from([
                ("fontName".into(), "Georgia".into()),
                ("fontSize".into(), "24".into()),
                ("fontColor".into(), "ABCDEF".into()),
                ("bold".into(), "false".into()),
                ("italic".into(), "true".into()),
                ("alignment".into(), "right".into()),
            ]),
        })
        .await
        .unwrap();
    let html = engine.snapshot().await.unwrap().html;
    for expected in [
        "font-family:'Georgia',sans-serif",
        "font-size:24.00pt",
        "color:#ABCDEF",
        "font-style:italic",
        "text-align:right",
    ] {
        assert!(html.contains(expected), "missing {expected}: {html}");
    }
    assert!(!html.contains("font-weight:bold"));
}

#[test]
fn shape_selector_skips_grouped_shapes_and_rejects_unsupported_prefixes() {
    let grouped = "<p:grpSp><p:sp><p:txBody><a:p><a:r><a:t>Grouped</a:t></a:r></a:p></p:txBody></p:sp></p:grpSp>";
    let xml = format!("<p:sld><p:cSld><p:spTree>{grouped}<p:sp><p:txBody><a:p><a:r><a:t>Target</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>");
    let properties = HashMap::from([("bold".to_owned(), "true".to_owned())]);
    let formatted = format_shape(&xml, 1, &properties).unwrap();
    assert!(formatted.contains(grouped));
    assert!(formatted.contains("<a:rPr b=\"1\"/><a:t>Target</a:t>"));
    assert!(format_shape(&xml, 2, &properties).is_err());
    let aliased = xml
        .replace(
            "<p:sld>",
            "<p:sld xmlns:d=\"http://schemas.openxmlformats.org/drawingml/2006/main\">",
        )
        .replace("a:", "d:");
    let error = format_shape(&aliased, 1, &properties).unwrap_err();
    assert!(error.to_string().contains("namespace prefixes"));
}
