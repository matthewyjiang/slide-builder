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
#[ignore = "requires a qualified native Obscura sandbox"]
async fn poster_overflow_fixture_is_repaired_with_shared_regions() {
    let directory = tempfile::tempdir().unwrap();
    let output_directory = if let Some(root) = std::env::var_os("SLIDE_BUILDER_TEST_ARTIFACTS") {
        fs::create_dir_all(&root).unwrap();
        tempfile::Builder::new()
            .prefix("poster-")
            .tempdir_in(root)
            .unwrap()
            .keep()
    } else {
        directory.path().to_owned()
    };
    let deck = DeckEngine::create(output_directory.join("poster-review.pptx"), None)
        .await
        .unwrap();
    let text = "Research question\nMethod and evidence\nFirst observation\nSecond observation\nKey limitation\nMain conclusion";
    let mut ids = vec![];
    // The first two boxes intersect, the next gutter is too wide, and six
    // explicit lines cannot fit in the authored 0.4-inch text boxes.
    for x in [0.75, 3.5, 8.0] {
        let result = deck
            .mutate(DeckMutation::Add {
                parent: "/slide[1]".into(),
                element_type: "rectangle".into(),
                properties: HashMap::from([
                    ("text".into(), text.into()),
                    ("x".into(), format!("{x}in")),
                    ("y".into(), "2in".into()),
                    ("width".into(), "3in".into()),
                    ("height".into(), "0.4in".into()),
                    ("fontSize".into(), "22".into()),
                    ("font".into(), "Arial".into()),
                    ("color".into(), "243746".into()),
                ]),
            })
            .await
            .unwrap();
        ids.push(result.affected[0].clone());
    }
    // Retain the peer-spacing relationship, then reproduce drift from later
    // independent edits. The audit should report this even before rendering.
    deck.layout_apply(json!({"operation":"distribute","ids":ids,"axis":"horizontal","gap":0.5}))
        .await
        .unwrap();
    for (id, x) in [(&ids[1], "3.5in"), (&ids[2], "8in")] {
        deck.mutate(DeckMutation::Set {
            path: id.clone(),
            properties: HashMap::from([("x".into(), x.into())]),
        })
        .await
        .unwrap();
    }
    let (width, height) = deck.slide_size().await.unwrap();
    let region_width = width - 1.5;
    let gutter = 0.5;
    let region_y = 2.0;
    let region_height = 2.5;
    let options = CaptureOptions {
        width: 1280,
        height: (1280.0 * height / width).round() as u32,
        ..Default::default()
    };
    let browser = Browser::with_embedded_worker(
        Path::new(env!("CARGO_BIN_EXE_slide-builder")),
        Path::new("auto"),
    )
    .unwrap();
    let pixels_per_inch = f64::from(options.width) / width;
    for phase in ["before", "after"] {
        if phase == "after" {
            deck.layout_set(json!({
                "margins":{"left":0.75,"right":0.75,"top":0.5,"bottom":0.5},
                "gutter":gutter,
                "regions":{"sections":{"x":0.75,"y":region_y,"width":region_width,"height":region_height}},
                "text_styles":{"body":{"font_size":22,"font_family":"Arial","color":"243746"}}
            }))
            .await
            .unwrap();
            deck.layout_apply(
                json!({"operation":"place","ids":ids,"region":"sections","axis":"horizontal"}),
            )
            .await
            .unwrap();
            deck.layout_apply(json!({"operation":"text_style","ids":ids,"style":"body"}))
                .await
                .unwrap();
        }
        let audit = deck.layout_audit().await.unwrap();
        fs::write(
            output_directory.join(format!("poster-{phase}-audit.json")),
            serde_json::to_vec_pretty(&audit).unwrap(),
        )
        .unwrap();
        let snapshot = deck.snapshot().await.unwrap();
        let html = output_directory.join(format!("poster-{phase}.html"));
        fs::write(
            &html,
            build_capture_html(&snapshot.html, 1, &options).unwrap(),
        )
        .unwrap();
        let output = output_directory.join(format!("poster-{phase}.png"));
        browser
            .capture(
                &html,
                &output,
                &output_directory.join(format!("poster-profile-{phase}")),
                &options,
            )
            .await
            .unwrap();
        let image = image::open(output).unwrap().to_rgb8();
        let ink: Vec<_> = image
            .enumerate_pixels()
            .filter_map(|(x, y, pixel)| (pixel.0 == [36, 55, 70]).then_some((x, y)))
            .collect();
        assert!(!ink.is_empty(), "poster text must render");
        if phase == "before" {
            assert_eq!(
                audit["warnings"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|warning| warning["kind"] == "possible_text_overflow")
                    .count(),
                ids.len(),
                "the audit must flag every overflowing multiline text box: {audit}"
            );
            assert!(
                audit["issues"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|issue| { issue["rule"]["operation"] == "distribute" }),
                "the audit must detect the declared peer-spacing drift: {audit}"
            );
            assert!(
                ink.iter()
                    .any(|(_, y)| f64::from(*y) > 2.4 * pixels_per_inch),
                "fixture must reproduce visible text beyond the original box bottoms"
            );
        } else {
            assert_eq!(audit["issues"], json!([]), "{audit}");
            assert_eq!(audit["warnings"], json!([]), "{audit}");
            let slot_width = (region_width - gutter * (ids.len() - 1) as f64) / ids.len() as f64;
            let mut bounds = vec![];
            for index in 0..ids.len() {
                let left = (0.75 + index as f64 * (slot_width + gutter)) * pixels_per_inch;
                let right = left + slot_width * pixels_per_inch;
                assert_eq!(
                    deck.inspect(Some(format!("/slide[1]/shape[{}]", index + 1)))
                        .await
                        .unwrap()["text"],
                    text,
                    "layout repair must preserve the section's content"
                );
                let mut ys: Vec<_> = ink
                    .iter()
                    .filter_map(|(x, y)| {
                        (f64::from(*x) >= left && f64::from(*x) < right).then_some(*y)
                    })
                    .collect();
                ys.sort_unstable();
                ys.dedup();
                let line_bands = usize::from(!ys.is_empty())
                    + ys.windows(2).filter(|pair| pair[1] > pair[0] + 1).count();
                assert_eq!(
                    line_bands,
                    text.lines().count(),
                    "all text lines must remain visible"
                );
                bounds.push((
                    *ys.iter().min().expect("each section must retain its text"),
                    *ys.iter().max().unwrap(),
                ));
            }
            assert!(bounds.iter().all(|bound| *bound == bounds[0]), "{bounds:?}");
            assert!(
                ink.iter().all(|(x, y)| {
                    let x = f64::from(*x) / pixels_per_inch;
                    let y = f64::from(*y) / pixels_per_inch;
                    y >= region_y
                        && y < region_y + region_height
                        && (0..ids.len()).any(|index| {
                            let left = 0.75 + index as f64 * (slot_width + gutter);
                            x >= left && x < left + slot_width
                        })
                }),
                "repaired text must stay inside its section and out of the gutters"
            );
        }
    }
    eprintln!("Poster review artifacts: {}", output_directory.display());
}
