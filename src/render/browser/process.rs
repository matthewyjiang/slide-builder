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
pub(in crate::render) async fn run(
    mut command: Command,
    timeout: Duration,
) -> Result<CaptureDiagnostics> {
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
    // Keep bounded diagnostics outside the timed future so cancellation does
    // not discard the only explanation of a renderer startup failure.
    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    let mut renderer_status = None;
    let operation = async {
        let (status, (), ()) = tokio::try_join!(
            async {
                let status = child.wait().await.context("wait for renderer")?;
                renderer_status = Some(status);
                Ok::<_, anyhow::Error>(status)
            },
            read_bounded(stdout, &mut stdout_bytes),
            read_bounded(stderr, &mut stderr_bytes),
        )?;
        if !status.success() {
            bail!(
                "renderer exited with {status}: {}",
                String::from_utf8_lossy(&stderr_bytes).trim()
            );
        }
        Ok(CaptureDiagnostics {
            stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
        })
    };
    let result = match tokio::time::timeout(timeout, operation).await {
        Ok(result) => result,
        Err(_) => {
            let state = match renderer_status {
                Some(status) => {
                    format!("renderer exited with {status}, but helper output pipes remained open")
                }
                None => "renderer completion was not observed".into(),
            };
            Err(anyhow::anyhow!(
                "renderer capture timed out after {} ms; {state}; stderr: {}; stdout: {}",
                timeout.as_millis(),
                String::from_utf8_lossy(&stderr_bytes).trim(),
                String::from_utf8_lossy(&stdout_bytes).trim(),
            ))
        }
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

async fn read_bounded(mut reader: impl AsyncRead + Unpin, all: &mut Vec<u8>) -> Result<()> {
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
    Ok(())
}
