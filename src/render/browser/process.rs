//! Bounded subprocess execution shared by renderer adapters.
use super::CaptureDiagnostics;
use anyhow::{bail, Context, Result};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

const DIAGNOSTIC_LIMIT: usize = 64 * 1024;

/// Kill the whole process group on cancellation as well as wall-clock timeout.
/// Bubblewrap additionally ties its isolated PID namespace to its parent's life.
pub(super) async fn run(mut command: Command, timeout: Duration) -> Result<CaptureDiagnostics> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .process_group(0);
    let mut child = command.spawn().context("launch renderer")?;
    let group = ProcessGroupGuard(child.id().context("renderer has no process id")? as i32);
    let stdout = child.stdout.take().context("capture renderer stdout")?;
    let stderr = child.stderr.take().context("capture renderer stderr")?;
    let operation = async {
        let (status, stdout, stderr) = tokio::try_join!(
            async { child.wait().await.context("wait for renderer") },
            read_bounded(stdout),
            read_bounded(stderr),
        )?;
        if !status.success() {
            bail!(
                "renderer exited with {status}: {}",
                String::from_utf8_lossy(&stderr).trim()
            );
        }
        Ok(CaptureDiagnostics {
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    };
    let result = match tokio::time::timeout(timeout, operation).await {
        Ok(result) => result,
        Err(_) => Err(anyhow::anyhow!(
            "renderer capture timed out after {} ms",
            timeout.as_millis()
        )),
    };
    // Kill lingering helpers even if the parent exited successfully. This also
    // prevents an inherited pipe from outliving a failed or cancelled capture.
    drop(group);
    let _ = child.wait().await;
    result
}

struct ProcessGroupGuard(i32);
impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        // Negative PID targets the group created immediately before spawn.
        unsafe {
            libc::kill(-self.0, libc::SIGKILL);
        }
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
