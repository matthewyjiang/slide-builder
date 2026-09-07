//! Experimental pre-exec Seatbelt isolation for the embedded Obscura worker.
//! SBPL is undocumented by Apple. Keep permissions explicit and qualify each
//! supported macOS release; never broaden policy just to make a capture succeed.
use super::{validate_executable, CaptureOptions};
use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use tokio::process::Command;

const PROFILE: &str = include_str!("macos.sb");

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Sandbox {
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

    pub fn command(&self, executable: &Path, html: &Path, output: &Path) -> Result<Command> {
        let executable = validate_executable(executable)?;
        let html = fs::canonicalize(html).context("resolve private capture HTML")?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .context("create private screenshot without replacing existing files")?;
        let output = fs::canonicalize(output).context("resolve private screenshot")?;
        let mut command = Command::new(&self.executable);
        command
            .env_clear()
            .env("SLIDE_BUILDER_SANDBOXED", "1")
            .current_dir("/");
        // Parameters are separate argv values, never interpolated into SBPL.
        // sandbox-exec applies the policy before the renderer's loader and native
        // initializers run. No inherited home, proxy, or DYLD_* configuration.
        for (name, path) in [
            ("EXECUTABLE", &executable),
            ("INPUT", &html),
            ("OUTPUT", &output),
        ] {
            let mut parameter = std::ffi::OsString::from(format!("{name}="));
            parameter.push(path);
            command.arg("-D").arg(parameter);
        }
        command.arg("-p").arg(PROFILE).arg(executable);
        Ok(command)
    }
}

pub(super) fn capture_command(
    sandbox: &Sandbox,
    executable: &Path,
    html: &Path,
    output: &Path,
    options: &CaptureOptions,
) -> Result<Command> {
    let mut command = sandbox.command(executable, html, output)?;
    command
        .args([
            super::super::worker::WORKER_ARGUMENT,
            &options.width.to_string(),
            &options.height.to_string(),
            &options.scale.to_string(),
            &options.timeout.as_millis().to_string(),
        ])
        .arg(fs::canonicalize(html)?)
        .arg(fs::canonicalize(output)?);
    Ok(command)
}

#[cfg(test)]
#[path = "macos_tests.rs"]
mod tests;
