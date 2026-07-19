use super::enrich;

#[test]
fn picture_stable_id_uses_the_ooxml_non_visual_id() {
    let html = r#"
        <div class="picture" data-path="/slide[1]/picture[42]"
             style="left:72pt;top:36pt;width:144pt;height:108pt"></div>
    "#;
    let mut inspected = serde_json::json!({
        "slides": [{"path": "/slide[1]", "shapes": [], "shape_count": 0}]
    });

    enrich(&mut inspected, html);

    assert_eq!(inspected["slides"][0]["shapes"][0]["id"], "42");
    assert_eq!(
        inspected["slides"][0]["shapes"][0]["path"],
        "/slide[1]/picture[42]"
    );
}

#[test]
fn html_geometry_is_reported_in_inches() {
    let html = r#"
        <style>:root { --slide-design-w: 960pt; --slide-design-h: 540pt; }</style>
        <div class="shape" data-path="/slide[1]/shape[2]" title="Sun"
             style="left:72.00pt;top:36.00pt;width:144.00pt;height:108.00pt"></div>
    "#;
    let mut inspected = serde_json::json!({
        "slides": [{"shapes": [{"path": "/slide[1]/shape[2]"}]}]
    });

    enrich(&mut inspected, html);

    assert_eq!(
        inspected,
        serde_json::json!({
            "slides": [{"shapes": [{
                "path": "/slide[1]/shape[2]",
                "geometry": {
                    "x": 1.0,
                    "y": 0.5,
                    "width": 2.0,
                    "height": 1.5,
                    "unit": "in"
                }
            }]}],
            "slide_size": {"width": 13.3333, "height": 7.5, "unit": "in"}
        })
    );
}
