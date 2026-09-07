//! Private renderer entry point, available only with a supported isolation backend.
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod native;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use native::run_if_requested;
pub(crate) const WORKER_ARGUMENT: &str = "--slide-builder-render-worker";

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) const UNSUPPORTED: &str = "Obscura requires Linux with bubblewrap or experimental macOS sandbox-exec isolation; this platform has no supported native sandbox. Unsandboxed rendering is not allowed; set render.engine = \"chromium\" to use Chromium";

/// Reject worker requests on platforms without the embedded sandbox backend.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn run_if_requested() -> anyhow::Result<bool> {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new(WORKER_ARGUMENT)) {
        anyhow::bail!(UNSUPPORTED);
    }
    Ok(false)
}
