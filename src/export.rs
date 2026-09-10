//! Raster PDF export from an immutable handler snapshot, independent of preview state.
use crate::{
    agent::deck_engine::{DeckEngine, DeckSnapshot},
    config::Config,
    render::{
        browser::Browser,
        cache::{CacheKey, RenderCache},
        pipeline::{handler_slide_count, BrowserPipeline, HANDLER_REVISION, RENDERER_VERSION},
    },
};
use anyhow::{bail, Context, Result};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

mod capture;
mod pdf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportReport {
    pub path: PathBuf,
    pub resolution_notice: Option<String>,
}

pub const USAGE: &str = "Usage: /export pdf [destination.pdf]. Defaults beside the active PowerPoint file. Existing files are never overwritten.";

/// The destination is the entire remainder, so paths containing spaces need no quoting.
pub fn parse_arguments(arguments: &str) -> Result<Option<PathBuf>> {
    let arguments = arguments.trim();
    let (format, destination) = arguments
        .split_once(char::is_whitespace)
        .unwrap_or((arguments, ""));
    if format.is_empty() {
        bail!("{USAGE}");
    }
    if !format.eq_ignore_ascii_case("pdf") {
        bail!("Unsupported export format '{format}'. {USAGE}");
    }
    let destination = destination.trim();
    if destination.is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(destination);
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
    {
        bail!("The export destination must end in .pdf. {USAGE}");
    }
    Ok(Some(path))
}

pub fn destination(deck: &Path, requested: Option<PathBuf>) -> PathBuf {
    requested.unwrap_or_else(|| deck.with_extension("pdf"))
}

/// Capture one committed deck version. Browser probing, rendering and PDF assembly
/// run outside the UI task; the temporary render cache belongs only to this export.
pub async fn export_pdf(
    engine: DeckEngine,
    config: Config,
    destination: PathBuf,
) -> Result<ExportReport> {
    let snapshot = engine
        .snapshot()
        .await
        .context("read current deck for PDF export")?;
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        check_destination(&destination)?;
        let browser = Browser::probe(&config.render).context("PDF export renderer unavailable")?;
        let resolution_notice =
            runtime.block_on(export_snapshot(snapshot, browser, &config, &destination))?;
        Ok(ExportReport {
            path: destination,
            resolution_notice,
        })
    })
    .await
    .context("PDF export task failed")?
}

fn check_destination(destination: &Path) -> Result<()> {
    match std::fs::symlink_metadata(destination) {
        Ok(_) => bail!("{} already exists. Choose another destination with /export pdf destination.pdf, or move the existing file first.", destination.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("inspect export destination {}", destination.display())),
    }
}

/// Export an already captured snapshot through the same PNG pipeline as previews.
/// Callers must run this away from the UI task because image/PDF encoding is blocking.
/// Returns a resolution notice when the 300 dpi target had to be reduced.
pub async fn export_snapshot(
    snapshot: DeckSnapshot,
    browser: Browser,
    config: &Config,
    destination: &Path,
) -> Result<Option<String>> {
    check_destination(destination)?;
    let (design_width, design_height) = snapshot.size_inches;
    if !design_width.is_finite()
        || !design_height.is_finite()
        || design_width <= 0.0
        || design_height <= 0.0
    {
        bail!("invalid slide dimensions {design_width} × {design_height}");
    }
    let (options, resolution_notice) = capture::options(
        snapshot.size_inches,
        browser.engine(),
        config.render.timeout_ms,
    )?;
    let key = CacheKey::new(
        snapshot.html.as_bytes(),
        HANDLER_REVISION,
        RENDERER_VERSION,
        options.width,
        options.height,
        options.scale,
    )?;
    let temporary = tempfile::tempdir().context("create PDF render workspace")?;
    let cache = RenderCache::new(
        temporary.path().to_path_buf(),
        config.render.keep_generations,
    )?;
    // Render sequentially to avoid simultaneous browser capture buffers.
    let pipeline = BrowserPipeline::new(browser, cache, options, /*max_concurrency*/ 1)?;
    let (directory, manifest) = pipeline
        .render(
            snapshot.generation,
            b"pdf-export",
            key,
            &snapshot.html,
            handler_slide_count(&snapshot.html),
        )
        .await?;
    let images = manifest
        .slides
        .iter()
        .map(|slide| directory.join(&slide.file))
        .collect::<Vec<_>>();
    // PDF user units are 1/72 inch.
    let bytes = pdf::assemble(
        &images,
        (design_width * 72.0) as f32,
        (design_height * 72.0) as f32,
    )?;
    publish(destination, &bytes)?;
    Ok(resolution_notice)
}

fn publish(destination: &Path, bytes: &[u8]) -> Result<()> {
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut output = tempfile::NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "create PDF in {}; choose an existing writable directory",
            parent.display()
        )
    })?;
    output.write_all(bytes).context("write PDF")?;
    output.as_file().sync_all().context("flush PDF")?;
    output.persist_noclobber(destination).map_err(|error| error.error)
        .with_context(|| format!("publish PDF to {} without overwriting; choose another destination if it already exists", destination.display()))?;
    Ok(())
}

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
