//! Experimental pre-exec Seatbelt isolation for the embedded Obscura worker.
//! SBPL is undocumented by Apple. Keep permissions explicit and qualify each
//! supported macOS release; never broaden policy just to make a capture succeed.
use super::{Launch, Request, Resolved};
use crate::render::executable::validate_executable;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;

const PROFILE: &str = include_str!("macos.sb");

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::render) struct Sandbox {
    pub executable: PathBuf,
}

impl Sandbox {
    pub fn probe(configured: &Path) -> Result<Self> {
        let path = if configured == Path::new("auto") {
            Path::new("/usr/bin/sandbox-exec")
        } else {
            configured
        };
        let executable = validate_executable(path).context("Obscura on macOS requires sandbox-exec; restore /usr/bin/sandbox-exec or configure render.sandbox_path. Unsandboxed rendering is not allowed")?;
        Ok(Self { executable })
    }

    pub fn command(&self, request: Request<'_>) -> Result<Launch> {
        let Resolved {
            executable,
            input,
            output,
        } = request.resolve()?;
        let executable = validate_executable(executable)?;
        let mut command = Command::new(&self.executable);
        command
            .env_clear()
            .env("SLIDE_BUILDER_SANDBOXED", "1")
            .current_dir("/");
        // Parameters are separate argv values, never interpolated into SBPL.
        // sandbox-exec applies the policy before the renderer's loader and native
        // initializers run. No inherited home, proxy, or DYLD_* configuration.
        for (name, path) in [
            ("EXECUTABLE", executable.as_path()),
            ("INPUT", input.as_path()),
            ("OUTPUT", output.as_path()),
        ] {
            let mut parameter = std::ffi::OsString::from(format!("{name}="));
            parameter.push(path);
            command.arg("-D").arg(parameter);
        }
        command.arg("-p").arg(PROFILE).arg(executable);
        Ok(Launch {
            command,
            input,
            output,
        })
    }
}

#[cfg(test)]
#[path = "macos_tests.rs"]
mod tests;
