use super::enrich;

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
