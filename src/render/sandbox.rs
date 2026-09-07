//! Platform isolation for a worker executable and its input/output files.
//! Renderer arguments, deadlines, process cleanup, and cache identity belong to
//! the caller. Preparing a launch creates its output file without replacing it.
use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
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

/// Canonical host paths after the shared create-without-replace output policy.
pub(super) struct Resolved<'a> {
    pub executable: &'a Path,
    pub input: PathBuf,
    pub output: PathBuf,
}

impl<'a> Request<'a> {
    /// Refuse existing files or symlinks. The pipeline removes its previous
    /// temporary output before each retry; a renderer cannot redirect writes.
    pub(super) fn resolve(self) -> Result<Resolved<'a>> {
        let input = fs::canonicalize(self.input).context("resolve capture input")?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.output)
            .with_context(|| format!("create private output {}", self.output.display()))?;
        let output = fs::canonicalize(self.output).context("resolve capture output")?;
        Ok(Resolved {
            executable: self.executable,
            input,
            output,
        })
    }
}

/// A sandbox command and the capture paths visible inside its isolation boundary.
pub(super) struct Launch {
    pub command: Command,
    pub input: PathBuf,
    pub output: PathBuf,
}
