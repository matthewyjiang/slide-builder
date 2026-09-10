use super::*;

#[test]
fn ordinary_slides_keep_300_dpi() {
    for engine in [RenderEngine::Obscura, RenderEngine::Chromium] {
        let (capture, notice) = options((5.0, 8.0), engine, 60_000).unwrap();
        assert_eq!(
            (capture.width, capture.height, capture.scale),
            (1500, 2400, 1.0)
        );
        assert_eq!(notice, None);
    }
}

#[test]
fn oversized_slide_fits_native_and_output_budgets() {
    for size in [(48.0, 36.0), (36.0, 48.0), (100.0, 100.0), (1000.0, 1.0)] {
        let (capture, notice) = options(size, RenderEngine::Obscura, 60_000).unwrap();
        capture.validate_for_engine(RenderEngine::Obscura).unwrap();
        assert_eq!(capture.scale, 1.0);
        assert!(notice
            .unwrap()
            .contains("PDF page size and slide layout are unchanged"));
        let ratio = (f64::from(capture.width) / (size.0 * 300.0))
            .max(f64::from(capture.height) / (size.1 * 300.0));
        assert!((f64::from(capture.width) - size.0 * 300.0 * ratio).abs() <= 1.0);
        assert!((f64::from(capture.height) - size.1 * 300.0 * ratio).abs() <= 1.0);
    }
}

#[test]
fn pixel_boundary_rounding_remains_safe() {
    for pixels in [4095.5, 4096.0, 4096.5, 4097.0, 16384.0] {
        let (capture, _) =
            options((pixels / 300.0, pixels / 300.0), RenderEngine::Obscura, 1).unwrap();
        capture.validate_for_engine(RenderEngine::Obscura).unwrap();
        assert!(capture.width <= 4096 && capture.height <= 4096);
    }
}

#[test]
fn chromium_keeps_its_own_budget() {
    let (capture, notice) = options((48.0, 36.0), RenderEngine::Chromium, 1).unwrap();
    assert_eq!((capture.width, capture.height), (14400, 10800));
    assert_eq!(notice, None);
    let (capture, notice) = options((100.0, 50.0), RenderEngine::Chromium, 1).unwrap();
    assert_eq!((capture.width, capture.height), (16384, 8192));
    assert!(notice.is_some());
}

#[test]
fn invalid_dimensions_and_timeouts_still_fail() {
    for size in [
        (0.0, 1.0),
        (-1.0, 1.0),
        (f64::NAN, 1.0),
        (1.0, f64::INFINITY),
    ] {
        assert!(options(size, RenderEngine::Obscura, 1).is_err());
    }
    assert!(options((48.0, 36.0), RenderEngine::Obscura, 0).is_err());
}
