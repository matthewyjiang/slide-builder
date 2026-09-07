//! Private renderer entry point, available only with Linux isolation.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::run_if_requested;
#[cfg(target_os = "linux")]
pub(crate) use linux::WORKER_ARGUMENT;

#[cfg(not(target_os = "linux"))]
pub(crate) const UNSUPPORTED: &str = "Obscura is supported only on Linux with bubblewrap; set render.engine = \"chromium\" and install Google Chrome, Chromium, Brave, or Microsoft Edge";

/// Reject worker requests on platforms without the embedded sandbox backend.
#[cfg(not(target_os = "linux"))]
pub fn run_if_requested() -> anyhow::Result<bool> {
    if std::env::args_os().nth(1).as_deref()
        == Some(std::ffi::OsStr::new("--slide-builder-render-worker"))
    {
        anyhow::bail!(UNSUPPORTED);
    }
    Ok(false)
}
