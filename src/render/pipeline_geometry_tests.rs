use super::*;

#[test]
fn fits_slide_geometry_within_capture_bounds() {
    let bounds = CaptureOptions::default();
    for (width, height, expected) in [
        (960, 540, (1280, 720)),
        (720, 540, (960, 720)),
        (540, 540, (720, 720)),
        (540, 720, (540, 720)),
        (1440, 360, (1280, 320)),
        (1000, 333, (1280, 426)),
    ] {
        for unit in ["px", "pt"] {
            let html =
                format!(":root{{--slide-design-w:{width}{unit};--slide-design-h:{height}{unit};}}");
            let options = capture_options_for_html(&html, &bounds).unwrap();
            assert_eq!((options.width, options.height), expected);
            assert_eq!(options.scale, bounds.scale);
            assert_eq!(options.timeout, bounds.timeout);
        }
    }
}

#[test]
fn supports_generic_html_but_rejects_zero_slide_dimensions() {
    let bounds = CaptureOptions::default();
    let options = capture_options_for_html("<html></html>", &bounds).unwrap();
    assert_eq!(
        (options.width, options.height),
        (bounds.width, bounds.height)
    );
    assert!(capture_options_for_html(
        ":root{--slide-design-w:0pt;--slide-design-h:540pt;}",
        &bounds
    )
    .is_err());
}
