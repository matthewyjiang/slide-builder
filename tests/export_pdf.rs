use slide_builder::{
    agent::deck_engine::{DeckEngine, DeckMutation},
    config::Config,
    export::export_snapshot,
    render::browser::Browser,
};
use std::{collections::HashMap, path::Path, process::Command};

#[tokio::test]
#[ignore = "requires a qualified native Obscura sandbox and Poppler"]
async fn obscura_exports_portrait_snapshot_as_full_bleed_pdf() {
    let browser = Browser::with_embedded_worker(
        Path::new(env!("CARGO_BIN_EXE_slide-builder")),
        Path::new("auto"),
    )
    .unwrap();
    exports_portrait_snapshot_as_full_bleed_pdf(browser).await;
}

#[tokio::test]
#[ignore = "requires Chromium, pdfinfo and pdftoppm"]
async fn chromium_exports_portrait_snapshot_as_full_bleed_pdf() {
    exports_portrait_snapshot_as_full_bleed_pdf(Browser::probe_chromium(None).unwrap()).await;
}

async fn exports_portrait_snapshot_as_full_bleed_pdf(browser: Browser) {
    let directory = tempfile::tempdir().unwrap();
    let engine = DeckEngine::create(directory.path().join("portrait.pptx"), None)
        .await
        .unwrap();
    engine.mutate(DeckMutation::RawSet {
        part: "ppt/presentation.xml".into(), xpath: "/presentation/sldSz".into(), action: "replace".into(),
        xml: Some("<p:sldSz xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" cx=\"4572000\" cy=\"7315200\"/>".into()),
    }).await.unwrap();
    engine
        .mutate(DeckMutation::Add {
            parent: "/".into(),
            element_type: "slide".into(),
            properties: HashMap::new(),
        })
        .await
        .unwrap();
    for (slide, color) in [(1, "FF0000"), (2, "0000FF")] {
        engine
            .mutate(DeckMutation::Add {
                parent: format!("/slide[{slide}]"),
                element_type: "rectangle".into(),
                properties: [
                    ("x", "0in"),
                    ("y", "0in"),
                    ("width", "5in"),
                    ("height", "8in"),
                    ("fill", color),
                    ("line", color),
                ]
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
            })
            .await
            .unwrap();
    }
    engine
        .mutate(DeckMutation::Add {
            parent: "/slide[1]".into(),
            element_type: "rectangle".into(),
            properties: [
                ("x", "1in"),
                ("y", "1in"),
                ("width", "1in"),
                ("height", "1in"),
                ("fill", "00FF00"),
                ("line", "00FF00"),
            ]
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect(),
        })
        .await
        .unwrap();
    let snapshot = engine.snapshot().await.unwrap();
    assert_eq!(snapshot.size_inches, (5.0, 8.0));
    // Export must keep the captured version even when the source deck changes.
    engine
        .mutate(DeckMutation::Remove {
            path: "/slide[2]".into(),
        })
        .await
        .unwrap();
    let mut config = Config::default();
    config.preview.enabled = false;
    config.preview.width = 500;
    config.preview.scale = 1;
    let output = directory.path().join("portrait.pdf");
    export_snapshot(snapshot, browser, &config, &output)
        .await
        .unwrap();
    let info = Command::new("pdfinfo").arg(&output).output().unwrap();
    assert!(
        info.status.success(),
        "{}",
        String::from_utf8_lossy(&info.stderr)
    );
    let info = String::from_utf8(info.stdout).unwrap();
    assert!(
        info.lines()
            .any(|line| line.starts_with("Pages:") && line.ends_with('2')),
        "{info}"
    );
    assert!(info.contains("360 x 576 pts"), "{info}");
    let raster = directory.path().join("page");
    let result = Command::new("pdftoppm")
        .args(["-png", "-r", "72"])
        .arg(&output)
        .arg(&raster)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for (page, expected) in [(1, [255, 0, 0]), (2, [0, 0, 255])] {
        let image = image::open(directory.path().join(format!("page-{page}.png")))
            .unwrap()
            .to_rgb8();
        assert_eq!(image.dimensions(), (360, 576));
        if page == 1 {
            // A physical one-inch square must remain square at 72 dpi.
            let green = image
                .enumerate_pixels()
                .filter(|(_, _, pixel)| pixel.0 == [0, 255, 0])
                .map(|(x, y, _)| (x, y))
                .collect::<Vec<_>>();
            let min_x = green.iter().map(|point| point.0).min().unwrap();
            let max_x = green.iter().map(|point| point.0).max().unwrap();
            let min_y = green.iter().map(|point| point.1).min().unwrap();
            let max_y = green.iter().map(|point| point.1).max().unwrap();
            // Measured on Obscura + pdftoppm: the pure-green interior is
            // 72 × 71 pixels because one edge is interpolated during resampling.
            for extent in [max_x - min_x + 1, max_y - min_y + 1] {
                assert!(
                    extent.abs_diff(72) <= 1,
                    "one-inch square spans {extent} pixels; expected 72 ± 1"
                );
            }
        }
        for point in [(1, 1), (358, 1), (1, 574), (358, 574), (180, 288)] {
            assert_eq!(
                image.get_pixel(point.0, point.1).0,
                expected,
                "page {page} at {point:?}"
            );
        }
    }
}
