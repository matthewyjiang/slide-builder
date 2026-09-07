//! Private renderer entry point, available only with a supported isolation backend.
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod native;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use native::run_if_requested;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) use native::WORKER_ARGUMENT;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) const UNSUPPORTED: &str = "Obscura requires Linux with bubblewrap or experimental macOS sandbox-exec isolation; this platform has no supported native sandbox";

/// Reject worker requests on platforms without the embedded sandbox backend.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn run_if_requested() -> anyhow::Result<bool> {
    if std::env::args_os().nth(1).as_deref()
        == Some(std::ffi::OsStr::new("--slide-builder-render-worker"))
    {
        anyhow::bail!(UNSUPPORTED);
    }
    Ok(false)
}
