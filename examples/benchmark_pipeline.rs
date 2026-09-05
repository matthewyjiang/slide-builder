//! Benchmark actual isolated Obscura and Chromium pipelines on the same deck.
//! Usage: benchmark_pipeline DECK OUTPUT OBSCURA CHROMIUM [WIDTH]
//! Each sample uses a fresh cache. Renderer discovery and handler export are
//! outside capture timing, as they are not repeated for every app refresh.
use anyhow::{bail, Context, Result};
use slide_builder::{
    agent::deck_engine::DeckEngine,
    config::{RenderConfig, RenderEngine},
    render::{
        browser::{Browser, CaptureOptions},
        cache::{CacheKey, RenderCache},
        pipeline::{handler_slide_count, BrowserPipeline, HANDLER_REVISION, RENDERER_VERSION},
    },
};
use std::{path::PathBuf, time::Instant};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let deck = PathBuf::from(
        args.next()
            .context("expected DECK OUTPUT OBSCURA CHROMIUM [WIDTH]")?,
    );
    let output = PathBuf::from(args.next().context("expected OUTPUT")?);
    let obscura = PathBuf::from(args.next().context("expected OBSCURA executable")?);
    let chromium = PathBuf::from(args.next().context("expected CHROMIUM executable")?);
    let width = args
        .next()
        .map(|value| value.to_string_lossy().parse::<u32>())
        .transpose()?
        .unwrap_or(1280);
    if output.exists() {
        bail!("output already exists: {}", output.display());
    }
    std::fs::create_dir_all(&output)?;
    let output = output.canonicalize()?;
    let export_start = Instant::now();
    let snapshot = DeckEngine::new(&deck)?.snapshot().await?;
    let export_seconds = export_start.elapsed().as_secs_f64();
    let count = handler_slide_count(&snapshot.html);
    let bytes = std::fs::read(&deck)?;
    let mut engines = Vec::new();
    for engine in [RenderEngine::Chromium, RenderEngine::Obscura] {
        let config = RenderConfig {
            engine,
            obscura_path: obscura.clone(),
            browser_path: chromium.clone(),
            ..RenderConfig::default()
        };
        let start = Instant::now();
        let browser = Browser::probe(&config)?;
        engines.push((engine, browser, start.elapsed().as_secs_f64()));
    }
    let options = CaptureOptions {
        width,
        height: width.saturating_mul(9) / 16,
        ..CaptureOptions::default()
    };
    let mut records = Vec::new();
    // One warmup plus five paired samples, matching the exploratory CLI study.
    for sample in 0..=5 {
        let order = if sample % 2 == 0 { [0, 1] } else { [1, 0] };
        for index in order {
            let (engine, browser, probe_seconds) = &engines[index];
            let name = match engine {
                RenderEngine::Obscura => "obscura",
                RenderEngine::Chromium => "chromium",
            };
            let cache = RenderCache::new(output.join(format!("{name}-{sample}")), 1)?;
            let pipeline = BrowserPipeline::new(browser.clone(), cache, options.clone(), 4)?;
            let key = CacheKey::new(
                &bytes,
                HANDLER_REVISION,
                RENDERER_VERSION,
                options.width,
                options.height,
                options.scale,
            )?;
            let start = Instant::now();
            let (directory, manifest) = pipeline
                .render(
                    sample,
                    b"benchmark-deck",
                    key.clone(),
                    &snapshot.html,
                    count,
                )
                .await?;
            let seconds = start.elapsed().as_secs_f64();
            // A subsequent generation must reuse this engine's entry while
            // returning the new correlation generation, not recapture it.
            let hit_start = Instant::now();
            let (hit_directory, hit) = pipeline
                .render(sample + 1, b"benchmark-deck", key, &snapshot.html, count)
                .await?;
            let hit_seconds = hit_start.elapsed().as_secs_f64();
            assert_eq!(directory, hit_directory);
            assert_eq!(hit.slides, manifest.slides);
            assert_eq!(hit.generation, sample + 1);
            records.push(serde_json::json!({
                "engine": name, "sample": sample, "warmup": sample == 0,
                "seconds": seconds, "cache_hit_seconds": hit_seconds,
                "probe_seconds": probe_seconds, "directory": directory,
                "renderer_identity": browser.cache_identity(), "slides": manifest.slides,
            }));
            std::fs::write(
                output.join("results.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "width": options.width, "height": options.height, "slide_count": count,
                    "export_seconds": export_seconds, "concurrency": 4, "records": records,
                }))?,
            )?;
            println!("{name} sample {sample}: {seconds:.3}s; cache hit {hit_seconds:.3}s");
        }
    }
    Ok(())
}
