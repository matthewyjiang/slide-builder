//! Export resolution policy: fit native surfaces, not just scaled output buffers.
use crate::{config::RenderEngine, render::browser::CaptureOptions};
use anyhow::{bail, Result};
use std::time::Duration;

pub(super) fn options(
    size_inches: (f64, f64),
    engine: RenderEngine,
    timeout_ms: u64,
) -> Result<(CaptureOptions, Option<String>)> {
    const DPI: f64 = 300.0;
    let (width, height) = ((size_inches.0 * DPI).round(), (size_inches.1 * DPI).round());
    if !width.is_finite() || !height.is_finite() || width < 1.0 || height < 1.0 {
        bail!("export capture dimensions are out of range at {DPI} dpi: {width} × {height}");
    }
    let max_dimension = match engine {
        RenderEngine::Chromium => f64::from(CaptureOptions::MAX_DIMENSION),
        RenderEngine::Obscura => f64::from(CaptureOptions::MAX_DIMENSION)
            .min(f64::from(obscura_js::MAX_CAPTURE_DIMENSION)),
    };
    let mut ratio = 1.0_f64
        .min(max_dimension / width)
        .min(max_dimension / height);
    if engine == RenderEngine::Obscura {
        // Divide sequentially to avoid overflowing width * height.
        ratio = ratio.min((obscura_js::MAX_CAPTURE_PIXELS as f64 / width / height).sqrt());
    }
    // Keep ordinary 300 dpi rounding unchanged. Floor reduced captures so
    // pixel rounding cannot push either native surface back over its budget.
    let dimension = |value: f64| (value * ratio).floor().max(1.0) as u32;
    let options = CaptureOptions {
        width: dimension(width),
        height: dimension(height),
        scale: 1.0,
        timeout: Duration::from_millis(timeout_ms),
    };
    options.validate_for_engine(engine)?;
    let reduced = f64::from(options.width) < width || f64::from(options.height) < height;
    let notice = reduced.then(|| format!(
        "Export resolution reduced from the 300 dpi target to {} × {} pixels per slide to fit renderer limits. PDF page size and slide layout are unchanged.",
        options.width, options.height,
    ));
    Ok((options, notice))
}

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;
