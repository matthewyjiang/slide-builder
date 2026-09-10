use slide_builder::{
    agent::deck_engine::{DeckEngine, DeckMutation, BLANK_DECK},
    render::{
        browser::{Browser, CaptureOptions},
        cache::{CacheKey, RenderCache},
        pipeline::{BrowserPipeline, HANDLER_REVISION, RENDERER_VERSION},
    },
};
use std::{
    fs,
    io::{Cursor, Read, Write},
    path::Path,
};

#[tokio::test]
#[ignore = "requires a qualified native Obscura sandbox"]
async fn rendered_decks_preserve_authored_aspect_ratio() {
    let browser = Browser::with_embedded_worker(
        Path::new(env!("CARGO_BIN_EXE_slide-builder")),
        Path::new("auto"),
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let options = CaptureOptions {
        width: 640,
        height: 360,
        ..Default::default()
    };
    let pipeline = BrowserPipeline::new(
        browser,
        RenderCache::new(directory.path().join("cache"), 1).unwrap(),
        options.clone(),
        /*max_concurrency*/ 1,
    )
    .unwrap();
    for (width, height, expected) in [
        (12, 9, (480, 360)),
        (9, 9, (360, 360)),
        (9, 12, (270, 360)),
        (16, 9, (640, 360)),
        (20, 5, (640, 160)),
    ] {
        let mut input = zip::ZipArchive::new(Cursor::new(BLANK_DECK)).unwrap();
        let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..input.len() {
            let mut entry = input.by_index(index).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if entry.name() == "ppt/presentation.xml" {
                let mut xml = String::from_utf8(bytes).unwrap();
                let start = xml.find("<p:sldSz ").unwrap();
                let end = start + xml[start..].find("/>").unwrap() + 2;
                xml.replace_range(
                    start..end,
                    &format!(
                        "<p:sldSz cx=\"{}\" cy=\"{}\"/>",
                        width * 914400,
                        height * 914400
                    ),
                );
                bytes = xml.into_bytes();
            }
            output
                .start_file(entry.name(), zip::write::SimpleFileOptions::default())
                .unwrap();
            output.write_all(&bytes).unwrap();
        }
        let bytes = output.finish().unwrap().into_inner();
        let path = directory.path().join(format!("{width}-{height}.pptx"));
        fs::write(&path, &bytes).unwrap();
        let deck = DeckEngine::new(&path).unwrap();
        deck.mutate(DeckMutation::Add {
            parent: "/slide[1]".into(),
            element_type: "rectangle".into(),
            properties: [
                ("x", "0in".into()),
                ("y", "0in".into()),
                ("width", format!("{width}in")),
                ("height", format!("{height}in")),
                ("fill", "0B1820".into()),
            ]
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
        })
        .await
        .unwrap();
        let snapshot = deck.snapshot().await.unwrap();
        let bytes = fs::read(&path).unwrap();
        let key = CacheKey::new(
            &bytes,
            HANDLER_REVISION,
            RENDERER_VERSION,
            options.width,
            options.height,
            options.scale,
        )
        .unwrap();
        let (images, manifest) = pipeline
            .render(1, b"aspect-ratio", key.clone(), &snapshot.html, 1)
            .await
            .unwrap();
        let slide = &manifest.slides[0];
        let image = image::open(images.join(&slide.file)).unwrap().to_rgb8();
        assert_eq!(image.dimensions(), expected, "{width}:{height} slide");
        // The full-bleed authored shape must reach every edge, with no white bars.
        for (x, y) in [
            (1, 1),
            (expected.0 - 2, 1),
            (1, expected.1 - 2),
            (expected.0 - 2, expected.1 - 2),
        ] {
            assert_eq!(image.get_pixel(x, y).0, [11, 24, 32]);
        }
        assert_eq!((slide.width, slide.height), expected);
        let (_, cached) = pipeline
            .render(2, b"aspect-ratio", key, &snapshot.html, 1)
            .await
            .unwrap();
        assert_eq!(cached.slides, manifest.slides);
        assert_eq!(cached.generation, 2);
    }
}
