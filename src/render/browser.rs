//! Discovery and constrained execution of a Chromium-family browser.

use anyhow::{bail, Context, Result};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

const CANDIDATES: &[&str] = &[
    "google-chrome-stable",
    "google-chrome",
    "chromium",
    "chromium-browser",
    "microsoft-edge-stable",
    "microsoft-edge",
];
const DIAGNOSTIC_LIMIT: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Browser {
    executable: PathBuf,
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
    /// Probe an explicit path first. `None` and `"auto"` search PATH and common
    /// absolute Linux installation paths.
    pub fn probe(configured: Option<&Path>) -> Result<Self> {
        if let Some(path) = configured {
            if path != Path::new("auto") {
                return Self::from_path(path);
            }
        }
        for name in CANDIDATES {
            if let Some(path) = find_in_path(OsStr::new(name)) {
                if let Ok(browser) = Self::from_path(&path) {
                    return Ok(browser);
                }
            }
        }
        for path in [
            "/usr/bin/google-chrome-stable",
            "/usr/bin/google-chrome",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/opt/google/chrome/chrome",
            "/usr/bin/microsoft-edge-stable",
        ] {
            if let Ok(browser) = Self::from_path(Path::new(path)) {
                return Ok(browser);
            }
        }
        bail!("no supported Chromium-family browser found; configure render.browser_path")
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        if !path.is_absolute() {
            bail!("browser path must be absolute: {}", path.display());
        }
        let metadata = fs::metadata(path)
            .with_context(|| format!("cannot inspect browser {}", path.display()))?;
        if !metadata.is_file() {
            bail!("browser is not a regular file: {}", path.display());
        }
        if metadata.permissions().mode() & 0o111 == 0 {
            bail!("browser is not executable: {}", path.display());
        }
        Ok(Self {
            executable: path.to_path_buf(),
        })
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Returns the exact fixed argument set used by `capture`. Paths are passed
    /// as OS strings and never interpreted by a shell.
    pub fn capture_args(
        &self,
        html: &Path,
        output: &Path,
        profile: &Path,
        options: &CaptureOptions,
    ) -> Result<Vec<OsString>> {
        validate_capture_path(html, "capture HTML")?;
        validate_capture_path(output, "screenshot")?;
        validate_capture_path(profile, "browser profile")?;
        let render_root = html
            .parent()
            .context("capture HTML has no parent directory")?;
        if !output.starts_with(render_root) || !profile.starts_with(render_root) {
            bail!("browser capture paths must share the private render directory");
        }
        if options.width == 0
            || options.height == 0
            || options.width > 16_384
            || options.height > 16_384
        {
            bail!("capture dimensions must be between 1 and 16384 pixels");
        }
        if !options.scale.is_finite() || !(0.25..=4.0).contains(&options.scale) {
            bail!("device scale must be finite and between 0.25 and 4.0");
        }
        let url = format!("file://{}", percent_encode_path(html)?);
        Ok(vec![
            "--headless=new".into(), "--hide-scrollbars".into(),
            "--disable-background-networking".into(), "--disable-component-update".into(),
            "--disable-default-apps".into(), "--disable-domain-reliability".into(),
            "--disable-features=Translate,MediaRouter,OptimizationHints,AutofillServerCommunication".into(),
            "--disable-sync".into(), "--metrics-recording-only".into(),
            "--no-first-run".into(), "--no-pings".into(),
            "--password-store=basic".into(), "--use-mock-keychain".into(),
            // Intentionally no --no-sandbox and no remote debugging interface.
            format!("--user-data-dir={}", profile.display()).into(),
            format!("--window-size={},{}", options.width, options.height).into(),
            format!("--force-device-scale-factor={}", options.scale).into(),
            format!("--virtual-time-budget={}", options.timeout.as_millis()).into(),
            format!("--screenshot={}", output.display()).into(), url.into(),
        ])
    }

    pub async fn capture(
        &self,
        html: &Path,
        output: &Path,
        profile: &Path,
        options: &CaptureOptions,
    ) -> Result<CaptureDiagnostics> {
        let args = self.capture_args(html, output, profile, options)?;
        fs::create_dir_all(profile).context("create isolated browser profile")?;
        let mut command = Command::new(&self.executable);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            // Isolate Chromium and all of its helper processes. The guard below
            // kills this whole group on timeout or future cancellation.
            .process_group(0);
        let mut child = command.spawn().context("launch browser")?;
        let mut process_group =
            ProcessGroupGuard(child.id().context("browser has no process id")? as i32);
        let stdout = child.stdout.take().context("capture browser stdout")?;
        let stderr = child.stderr.take().context("capture browser stderr")?;
        let out_task = tokio::spawn(read_bounded(stdout));
        let err_task = tokio::spawn(read_bounded(stderr));
        let status = match tokio::time::timeout(options.timeout, child.wait()).await {
            Ok(status) => {
                let status = status.context("wait for browser")?;
                process_group.disarm();
                status
            }
            Err(_) => {
                process_group.kill();
                let _ = child.wait().await;
                bail!(
                    "browser capture timed out after {} ms",
                    options.timeout.as_millis()
                );
            }
        };
        let stdout =
            String::from_utf8_lossy(&out_task.await.context("join stdout reader")??).into_owned();
        let stderr =
            String::from_utf8_lossy(&err_task.await.context("join stderr reader")??).into_owned();
        if !status.success() {
            bail!("browser exited with {status}: {}", stderr.trim());
        }
        if !output.is_file() {
            bail!("browser succeeded without producing a screenshot");
        }
        Ok(CaptureDiagnostics { stderr, stdout })
    }
}

struct ProcessGroupGuard(i32);

impl ProcessGroupGuard {
    fn disarm(&mut self) {
        self.0 = 0;
    }

    fn kill(&mut self) {
        if self.0 != 0 {
            // Linux kill(2) with a negative PID targets the process group. This
            // tiny FFI avoids adding a libc dependency solely for one syscall.
            unsafe extern "C" {
                fn kill(pid: i32, signal: i32) -> i32;
            }
            const SIGKILL: i32 = 9;
            // Failure is intentionally best-effort. The child handle retains
            // tokio's kill-on-drop fallback for the browser parent process.
            let _ = unsafe { kill(-self.0, SIGKILL) };
            self.0 = 0;
        }
    }
}

impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        self.kill();
    }
}

async fn read_bounded(mut reader: impl AsyncRead + Unpin) -> Result<Vec<u8>> {
    let mut all = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let n = reader.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        if all.len() < DIAGNOSTIC_LIMIT {
            let remaining = DIAGNOSTIC_LIMIT - all.len();
            all.extend_from_slice(&chunk[..n.min(remaining)]);
        }
    }
    Ok(all)
}

fn validate_capture_path(path: &Path, label: &str) -> Result<()> {
    if !path.is_absolute() {
        bail!("{label} path must be absolute: {}", path.display());
    }
    if path.as_os_str().as_bytes().contains(&0) {
        bail!("{label} path contains NUL");
    }
    Ok(())
}

fn find_in_path(name: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|p| p.join(name))
        .find(|p| p.is_file())
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
mod tests {
    use super::*;
    #[test]
    fn args_preserve_sandbox_and_disable_network_features() {
        let browser = Browser {
            executable: "/bin/true".into(),
        };
        let args = browser
            .capture_args(
                Path::new("/tmp/a b.html"),
                Path::new("/tmp/o.png"),
                Path::new("/tmp/p"),
                &CaptureOptions::default(),
            )
            .unwrap();
        let joined = args
            .iter()
            .map(|x| x.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(!joined.contains("--no-sandbox"));
        assert!(!joined.contains("remote-debugging"));
        assert!(joined.contains("file:///tmp/a%20b.html"));
    }
    #[test]
    fn rejects_relative_and_invalid_geometry() {
        let b = Browser {
            executable: "/bin/true".into(),
        };
        assert!(b
            .capture_args(
                Path::new("x"),
                Path::new("/tmp/o"),
                Path::new("/tmp/p"),
                &CaptureOptions::default()
            )
            .is_err());
        let mut o = CaptureOptions::default();
        o.scale = f32::NAN;
        assert!(b
            .capture_args(
                Path::new("/tmp/x"),
                Path::new("/tmp/o"),
                Path::new("/tmp/p"),
                &o
            )
            .is_err());
    }
}
