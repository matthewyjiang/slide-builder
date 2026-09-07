use slide_builder::{
    agent::deck_engine::{DeckEngine, DeckMutation},
    render::{
        browser::{Browser, CaptureOptions},
        pipeline::build_capture_html,
    },
};
use std::{collections::HashMap, fs, path::Path};

#[tokio::test]
#[ignore = "requires a qualified native Obscura sandbox"]
async fn light_text_on_dark_background_reaches_rendered_pixels() {
    let directory = tempfile::tempdir().unwrap();
    let deck = DeckEngine::create(directory.path().join("contrast.pptx"), None)
        .await
        .unwrap();
    for properties in [
        HashMap::from([
            ("x", "0in"),
            ("y", "0in"),
            ("width", "13.333in"),
            ("height", "7.5in"),
            ("fill", "0B1820"),
        ]),
        HashMap::from([
            ("x", "1in"),
            ("y", "1in"),
            ("width", "10in"),
            ("height", "2in"),
            ("text", "Your terminal forgets."),
            ("color", "FFFFFF"),
            ("fontSize", "44"),
            ("bold", "true"),
        ]),
    ] {
        deck.mutate(DeckMutation::Add {
            parent: "/slide[1]".into(),
            element_type: "rectangle".into(),
            properties: properties
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        })
        .await
        .unwrap();
    }
    let snapshot = deck.snapshot().await.unwrap();
    let options = CaptureOptions {
        width: 1280,
        height: 720,
        ..Default::default()
    };
    let html = directory.path().join("capture.html");
    fs::write(
        &html,
        build_capture_html(&snapshot.html, 1, &options).unwrap(),
    )
    .unwrap();
    let output = directory.path().join("contrast.png");
    Browser::with_embedded_worker(
        Path::new(env!("CARGO_BIN_EXE_slide-builder")),
        Path::new("auto"),
    )
    .unwrap()
    .capture(&html, &output, &directory.path().join("profile"), &options)
    .await
    .unwrap();
    let image = image::open(output).unwrap().to_rgb8();
    // At 1280x720, the authored text rectangle is x=96..1056, y=96..288.
    // Check inside it, excluding slide edges, so a white canvas cannot pass.
    assert_eq!(image.get_pixel(64, 64).0, [11, 24, 32]);
    assert!(
        image
            .enumerate_pixels()
            .any(|(x, y, pixel)| (96..1056).contains(&x)
                && (96..288).contains(&y)
                && pixel.0 == [255, 255, 255]),
        "the rendered title contains no white text pixels"
    );
}
