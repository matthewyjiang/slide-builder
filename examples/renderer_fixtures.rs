//! Export real handler capture HTML for an external renderer comparison.
//! Usage: cargo run --example renderer_fixtures -- OUTPUT [--slides N | DECK.pptx ...]
use anyhow::{bail, Context, Result};
use slide_builder::{
    agent::deck_engine::{DeckEngine, DeckMutation},
    render::{
        browser::CaptureOptions, pipeline::build_capture_html, pipeline::handler_slide_count,
    },
};
use std::{collections::HashMap, path::PathBuf};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1).peekable();
    let output = PathBuf::from(
        args.next()
            .context("usage: renderer_fixtures OUTPUT [--slides N | DECK.pptx ...]")?,
    );
    let slide_count = if args.peek().is_some_and(|arg| arg == "--slides") {
        args.next();
        args.next()
            .context("--slides needs a count")?
            .to_string_lossy()
            .parse::<u32>()?
    } else {
        1
    };
    if slide_count == 0 {
        bail!("generated slide count must be positive; requested {slide_count}");
    }
    let mut decks: Vec<PathBuf> = args.map(PathBuf::from).collect();
    if slide_count != 1 && !decks.is_empty() {
        bail!("--slides selects generated fixtures; do not also supply deck paths");
    }
    if output.exists() {
        bail!(
            "refusing to overwrite fixture directory {}",
            output.display()
        );
    }
    std::fs::create_dir_all(&output)?;
    let output = output.canonicalize()?;
    if decks.is_empty() {
        let deck = output.join("generated.pptx");
        let engine = DeckEngine::create(&deck, /*template*/ None).await?;
        let mut mutations = Vec::new();
        for (kind, x, y, width, height, fill, text) in [
            ("rectangle", "0in", "0in", "10in", "7.5in", "173B50", ""),
            ("rectangle", "0.5in", "0.5in", "9in", "1in", "F2B544", "Renderer comparison: typography and geometry"),
            ("rectangle", "0.5in", "2in", "4in", "3in", "F0F0F0", "A deliberately narrow paragraph tests line wrapping and font metrics. Robotics, perception, planning. 0123456789"),
            ("ellipse", "5in", "2in", "3in", "2in", "48B5A0", "Ellipse"),
            ("rectangle", "5in", "4.5in", "4in", "1in", "E96A59", "Shapes should not move or disappear"),
        ] {
            mutations.push(DeckMutation::Add {
                parent: "/slide[1]".into(),
                element_type: kind.into(),
                properties: HashMap::from([
                    ("x".into(), x.into()), ("y".into(), y.into()),
                    ("width".into(), width.into()), ("height".into(), height.into()),
                    ("fill".into(), fill.into()), ("text".into(), text.into()),
                ]),
            });
        }
        let raster = output.join("raster.png");
        image::RgbImage::from_fn(256, 128, |x, y| image::Rgb([x as u8, (y * 2) as u8, 120]))
            .save(&raster)?;
        mutations.push(DeckMutation::Add {
            parent: "/slide[1]".into(),
            element_type: "image".into(),
            properties: HashMap::from([
                ("path".into(), raster.display().to_string()),
                ("x".into(), "0.5in".into()),
                ("y".into(), "5.5in".into()),
                ("width".into(), "3in".into()),
                ("height".into(), "1.5in".into()),
            ]),
        });
        for index in 1..=slide_count {
            if index > 1 {
                engine
                    .mutate(DeckMutation::Add {
                        parent: "/".into(),
                        element_type: "slide".into(),
                        properties: HashMap::new(),
                    })
                    .await?;
            }
            let mut slide_mutations = mutations.clone();
            for mutation in &mut slide_mutations {
                if let DeckMutation::Add {
                    parent, properties, ..
                } = mutation
                {
                    *parent = format!("/slide[{index}]");
                    if let Some(text) = properties.get_mut("text") {
                        *text = text.replace("Renderer comparison:", &format!("Slide {index}:"));
                    }
                }
            }
            engine.mutate_many(slide_mutations).await?;
        }
        decks.push(deck);
        decks.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/blank.pptx"));
    }
    let options = CaptureOptions::default();
    let mut manifest = Vec::new();
    for (deck_index, deck) in decks.iter().enumerate() {
        let snapshot = DeckEngine::new(deck)?.snapshot().await?;
        std::fs::write(
            output.join(format!("deck-{deck_index}.source.html")),
            &snapshot.html,
        )?;
        for slide in 1..=handler_slide_count(&snapshot.html) {
            let name = format!("deck-{deck_index}-slide-{slide}.html");
            std::fs::write(
                output.join(&name),
                build_capture_html(&snapshot.html, slide, &options)?,
            )?;
            manifest.push(serde_json::json!({"html": name, "deck": deck, "slide": slide}));
        }
    }
    // Synthetic probes supplement the handler fixtures; they are not real decks.
    for (name, content) in [
        (
            "typography",
            r#"<div style="padding:60px;font:28px 'DejaVu Sans',sans-serif;color:#173b50"><h1>Perception and planning</h1><p style="width:450px;line-height:1.35">A narrow text column should wrap identically. AV fi fl 0123456789. Robotics requires precise geometry, not merely a plausible image.</p><p style="font-family:serif;font-style:italic">Serif italic and <b>bold text</b></p><p style="font-family:monospace">x = 1.234; robot.move();</p></div>"#,
        ),
        (
            "svg",
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="900" height="500" viewBox="0 0 900 500"><defs><linearGradient id="g"><stop stop-color="#173b50"/><stop offset="1" stop-color="#48b5a0"/></linearGradient><clipPath id="clip"><circle cx="620" cy="250" r="160"/></clipPath></defs><rect x="40" y="40" width="400" height="350" rx="30" fill="url(#g)"/><path d="M80 340 Q230 20 400 280" fill="none" stroke="#f2b544" stroke-width="15"/><g clip-path="url(#clip)"><rect x="450" y="40" width="400" height="400" fill="#e96a59"/><rect x="500" y="150" width="400" height="180" fill="#173b50"/></g><text x="70" y="450" font-size="32">SVG gradient, path, clip, text</text></svg>"##,
        ),
        (
            "css-effects",
            r##"<div style="position:absolute;left:90px;top:80px;width:400px;height:250px;background:linear-gradient(135deg,#173b50,#48b5a0);border-radius:36px;box-shadow:20px 20px 15px #0008;transform:rotate(12deg)"></div><div style="position:absolute;left:420px;top:200px;width:360px;height:220px;background:#e96a59;opacity:.65;border:12px solid #f2b544;clip-path:polygon(0 0,100% 0,75% 100%,25% 100%)"></div><div style="position:absolute;left:80px;top:500px;font:32px sans-serif">Rotation, clipping, alpha, shadow</div>"##,
        ),
        (
            "layout",
            r##"<div style="display:grid;grid-template-columns:repeat(3,280px);gap:30px;padding:50px;font:28px sans-serif"><div style="background:#173b50;color:white;padding:20px">Grid one</div><div style="background:#48b5a0;padding:20px">Grid two</div><div style="background:#f2b544;padding:20px">Grid three</div></div><div style="display:flex;justify-content:space-between;margin:50px;background:#eee;height:140px;align-items:center;font:32px sans-serif"><span>Left</span><span>Center</span><span>Right</span></div>"##,
        ),
    ] {
        let source = format!(
            r#"<!doctype html><html><head><style>:root{{--slide-design-w:960pt;--slide-design-h:540pt}}.slide{{position:relative;width:1280px;height:720px;background:white}}</style></head><body><div class="main"><div class="slide-container" data-slide="1"><div class="slide-wrapper"><div class="slide" style="background:red">WRONG SLIDE</div></div></div><div class="slide-container" data-slide="2"><div class="slide-wrapper"><div class="slide">{content}</div></div></div></div></body></html>"#
        );
        let name = format!("probe-{name}.html");
        std::fs::write(
            output.join(&name),
            build_capture_html(&source, 2, &options)?,
        )?;
        manifest.push(serde_json::json!({"html": name, "synthetic": true, "slide": 2}));
    }
    std::fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    // Kept out of the performance manifest. The current CSP forbids this local
    // external stylesheet even though the offline HTML validator accepts it.
    std::fs::create_dir(output.join("security"))?;
    std::fs::write(
        output.join("sentinel.css"),
        ".slide{background:#00ff00!important}",
    )?;
    let security_source = r#"<!doctype html><html><head><style>:root{--slide-design-w:960pt;--slide-design-h:540pt}.slide{position:relative;width:1280px;height:720px;background:white}</style></head><body><div class="main"><link rel="stylesheet" href="../sentinel.css"><div class="slide-container" data-slide="1"><div class="slide-wrapper"><div class="slide">External stylesheet must be blocked</div></div></div></div></body></html>"#;
    std::fs::write(
        output.join("security/capture.html"),
        build_capture_html(security_source, 1, &options)?,
    )?;
    println!(
        "Exported {} captures to {}",
        manifest.len(),
        output.display()
    );
    Ok(())
}
