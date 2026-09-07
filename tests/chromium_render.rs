use slide_builder::{
    config::{RenderConfig, RenderEngine},
    render::browser::{Browser, CaptureOptions},
};

#[tokio::test]
#[ignore = "requires an installed Chromium-family browser"]
async fn chromium_capture_preserves_viewport_at_multiple_scales() {
    let config = RenderConfig {
        engine: RenderEngine::Chromium,
        ..Default::default()
    };
    let browser = Browser::probe(&config).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let html = directory.path().join("capture with spaces.html");
    std::fs::write(&html, "<html><body style='margin:0;background:white'><div style='position:absolute;left:25%;top:40px;width:80px;height:60px;background:#ff0000'></div></body></html>").unwrap();
    for scale in [1.0, 2.0] {
        let output = directory.path().join(format!("scale-{scale}.png"));
        let options = CaptureOptions {
            width: 640,
            height: 360,
            scale,
            ..Default::default()
        };
        browser
            .capture(
                &html,
                &output,
                &directory.path().join(format!("profile-{scale}")),
                &options,
            )
            .await
            .unwrap();
        let image = image::open(output).unwrap().to_rgb8();
        let scaled = |value: u32| (value as f32 * scale) as u32;
        assert_eq!(image.dimensions(), (scaled(640), scaled(360)));
        for (x, y, pixel) in image.enumerate_pixels() {
            let inside =
                (scaled(160)..scaled(240)).contains(&x) && (scaled(40)..scaled(100)).contains(&y);
            assert_eq!(
                pixel.0,
                if inside { [255, 0, 0] } else { [255, 255, 255] },
                "scale {scale} at {x},{y}"
            );
        }
    }
}
