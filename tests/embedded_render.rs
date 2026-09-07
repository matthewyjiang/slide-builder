use slide_builder::{
    agent::deck_engine::{DeckEngine, BLANK_DECK},
    render::{
        browser::{Browser, CaptureOptions},
        pipeline::build_capture_html,
    },
};
use std::{fs, path::Path, process::Command};

#[tokio::test]
#[ignore = "requires a qualified native Obscura sandbox"]
async fn embedded_scale_preserves_layout_and_paints_crisp_shapes() {
    let browser = Browser::with_embedded_worker(
        Path::new(env!("CARGO_BIN_EXE_slide-builder")),
        Path::new("auto"),
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let html = directory.path().join("capture.html");
    fs::write(&html, "<html><body style='margin:0;background:white'><div style='position:absolute;left:25%;top:40px;width:80px;height:60px;background:#ff0000'></div></body></html>").unwrap();
    for scale in [0.25, 1.0, 2.0, 4.0] {
        let options = CaptureOptions {
            width: 640,
            height: 360,
            scale,
            ..Default::default()
        };
        let output = directory.path().join(format!("scale-{scale}.png"));
        browser
            .capture(&html, &output, &directory.path().join("profile"), &options)
            .await
            .unwrap();
        let image = image::open(output).unwrap().to_rgb8();
        let scaled = |value: u32| (value as f32 * scale) as u32;
        assert_eq!(image.dimensions(), (scaled(640), scaled(360)));
        // Every pixel must match the CSS box, including its sharp boundaries.
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

#[test]
fn direct_worker_invocation_fails_before_loading_configuration() {
    let output = Command::new(env!("CARGO_BIN_EXE_slide-builder"))
        .env_clear()
        .args(["--slide-builder-render-worker", "640", "360", "1", "1000"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("must be launched through the preview sandbox"));
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    assert!(String::from_utf8_lossy(&output.stderr).contains("set render.engine = \"chromium\""));
}

#[tokio::test]
#[ignore = "requires a qualified native Obscura sandbox"]
async fn embedded_worker_captures_handler_html_and_blocks_host_styles() {
    let browser = Browser::with_embedded_worker(
        Path::new(env!("CARGO_BIN_EXE_slide-builder")),
        Path::new("auto"),
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let deck = directory.path().join("fixture.pptx");
    fs::write(&deck, BLANK_DECK).unwrap();
    let snapshot = DeckEngine::new(&deck).unwrap().snapshot().await.unwrap();
    let options = CaptureOptions {
        width: 640,
        height: 360,
        scale: 2.0,
        ..Default::default()
    };
    let sentinel = directory.path().join("sentinel.css");
    fs::write(&sentinel, "html,body,.slide{background:#00ff00!important}").unwrap();
    // Inject after pipeline validation to test the OS boundary independently
    // of the HTML sanitizer, which already rejects explicit file URLs.
    let source = build_capture_html(&snapshot.html, 1, &options).unwrap().replace("</body>", &format!(
        "<link rel=\"stylesheet\" href=\"file://{}\"><link rel=\"stylesheet\" href=\"sentinel.css\"></body>", sentinel.display()
    ));
    let html = directory.path().join("capture.html");
    fs::write(&html, source).unwrap();
    let output = directory.path().join("capture.png");
    browser
        .capture(&html, &output, &directory.path().join("profile"), &options)
        .await
        .unwrap();
    let image = image::open(&output).unwrap().to_rgb8();
    assert_eq!(image.dimensions(), (1280, 720));
    assert!(!image.pixels().any(|pixel| pixel.0 == [0, 255, 0]));

    // Prove that authored CSS is painted, rather than accepting a blank PNG.
    fs::remove_file(&output).unwrap();
    fs::write(
        &html,
        "<html><body style='margin:0;background:#ff0000'></body></html>",
    )
    .unwrap();
    browser
        .capture(&html, &output, &directory.path().join("profile"), &options)
        .await
        .unwrap();
    let image = image::open(output).unwrap().to_rgb8();
    assert_eq!(image.get_pixel(320, 180).0, [255, 0, 0]);
}
