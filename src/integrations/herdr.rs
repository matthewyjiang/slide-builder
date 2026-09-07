//! Native, newline-framed Herdr socket requests. No subprocesses or terminal output.

use std::{
    env, io,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde_json::{json, Value};
use tokio::{sync::watch, task::JoinHandle};

// Match Rho's Herdr budgets: 500 ms for status, 100 ms for graphics, and
// 64 KiB per response. Read one extra byte to distinguish overflow from EOF.
const REQUEST_TIMEOUT: Duration = Duration::from_millis(500);
const GRAPHICS_PROBE_TIMEOUT: Duration = Duration::from_millis(100);
#[cfg(unix)]
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;
const SOURCE: &str = "herdr:slide-builder";
const AGENT: &str = "slide-builder";

type Diagnostics = Arc<Mutex<Option<String>>>;

#[derive(Clone, Debug, Default)]
pub struct HerdrClient {
    config: Option<Config>,
}

#[derive(Clone, Debug)]
struct Config {
    socket_path: PathBuf,
    pane_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HerdrState {
    Idle,
    Working,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HerdrGraphicsCapability {
    NotHerdr,
    Paintable { width: u16, height: u16 },
    Unpaintable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Report {
    state: HerdrState,
    message: Option<String>,
}

/// One sequential worker owns registration, state updates, and release.
/// Pending updates coalesce to the latest state; an in-flight update completes
/// before that latest state is sent. Successful identical updates are skipped;
/// failed updates retry on the next state or message change, not on UI ticks.
/// Dropping also closes the queue; `shutdown` additionally waits for release.
#[derive(Debug)]
pub struct HerdrReporter {
    updates: Option<watch::Sender<Option<Report>>>,
    worker: Option<JoinHandle<io::Result<()>>>,
    diagnostics: Diagnostics,
}

impl HerdrClient {
    pub fn from_env() -> Self {
        Self::from_env_vars(|key| env::var(key).ok())
    }

    pub(crate) fn from_env_vars(mut get_var: impl FnMut(&str) -> Option<String>) -> Self {
        let enabled = cfg!(unix) && get_var("HERDR_ENV").as_deref() == Some("1");
        let socket_path = get_var("HERDR_SOCKET_PATH").filter(|value| !value.is_empty());
        let pane_id = get_var("HERDR_PANE_ID").filter(|value| !value.is_empty());
        let config =
            enabled
                .then_some((socket_path, pane_id))
                .and_then(|(socket_path, pane_id)| {
                    Some(Config {
                        socket_path: socket_path?.into(),
                        pane_id: pane_id?,
                    })
                });
        Self { config }
    }

    /// Under a configured Herdr pane, failed or incomplete probes are unpaintable.
    pub async fn graphics_capability(&self) -> HerdrGraphicsCapability {
        let Some(config) = &self.config else {
            return HerdrGraphicsCapability::NotHerdr;
        };
        let result = exchange(
            config,
            "pane.graphics.info",
            json!({"pane_id": config.pane_id}),
            GRAPHICS_PROBE_TIMEOUT,
        )
        .await;
        let cells = result.ok().and_then(|result| {
            let pixel_size = |key| {
                result
                    .get(key)
                    .and_then(Value::as_u64)
                    .and_then(|value| u16::try_from(value).ok())
                    .filter(|value| *value > 0)
            };
            Some((pixel_size("cell_width_px")?, pixel_size("cell_height_px")?))
        });
        match cells {
            Some((width, height)) => HerdrGraphicsCapability::Paintable { width, height },
            None => HerdrGraphicsCapability::Unpaintable,
        }
    }

    /// Starts reporting within an active Tokio runtime. Outside Tokio, or when
    /// Herdr is not configured, returns an inert reporter rather than panicking.
    pub fn start_reporting(&self, session_id: &str) -> HerdrReporter {
        let diagnostics = Diagnostics::default();
        let mut reporter = HerdrReporter {
            updates: None,
            worker: None,
            diagnostics,
        };
        let Some(config) = self.config.clone() else {
            return reporter;
        };
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            record_error(
                &reporter.diagnostics,
                "Herdr reporting requires an active Tokio runtime".into(),
            );
            return reporter;
        };
        let (sender, receiver) = watch::channel(None);
        reporter.updates = Some(sender);
        reporter.worker = Some(runtime.spawn(run_reporter(
            config,
            session_id.to_owned(),
            receiver,
            Arc::clone(&reporter.diagnostics),
        )));
        reporter
    }
}

impl HerdrReporter {
    /// Enqueues an update without waiting on socket I/O. Queue storage is bounded
    /// to the latest pending update, not the number of calls to this method.
    pub fn report(&self, state: HerdrState, message: Option<&str>) {
        if let Some(updates) = &self.updates {
            let report = Some(Report {
                state,
                message: message.map(str::to_owned),
            });
            updates.send_if_modified(|current| {
                if *current == report {
                    false
                } else {
                    *current = report;
                    true
                }
            });
        }
    }

    /// Latest transport/protocol failure, retained for diagnostics without stderr.
    pub fn last_error(&self) -> Option<String> {
        self.diagnostics.lock().ok().and_then(|error| error.clone())
    }

    /// Drains the latest pending update and releases registration, in that order.
    /// Returns worker/release failure or the last earlier nonfatal failure,
    /// even if a later retry succeeded. Callers can display this diagnostic
    /// after restoring the terminal without losing errors when consuming self.
    pub async fn shutdown(mut self) -> io::Result<()> {
        self.updates.take();
        if let Some(worker) = self.worker.take() {
            worker.await.map_err(io::Error::other)??;
        }
        match self.last_error() {
            Some(error) => Err(io::Error::other(error)),
            None => Ok(()),
        }
    }
}

fn record_error(diagnostics: &Diagnostics, error: String) {
    if let Ok(mut latest) = diagnostics.lock() {
        *latest = Some(error);
    }
}

async fn run_reporter(
    config: Config,
    session_id: String,
    mut updates: watch::Receiver<Option<Report>>,
    diagnostics: Diagnostics,
) -> io::Result<()> {
    let identity = json!({"pane_id": config.pane_id, "source": SOURCE, "agent": AGENT});
    let mut session = identity.clone();
    session["agent_session_id"] = json!(session_id);
    let mut last_successful_report = None;
    while updates.changed().await.is_ok() {
        // Never hold a watch borrow across socket I/O: report() must stay synchronous.
        let Some(report) = updates.borrow_and_update().clone() else {
            continue;
        };
        if last_successful_report.as_ref() == Some(&report) {
            continue;
        }
        let mut params = session.clone();
        params["state"] = json!(match report.state {
            HerdrState::Idle => "idle",
            HerdrState::Working => "working",
            HerdrState::Blocked => "blocked",
        });
        if let Some(message) = &report.message {
            params["message"] = json!(message);
        }
        match exchange(&config, "pane.report_agent", params, REQUEST_TIMEOUT).await {
            Ok(_) => last_successful_report = Some(report),
            Err(error) => {
                // A failed acknowledgement leaves the host state unknown.
                last_successful_report = None;
                record_error(&diagnostics, error.to_string());
            }
        }
    }
    let result = exchange(&config, "pane.release_agent", identity, REQUEST_TIMEOUT)
        .await
        .map(|_| ());
    if let Err(error) = &result {
        record_error(&diagnostics, error.to_string());
    }
    result
}

async fn exchange(
    config: &Config,
    method: &str,
    params: Value,
    timeout: Duration,
) -> io::Result<Value> {
    let request = json!({"id": format!("{SOURCE}:{}", uuid::Uuid::new_v4()), "method": method, "params": params});
    let mut payload = serde_json::to_vec(&request).map_err(io::Error::other)?;
    payload.push(b'\n');
    let response = exchange_payload(config.socket_path.clone(), payload, timeout).await?;
    let response: Value = serde_json::from_slice(&response).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Herdr {method}: malformed response: {error}"),
        )
    })?;
    if let Some(error) = response.get("error").filter(|error| !error.is_null()) {
        return Err(io::Error::other(format!(
            "Herdr {method}: RPC error: {error}"
        )));
    }
    response.get("result").cloned().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Herdr {method}: response is missing result"),
        )
    })
}

#[cfg(unix)]
async fn exchange_payload(
    socket_path: PathBuf,
    payload: Vec<u8>,
    timeout: Duration,
) -> io::Result<Vec<u8>> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    tokio::time::timeout(timeout, async move {
        let mut stream = tokio::net::UnixStream::connect(socket_path).await?;
        stream.write_all(&payload).await?;
        stream.shutdown().await?;
        let mut reader = BufReader::new(stream).take(MAX_RESPONSE_BYTES + 1);
        let mut response = Vec::new();
        reader.read_until(b'\n', &mut response).await?;
        if response.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("Herdr response byte budget exceeded: limit {MAX_RESPONSE_BYTES}, received at least {}", response.len())));
        }
        if response.is_empty() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Herdr closed without a response"));
        }
        Ok(response)
    }).await.map_err(|_| io::Error::new(io::ErrorKind::TimedOut, format!("Herdr request exceeded timeout budget of {} ms", timeout.as_millis())))?
}

#[cfg(not(unix))]
async fn exchange_payload(
    _socket_path: PathBuf,
    _payload: Vec<u8>,
    _timeout: Duration,
) -> io::Result<Vec<u8>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Herdr socket transport is Unix-only",
    ))
}

#[cfg(test)]
#[path = "herdr_tests.rs"]
mod tests;
