//! Private executable entry point for isolated native rendering.
//!
//! Call before starting an application's runtime. Rendering stays on one thread
//! in a disposable process; the parent owns the hard deadline and kills the
//! bubblewrap process group on cancellation, including synchronous native hangs.
use anyhow::{bail, Context, Result};
use obscura_browser::{BrowserContext, Page, WaitUntil};
use std::{ffi::OsStr, path::Path, sync::Arc, time::Duration};

pub(crate) const WORKER_ARGUMENT: &str = "--slide-builder-render-worker";

/// Dispatch the private worker mode. Executables using `Browser::probe` must
/// call this before handling normal arguments so they can render as subprocesses.
#[doc(hidden)]
pub fn run_if_requested() -> Result<bool> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(OsStr::new(WORKER_ARGUMENT)) {
        return Ok(false);
    }
    // This is an accidental-invocation guard, not the security boundary. Only
    // Sandbox::command establishes isolation; no public URL/path input exists.
    if std::env::current_exe()? != Path::new("/app/slide-builder") {
        bail!("private render worker must be launched through the preview sandbox");
    }
    let width = args
        .next()
        .context("missing worker width")?
        .to_str()
        .context("invalid worker width")?
        .parse::<u32>()?;
    let height = args
        .next()
        .context("missing worker height")?
        .to_str()
        .context("invalid worker height")?
        .parse::<u32>()?;
    let scale = args
        .next()
        .context("missing worker scale")?
        .to_str()
        .context("invalid worker scale")?
        .parse::<f32>()?;
    let timeout_ms = args
        .next()
        .context("missing worker deadline")?
        .to_str()
        .context("invalid worker deadline")?
        .parse::<u64>()?;
    if args.next().is_some() {
        bail!("unexpected private render worker arguments");
    }
    let options = crate::render::browser::CaptureOptions {
        width,
        height,
        scale,
        timeout: Duration::from_millis(timeout_ms),
    };
    // Reuse the public capture limits before allocating the DOM or pixel buffer.
    options.validate_for_engine(crate::config::RenderEngine::Obscura)?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(capture(&options))?;
    Ok(true)
}

async fn capture(options: &crate::render::browser::CaptureOptions) -> Result<()> {
    let context = Arc::new(BrowserContext::new("slide-preview".into()));
    let mut page = Page::new("slide".into(), context);
    let viewport = (options.width as f32, options.height as f32);
    page.set_viewport(viewport);
    page.set_device_scale_factor(options.scale);
    page.set_navigation_timeout(options.timeout);
    page.navigate_with_wait("file:///input/capture.html", WaitUntil::Load)
        .await
        .context("load private capture HTML")?;
    // Match the qualified CLI's --wait 0 and screenshot resource preparation.
    page.settle(0).await;
    // v0.2.2's screenshot CLI uses a 3-second resource preparation budget.
    // Preserve that settling policy; the parent still owns the total deadline.
    const RESOURCE_PREPARATION_BUDGET_MS: u64 = 3_000;
    page.prepare_screenshot_resources(RESOURCE_PREPARATION_BUDGET_MS)
        .await;
    let png = if options.scale == 1.0 {
        // Preserve the qualified scale-one raster path and its exact pixels.
        page.screenshot(viewport)
            .context("Obscura produced no screenshot")?
    } else {
        page.screenshot_region(options.capture_region())
            .map_err(|error| anyhow::anyhow!("Obscura scaled screenshot failed: {error:?}"))?
    };
    std::fs::write("/output/capture.png", png).context("write private screenshot")
}
