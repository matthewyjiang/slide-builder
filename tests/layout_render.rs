use serde_json::json;
use slide_builder::{
    agent::deck_engine::{DeckEngine, DeckMutation},
    render::{
        browser::{Browser, CaptureOptions},
        pipeline::build_capture_html,
    },
};
use std::{collections::HashMap, fs, path::Path};

#[tokio::test]
#[ignore = "requires Linux user namespaces and bubblewrap"]
async fn shared_layout_and_type_styles_reach_rendered_slides() {
    let directory = tempfile::tempdir().unwrap();
    // Opt-in artifact retention for visual review; normal test runs leave no files.
    let output_directory = std::env::var_os("SLIDE_BUILDER_TEST_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.path().to_owned());
    fs::create_dir_all(&output_directory).unwrap();
    let deck = DeckEngine::create(output_directory.join("layout-review.pptx"), None)
        .await
        .unwrap();
    let mut ids = vec![];
    for (text, x, y, width, height, size) in [
        (
            "Evidence should line up",
            "0.9in",
            "0.5in",
            "11in",
            "0.8in",
            "24",
        ),
        ("First result", "1in", "2in", "2in", "0.5in", "24"),
        ("Second result", "4.2in", "2.15in", "3in", "0.6in", "20"),
        ("Third result", "8in", "1.9in", "2.5in", "0.7in", "26"),
    ] {
        let result = deck
            .mutate(DeckMutation::Add {
                parent: "/slide[1]".into(),
                element_type: "rectangle".into(),
                properties: HashMap::from([
                    ("text", text),
                    ("x", x),
                    ("y", y),
                    ("width", width),
                    ("height", height),
                    ("fontSize", size),
                    ("font", "Arial"),
                    ("color", "243746"),
                ])
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
            })
            .await
            .unwrap();
        ids.push(result.affected[0].clone());
    }
    let options = CaptureOptions {
        width: 1280,
        height: 720,
        ..Default::default()
    };
    let browser = Browser::with_embedded_worker(
        Path::new(env!("CARGO_BIN_EXE_slide-builder")),
        Path::new("auto"),
    )
    .unwrap();
    for phase in ["before", "after"] {
        if phase == "after" {
            deck.layout_set(json!({
                "margins":{"left":0.75,"right":0.75,"top":0.5,"bottom":0.5},
                "gutter":0.5,
                "regions":{"evidence":{"x":0.75,"y":2,"width":11,"height":1}},
                "text_styles":{
                    "headline":{"font_size":40,"font_family":"Arial","color":"142837","bold":true},
                    "body":{"font_size":24,"font_family":"Arial","color":"243746"}
                }
            }))
            .await
            .unwrap();
            for operation in [
                json!({"operation":"place","ids":ids[1..],"region":"evidence","axis":"horizontal"}),
                json!({"operation":"align","ids":[ids[0]],"reference":ids[1],"edge":"left"}),
                json!({"operation":"text_style","ids":[ids[0]],"style":"headline"}),
                json!({"operation":"text_style","ids":ids[1..],"style":"body"}),
            ] {
                deck.layout_apply(operation).await.unwrap();
            }
        }
        let snapshot = deck.snapshot().await.unwrap();
        let html = output_directory.join(format!("layout-{phase}.html"));
        fs::write(
            &html,
            build_capture_html(&snapshot.html, 1, &options).unwrap(),
        )
        .unwrap();
        let output = output_directory.join(format!("layout-{phase}.png"));
        browser
            .capture(
                &html,
                &output,
                &output_directory.join(format!("profile-{phase}")),
                &options,
            )
            .await
            .unwrap();
        let image = image::open(&output).unwrap().to_rgb8();
        assert_eq!(image.dimensions(), (options.width, options.height));
        if phase == "after" {
            let pixels_per_inch = f64::from(options.width) / deck.slide_size().await.unwrap().0;
            let labels = ids.len() - 1;
            let slot_width = (11.0 - 0.5 * (labels - 1) as f64) / labels as f64;
            // Measure the actual text ink in each authored slot, not just the
            // transform rectangles. The labels share size and vertical alignment.
            let rows: Vec<_> = (0..labels)
                .map(|index| {
                    let left = (0.75 + index as f64 * (slot_width + 0.5)) * pixels_per_inch;
                    let right = left + slot_width * pixels_per_inch;
                    let ys: Vec<_> = image
                        .enumerate_pixels()
                        .filter_map(|(x, y, pixel)| {
                            (f64::from(x) >= left
                                && f64::from(x) < right
                                && f64::from(y) >= 2.0 * pixels_per_inch
                                && f64::from(y) < 3.0 * pixels_per_inch
                                && pixel.0 == [36, 55, 70])
                            .then_some(y)
                        })
                        .collect();
                    (
                        *ys.iter().min().expect("body text must render"),
                        *ys.iter().max().unwrap(),
                    )
                })
                .collect();
            assert!(
                rows.iter().all(|row| *row == rows[0]),
                "label ink bounds differ: {rows:?}"
            );
            let headline_rows: Vec<_> = image
                .enumerate_pixels()
                .filter_map(|(_, y, pixel)| (pixel.0 == [20, 40, 55]).then_some(y))
                .collect();
            let headline_height = headline_rows.iter().max().expect("headline must render")
                - headline_rows.iter().min().unwrap();
            assert!(
                headline_height > rows[0].1 - rows[0].0,
                "headline must lead the supporting text"
            );
        }
    }
    assert_eq!(deck.layout_audit().await.unwrap()["issues"], json!([]));
    eprintln!("Layout review artifacts: {}", output_directory.display());
}
