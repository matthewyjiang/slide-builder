//! No unsandboxed substitute for a supported Obscura backend.
use super::Launch;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Sandbox {
    pub executable: PathBuf,
}

impl Sandbox {
    pub fn probe(_configured: &Path) -> Result<Self> {
        bail!(crate::render::worker::UNSUPPORTED)
    }

    pub fn command(&self, _executable: &Path, _html: &Path, _output: &Path) -> Result<Launch> {
        bail!(crate::render::worker::UNSUPPORTED)
    }
}
