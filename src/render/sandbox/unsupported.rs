//! No unsandboxed substitute for a supported Obscura backend.
use super::{Launch, Request};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::render) struct Sandbox {
    pub executable: PathBuf,
}

impl Sandbox {
    pub fn probe(_configured: &Path) -> Result<Self> {
        bail!(crate::render::worker::UNSUPPORTED)
    }

    pub fn command(&self, _request: Request<'_>) -> Result<Launch> {
        bail!(crate::render::worker::UNSUPPORTED)
    }
}
