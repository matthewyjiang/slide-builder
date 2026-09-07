//! No unsandboxed substitute for the Linux-only Obscura backend.
use super::CaptureOptions;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Sandbox {
    pub executable: PathBuf,
}

impl Sandbox {
    pub fn probe(_configured: &Path) -> Result<Self> {
        bail!(crate::render::worker::UNSUPPORTED)
    }
}

pub(super) fn capture_command(
    _sandbox: &Sandbox,
    _executable: &Path,
    _html: &Path,
    _output: &Path,
    _options: &CaptureOptions,
) -> Result<Command> {
    bail!(crate::render::worker::UNSUPPORTED)
}
