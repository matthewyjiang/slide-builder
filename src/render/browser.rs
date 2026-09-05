//! Renderer discovery and constrained HTML-to-PNG capture.
use crate::config::{RenderConfig, RenderEngine};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;

mod chromium;
mod obscura;
mod process;

#[derive(Clone, Debug, Eq, PartialEq)]
enum Engine {
    Chromium,
    Obscura(obscura::Sandbox),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Browser {
    executable: PathBuf,
    engine: Engine,
    cache_identity: Arc<str>,
}

#[derive(Clone, Debug)]
pub struct CaptureOptions {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub timeout: Duration,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            scale: 1.0,
            timeout: Duration::from_secs(60),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CaptureDiagnostics {
    pub stderr: String,
    pub stdout: String,
}

impl Browser {
    /// Select exactly the configured engine. Missing Obscura or isolation is
    /// an error, never permission to fall back to an unisolated renderer.
    pub fn probe(config: &RenderConfig) -> Result<Self> {
        match config.engine {
            RenderEngine::Chromium => Self::probe_chromium(Some(&config.browser_path)),
            RenderEngine::Obscura => {
                let executable = executable_path(&config.obscura_path, &["obscura"])
                    .context("no Obscura renderer found; install a render-enabled Obscura release (0.2.2 or newer) or configure render.obscura_path")?;
                let sandbox = obscura::Sandbox::probe(&config.sandbox_path)?;
                let identity = format!(
                    "obscura-isolated-v1-{}-{}",
                    executable_identity(&executable)?,
                    executable_identity(&sandbox.executable)?
                );
                Ok(Self {
                    executable,
                    engine: Engine::Obscura(sandbox),
                    cache_identity: identity.into(),
                })
            }
        }
    }

    pub fn probe_chromium(configured: Option<&Path>) -> Result<Self> {
        let executable = executable_path(
            configured.unwrap_or(Path::new("auto")),
            chromium::CANDIDATES,
        )
        .context("no Chromium-family browser found; configure render.browser_path")?;
        Self::from_path(&executable)
    }

    /// Explicit Chromium constructor, also useful with test capture executables.
    pub fn from_path(path: &Path) -> Result<Self> {
        let executable = validate_executable(path)?;
        let cache_identity = format!("chromium-v1-{}", executable_identity(&executable)?).into();
        Ok(Self {
            executable,
            engine: Engine::Chromium,
            cache_identity,
        })
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Includes adapter policy and executable installation identity, not just
    /// the engine name. Restart after upgrading a renderer.
    pub fn cache_identity(&self) -> &str {
        &self.cache_identity
    }

    pub fn validate_options(&self, options: &CaptureOptions) -> Result<()> {
        if options.width == 0
            || options.height == 0
            || options.width > 16_384
            || options.height > 16_384
        {
            bail!(
                "capture dimensions must be between 1 and 16384 pixels; requested {}x{}",
                options.width,
                options.height
            );
        }
        if !options.scale.is_finite() || !(0.25..=4.0).contains(&options.scale) {
            bail!(
                "device scale must be finite and between 0.25 and 4.0; requested {}",
                options.scale
            );
        }
        if options.timeout.is_zero() {
            bail!("capture timeout must be positive; requested 0 ms");
        }
        match self.engine {
            Engine::Obscura(_) if options.scale != 1.0 => {
                bail!("Obscura requires preview.scale = 1; requested {}. Increase preview.width for larger native captures, or select render.engine = \"chromium\" for device scaling", options.scale);
            }
            Engine::Obscura(_) | Engine::Chromium => Ok(()),
        }
    }

    pub async fn capture(
        &self,
        html: &Path,
        output: &Path,
        profile: &Path,
        options: &CaptureOptions,
    ) -> Result<CaptureDiagnostics> {
        self.validate_options(options)?;
        for (path, label) in [
            (html, "capture HTML"),
            (output, "screenshot"),
            (profile, "browser profile"),
        ] {
            validate_capture_path(path, label)?;
        }
        let root = html
            .parent()
            .context("capture HTML has no parent directory")?;
        if output.parent() != Some(root)
            || profile.parent() != Some(root)
            || html == output
            || html == profile
            || output == profile
        {
            bail!(
                "browser capture paths must be distinct siblings in the private render directory"
            );
        }
        let command = match &self.engine {
            Engine::Chromium => {
                fs::create_dir_all(profile).context("create isolated browser profile")?;
                let mut command = Command::new(&self.executable);
                command.args(chromium::capture_args(html, output, profile, options)?);
                command
            }
            Engine::Obscura(sandbox) => {
                obscura::capture_command(sandbox, &self.executable, html, output, options)?
            }
        };
        let diagnostics = process::run(command, options.timeout).await.with_context(|| match self.engine {
            Engine::Chromium => "Chromium capture failed",
            Engine::Obscura(_) => "isolated Obscura capture failed; bubblewrap must support unprivileged user/network namespaces. No unsandboxed fallback is allowed",
        })?;
        if !output.is_file() || output.metadata()?.len() == 0 {
            bail!(
                "renderer succeeded without producing screenshot {} (stdout: {}; stderr: {})",
                output.display(),
                diagnostics.stdout.trim(),
                diagnostics.stderr.trim()
            );
        }
        Ok(diagnostics)
    }
}

fn executable_path(configured: &Path, candidates: &[&str]) -> Result<PathBuf> {
    if configured != Path::new("auto") {
        return validate_executable(configured);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    let directories: Vec<_> = std::env::split_paths(&path)
        .chain([PathBuf::from("/usr/bin"), PathBuf::from("/usr/local/bin")])
        .collect();
    for name in candidates {
        for directory in &directories {
            if let Ok(path) = validate_executable(&directory.join(name)) {
                return Ok(path);
            }
        }
    }
    bail!("executable not found on PATH: {}", candidates.join(", "))
}

fn validate_executable(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("renderer path must be absolute: {}", path.display());
    }
    let metadata = fs::metadata(path)
        .with_context(|| format!("cannot inspect executable {}", path.display()))?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        bail!("not an executable file: {}", path.display());
    }
    fs::canonicalize(path).with_context(|| format!("resolve executable {}", path.display()))
}

/// Stat tracks replacement and in-place updates without hashing hundreds of
/// megabytes at startup. That took 2-4 seconds in the measured debug build.
/// ctime also catches rewrites that restore mtime; the renderer is trusted code,
/// so this is cache invalidation, not executable authenticity verification.
fn executable_identity(path: &Path) -> Result<String> {
    let metadata = fs::metadata(path)?;
    let mut hash = Sha256::new();
    hash.update(path.as_os_str().as_bytes());
    hash.update([0]);
    hash.update(metadata.dev().to_le_bytes());
    hash.update(metadata.ino().to_le_bytes());
    hash.update(metadata.size().to_le_bytes());
    hash.update(metadata.mtime().to_le_bytes());
    hash.update(metadata.mtime_nsec().to_le_bytes());
    hash.update(metadata.ctime().to_le_bytes());
    hash.update(metadata.ctime_nsec().to_le_bytes());
    Ok(format!("{:x}", hash.finalize()))
}

fn validate_capture_path(path: &Path, label: &str) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        bail!(
            "{label} path must be absolute without parent traversal: {}",
            path.display()
        );
    }
    if path.as_os_str().as_bytes().contains(&0) {
        bail!("{label} path contains NUL");
    }
    Ok(())
}

fn percent_encode_path(path: &Path) -> Result<String> {
    let text = path.to_str().context("capture path is not valid UTF-8")?;
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    Ok(encoded)
}

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
