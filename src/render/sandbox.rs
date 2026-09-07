//! Platform isolation for a worker executable and its input/output files.
//! Renderer arguments, deadlines, process cleanup, and cache identity belong to
//! the caller. Preparing a launch creates its output file without replacing it.
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[cfg(target_os = "linux")]
#[path = "sandbox/linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "sandbox/macos.rs"]
mod platform;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[path = "sandbox/unsupported.rs"]
mod platform;

pub(super) use platform::Sandbox;

/// Host paths exposed to the isolated worker. The caller validates capture paths
/// and supplies a trusted executable before requesting a launch.
pub(super) struct Request<'a> {
    pub executable: &'a Path,
    pub input: &'a Path,
    pub output: &'a Path,
}

/// A sandbox command and the capture paths visible inside its isolation boundary.
pub(super) struct Launch {
    pub command: Command,
    pub input: PathBuf,
    pub output: PathBuf,
}
