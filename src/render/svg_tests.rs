use super::*;

fn svg(body: &str) -> String {
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">{body}</svg>"#)
}

#[test]
fn renders_a_static_icon_to_a_transparent_png() {
    let validated = validate_and_render(&svg(
        r##"<g fill="none" stroke="#346699" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12 L10 18 L20 6"/></g>"##,
    )).unwrap();
    let decoded = tiny_skia::Pixmap::decode_png(&validated.png).unwrap();
    assert_eq!(
        (decoded.width(), decoded.height()),
        (validated.width, validated.height)
    );
    assert_eq!(validated.aspect_ratio, 1.0);
    assert!(validated.warnings.is_empty());
    assert_eq!(decoded.pixel(0, 0).unwrap().alpha(), 0);
    assert!(decoded.pixels().iter().any(|pixel| pixel.alpha() > 0));
}

#[test]
fn preserves_document_aspect_and_bounds_huge_source_dimensions() {
    let side = crate::render::browser::CaptureOptions::default()
        .width
        .max(crate::render::browser::CaptureOptions::default().height);
    for (view_box, body, ratio, dimensions) in [
        (
            "0 0 30 10",
            r#"<rect x="1" y="1" width="28" height="8"/>"#,
            3.0,
            (side, (f64::from(side) / 3.0).round() as u32),
        ),
        (
            "0 0 10 30",
            r#"<rect x="1" y="1" width="8" height="28"/>"#,
            1.0 / 3.0,
            ((f64::from(side) / 3.0).round() as u32, side),
        ),
        (
            "-1000000000 -500000000 2000000000 1000000000",
            r#"<ellipse rx="800000000" ry="400000000"/>"#,
            2.0,
            (side, side / 2),
        ),
    ] {
        let source =
            format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view_box}">{body}</svg>"#);
        let validated = validate_and_render(&source).unwrap();
        assert_eq!(validated.aspect_ratio, ratio);
        assert_eq!((validated.width, validated.height), dimensions);
    }
    let validated = validate_and_render(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="90px" height="30"><circle cx="12" cy="12" r="8"/></svg>"#).unwrap();
    assert_eq!(validated.aspect_ratio, 3.0);
    assert_eq!(
        (validated.width, validated.height),
        (side, (f64::from(side) / 3.0).round() as u32)
    );
}

#[test]
fn rejects_partial_document_dimensions() {
    for dimensions in [r#"width="90""#, r#"height="30""#] {
        let source = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" {dimensions}><circle cx="12" cy="12" r="4"/></svg>"#
        );
        assert_eq!(
            validate_and_render(&source).unwrap_err().to_string(),
            "SVG width and height must be provided together or both omitted"
        );
    }
}

#[test]
fn rejects_malformed_xml_and_invalid_geometry_even_beside_valid_artwork() {
    for source in [
        "",
        "not xml",
        "<svg>",
        "<svg></path>",
        r#"<svg><circle r="1"/></svg>"#,
        r#"<svg viewBox="0 0 0 24"/>"#,
        r#"<svg viewBox="0 0 -24 24"/>"#,
        r#"<svg viewBox="0 0 NaN 24"/>"#,
        r#"<svg viewBox="0 0 24 24 1"/>"#,
        r#"<svg viewBox="0 0 1e100 24"/>"#,
        r#"<svg viewBox="0 0 1e-100 24"/>"#,
    ] {
        let source = source.replacen("<svg", r#"<svg xmlns="http://www.w3.org/2000/svg""#, 1);
        assert!(validate_and_render(&source).is_err(), "accepted {source}");
    }
    for malformed in [
        r#"<rect width="-1" height="2"/>"#,
        r#"<rect width="0" height="2"/>"#,
        r#"<rect width="2"/>"#,
        r#"<circle r="nope"/>"#,
        r#"<ellipse rx="1" ry="-2"/>"#,
        r#"<circle r="1" cx="1e100"/>"#,
        r#"<circle r="1" cx="1e-100"/>"#,
        r#"<circle r="1" opacity="2"/>"#,
        r#"<circle r="1" transform="rotate(bad)"/>"#,
        r#"<circle r="1" transform="translate(1 2) garbage"/>"#,
        r#"<circle r="1" transform="matrix(1 0 0 1 1e100 0)"/>"#,
        r#"<path d="M1 1 L2 2 Lbroken"/>"#,
        r#"<path d="M1 1 L2 2 L"/>"#,
        r#"<path d="M1 1 L1e100 2"/>"#,
        r#"<path d="M1 1 L2 2 Z Z Z"/>"#,
        r#"<path d="M1 1 A-1 2 0 0 0 3 4"/>"#,
        r#"<path d=""/>"#,
        r#"<polygon points="1 1 2 2 3"/>"#,
        r#"<polyline points="1 1 nope"/>"#,
    ] {
        let source = svg(&format!(r#"<circle cx="12" cy="12" r="4"/>{malformed}"#));
        assert!(
            validate_and_render(&source).is_err(),
            "accepted {malformed}"
        );
    }
}

#[test]
fn rejects_active_external_and_unknown_svg_content() {
    for unsafe_body in [
        r#"<script>alert(1)</script>"#,
        r#"<foreignObject/>"#,
        r#"<image href="data:image/png;base64,AA=="/>"#,
        r#"<text x="1" y="1">hello</text>"#,
        r#"<style>circle { fill: red; }</style>"#,
        r##"<use href="#icon"/>"##,
        r#"<filter/>"#,
        r#"<animate/>"#,
        r#"<unknown/>"#,
        r#"<circle r="1" onload="alert(1)"/>"#,
        r#"<circle r="1" style="fill:red"/>"#,
        r#"<circle r="1" fill="url(https://example.com/a.svg)"/>"#,
        r#"<circle r="1" fill="u&#114;l(https://example.com/a.svg)"/>"#,
        r#"<circle r="1" fill="u\72l(https://example.com/a.svg)"/>"#,
        r##"<circle r="1" fill="url(#paint)"/>"##,
        r#"<circle r="1" fill="red; background:url(file:///tmp/a)"/>"#,
        r#"<circle r="1" href="file:///tmp/a"/>"#,
        r#"<circle r="1" unknown="1"/>"#,
        r#"<circle r="1" xml:base="https://example.com"/>"#,
        r#"<circle xmlns:other="https://example.com" r="1"/>"#,
        r#"<other:circle xmlns:other="https://example.com" r="1"/>"#,
        r#"<circle xmlns="" r="1"/>"#,
        r#"<circle xmlns:xlink="http://www.w3.org/1999/xlink" xlink:href="/tmp/a" r="1"/>"#,
        r#"<svg viewBox="0 0 1 1"/>"#,
        r#"<?xml-stylesheet href="https://example.com/a.css"?>"#,
        r#"<g opacity="0.5"><circle r="1"/></g>"#,
        r#"<path d="M0 0L24 24" stroke="red" stroke-dasharray="0.000000001 0.000000001"/>"#,
    ] {
        let source = svg(&format!(r#"<circle cx="12" cy="12" r="4"/>{unsafe_body}"#));
        assert!(
            validate_and_render(&source).is_err(),
            "accepted {unsafe_body}"
        );
    }
    for source in [
        r#"<!DOCTYPE svg [<!ENTITY fill "red">]><svg viewBox="0 0 24 24"><circle r="1" fill="&fill;"/></svg>"#,
        r#"<!DOCTYPE svg SYSTEM "file:///tmp/a"><svg viewBox="0 0 24 24"/>"#,
        r#"<svg xmlns="https://example.com" viewBox="0 0 24 24"/>"#,
        r#"<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="4"/></svg>"#,
    ] {
        assert!(validate_and_render(source).is_err(), "accepted {source}");
    }
}

#[test]
fn rejects_empty_and_invisible_renders() {
    for body in [
        "",
        "<g/>",
        r#"<circle cx="12" cy="12" r="4" fill="none"/>"#,
        r#"<circle cx="12" cy="12" r="4" opacity="0"/>"#,
        r#"<circle cx="12" cy="12" r="4" visibility="hidden"/>"#,
        r#"<circle cx="12" cy="12" r="4" display="none"/>"#,
        r#"<circle cx="120" cy="120" r="4"/>"#,
        r#"<path d="M12 12"/>"#,
    ] {
        let error = validate_and_render(&svg(body)).unwrap_err();
        assert!(
            error.to_string().contains("no visible artwork"),
            "{body}: {error:#}"
        );
    }
}

#[test]
fn warns_only_when_visible_pixels_reach_the_boundary() {
    let validated = validate_and_render(&svg(r#"<rect width="24" height="24"/>"#)).unwrap();
    assert_eq!(
        validated.warnings,
        vec!["Artwork reaches the preview boundary; check for possible clipping."]
    );
    let validated =
        validate_and_render(&svg(r#"<rect x="2" y="2" width="20" height="20"/>"#)).unwrap();
    assert!(validated.warnings.is_empty());
}

#[test]
fn renders_maximum_nesting_with_transforms_and_shape_opacity() {
    let groups = MAX_SVG_DEPTH - 1;
    let source = svg(&format!(
        "{}<circle cx=\"12\" cy=\"12\" r=\"4\" opacity=\"0.5\"/>{}",
        "<g transform=\"rotate(1 12 12)\" opacity=\"1\">".repeat(groups),
        "</g>".repeat(groups)
    ));
    let validated = validate_and_render(&source).unwrap();
    let decoded = tiny_skia::Pixmap::decode_png(&validated.png).unwrap();
    assert!(decoded.pixels().iter().any(|pixel| pixel.alpha() > 0));
    assert!(validated.warnings.is_empty());
}

#[test]
fn nesting_and_input_budgets_report_limit_and_asked() {
    // Exercise both Empty and Start events at the boundary, plus the original
    // 1025-group input that overflowed roxmltree before validation ran.
    for groups in [MAX_SVG_DEPTH, MAX_SVG_DEPTH + 1, 1025] {
        let source = svg(&format!(
            "{}<circle r=\"1\"/>{}",
            "<g>".repeat(groups),
            "</g>".repeat(groups)
        ));
        assert_eq!(
            validate_and_render(&source).unwrap_err().to_string(),
            format!(
                "SVG nesting budget exceeded: limit {MAX_SVG_DEPTH}, asked {}",
                MAX_SVG_DEPTH + 1
            )
        );
    }
    let source = " ".repeat(MAX_MEDIA_BYTES as usize + 1);
    let error = validate_and_render(&source).unwrap_err().to_string();
    assert!(
        error.contains(&format!("limit {MAX_MEDIA_BYTES}, asked {}", source.len())),
        "{error}"
    );
}
