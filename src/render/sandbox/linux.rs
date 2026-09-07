//! Fail-closed Linux isolation for the embedded Obscura worker.
//!
//! The renderer sees only its executable, read-only runtime libraries/fonts,
//! one input HTML file, and one writable output file. It cannot see the host
//! render directory, home, project, sockets, credentials, or network namespace.
use super::{Launch, Request};
use crate::render::executable::executable_path;
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::render) struct Sandbox {
    pub executable: PathBuf,
    font_home: Option<PathBuf>,
}

impl Sandbox {
    pub fn probe(configured: &Path) -> Result<Self> {
        let executable = executable_path(configured, &["bwrap"])
            .context("Obscura requires bubblewrap (bwrap); install it or configure render.sandbox_path. Unsandboxed rendering is not allowed")?;
        Ok(Self {
            executable,
            font_home: std::env::var_os("HOME").map(PathBuf::from),
        })
    }

    /// Build a fresh namespace for each capture. Only explicitly selected input
    /// and output files are bind-mounted, never their containing directories.
    pub fn command(&self, request: Request<'_>) -> Result<Launch> {
        let Request {
            executable,
            input: html,
            output,
        } = request;
        let html = fs::canonicalize(html).context("resolve capture HTML")?;
        // Refuse existing files/symlinks. The pipeline removes its previous
        // temporary output before each retry; a renderer cannot redirect writes.
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .with_context(|| format!("create private screenshot {}", output.display()))?;
        let output = fs::canonicalize(output).context("resolve screenshot output")?;
        let mut args: Vec<OsString> = [
            "--unshare-all",
            "--unshare-user",
            "--disable-userns",
            "--die-with-parent",
            "--new-session",
            "--cap-drop",
            "ALL",
            "--clearenv",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--dir",
            "/home/renderer",
            "--setenv",
            "HOME",
            "/home/renderer",
            "--setenv",
            "XDG_CACHE_HOME",
            "/tmp/cache",
            "--setenv",
            "LANG",
            "C.UTF-8",
            "--setenv",
            "PATH",
            "/app",
            "--chdir",
            "/input",
        ]
        .map(OsString::from)
        .into();
        // Keep the dynamic loader's conventional paths across Linux layouts.
        // No /usr/bin, /usr/local, /etc, /run, or host /tmp wholesale mounts.
        for path in [
            "/usr/lib",
            "/usr/lib64",
            "/lib",
            "/lib64",
            "/etc/ld.so.cache",
            "/etc/fonts",
            "/usr/share/fonts",
            "/usr/local/share/fonts",
            "/usr/share/fontconfig",
            "/var/cache/fontconfig",
        ] {
            if Path::new(path).exists() {
                args.extend(["--ro-bind".into(), path.into(), path.into()]);
            }
        }
        if let Some(home) = &self.font_home {
            for relative in [".fonts", ".local/share/fonts"] {
                let source = home.join(relative);
                if source.is_dir() {
                    args.extend([
                        "--ro-bind".into(),
                        source.into_os_string(),
                        Path::new("/home/renderer").join(relative).into_os_string(),
                    ]);
                }
            }
        }
        args.extend([
            "--ro-bind".into(),
            executable.as_os_str().to_owned(),
            "/app/slide-builder".into(),
            "--ro-bind".into(),
            html.into_os_string(),
            "/input/capture.html".into(),
            "--bind".into(),
            output.into_os_string(),
            "/output/capture.png".into(),
            "--".into(),
            "/app/slide-builder".into(),
        ]);
        let mut command = Command::new(&self.executable);
        // Clear before launching bwrap too: LD_PRELOAD, proxies, credentials,
        // permissive OBSCURA_* settings and inherited display sockets stay out.
        command.env_clear().args(args);
        Ok(Launch {
            command,
            input: "/input/capture.html".into(),
            output: "/output/capture.png".into(),
        })
    }
}

#[cfg(test)]
#[path = "linux_tests.rs"]
mod tests;
