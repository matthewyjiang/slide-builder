use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};

use crate::{
    keys::{encode_key, encode_paste},
    pty::PtyController,
    Key, PtySize,
};

/// Controls a real child terminal and reconstructs its visible screen.
/// Wait and exit failures automatically save diagnostics to a unique directory.
pub struct PtyHarness {
    pty: PtyController,
    parser: vt100::Parser,
    raw: Vec<u8>,
    actions: Vec<String>,
    artifacts: PathBuf,
}

impl PtyHarness {
    pub fn spawn(
        binary: &Path,
        args: &[&str],
        size: PtySize,
        env: &[(String, String)],
        cwd: &Path,
        artifacts: &Path,
    ) -> Result<Self> {
        anyhow::ensure!(
            size.rows > 0 && size.cols > 0,
            "terminal dimensions must be nonzero"
        );
        Ok(Self {
            pty: PtyController::spawn(binary, size, args, env, Some(cwd))?,
            parser: vt100::Parser::new(size.rows, size.cols, /*scrollback_len*/ 0),
            raw: Vec::new(),
            actions: vec![format!("spawn {} {args:?} {size:?}", binary.display())],
            artifacts: artifacts.into(),
        })
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    pub fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.actions.push(format!("send {bytes:?}"));
        self.pty.inject_bytes(bytes)
    }

    pub fn key(&mut self, key: Key) -> Result<()> {
        self.send(&encode_key(&key))
    }

    pub fn paste(&mut self, text: &str) -> Result<()> {
        self.send(&encode_paste(text))
    }

    pub fn resize(&mut self, size: PtySize) -> Result<()> {
        anyhow::ensure!(
            size.rows > 0 && size.cols > 0,
            "terminal dimensions must be nonzero"
        );
        self.actions.push(format!("resize {size:?}"));
        self.pty.resize(size.rows, size.cols)?;
        self.parser.screen_mut().set_size(size.rows, size.cols);
        Ok(())
    }

    pub fn poll(&mut self, budget: Duration) {
        let bytes = self.pty.drain(budget);
        self.parser.process(&bytes);
        self.raw.extend(bytes);
    }

    pub fn wait_for_text(&mut self, text: &str, timeout: Duration) -> Result<()> {
        self.wait_until(&format!("text {text:?}"), timeout, |screen| {
            screen.contents().contains(text)
        })
    }

    pub fn wait_for_text_gone(&mut self, text: &str, timeout: Duration) -> Result<()> {
        self.wait_until(&format!("text {text:?} gone"), timeout, |screen| {
            !screen.contents().contains(text)
        })
    }

    pub fn wait_until(
        &mut self,
        label: &str,
        timeout: Duration,
        predicate: impl Fn(&vt100::Screen) -> bool,
    ) -> Result<()> {
        self.actions.push(format!("wait {label}"));
        let deadline = Instant::now() + timeout;
        loop {
            self.poll(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(25)),
            );
            if predicate(self.screen()) {
                return Ok(());
            }
            if !self.pty.is_running() {
                self.poll(Duration::from_millis(50));
                if predicate(self.screen()) {
                    return Ok(());
                }
                return self.fail(&format!("child exited waiting for {label}"));
            }
            if Instant::now() >= deadline {
                return self.fail(&format!("timeout waiting for {label}"));
            }
        }
    }

    pub fn expect_exit(&mut self, code: u32, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            // Drain while waiting so a child cannot block on a full output pipe.
            self.poll(Duration::from_millis(10));
            if let Some(actual) = self.pty.wait_exit(Duration::ZERO)? {
                self.poll(Duration::from_millis(50));
                if actual == code {
                    return Ok(());
                }
                return self.fail(&format!("expected exit {code}, got {actual}"));
            }
            if Instant::now() >= deadline {
                return self.fail("timeout waiting for exit");
            }
        }
    }

    /// Save raw ANSI output, visible text, and input history without environment secrets.
    pub fn capture(&self, label: &str) -> Result<PathBuf> {
        fs::create_dir_all(&self.artifacts)?;
        let dir = tempfile::Builder::new()
            .prefix("capture-")
            .tempdir_in(&self.artifacts)?
            .keep();
        fs::write(dir.join("raw.pty"), &self.raw)?;
        fs::write(dir.join("screen.txt"), self.screen().contents())?;
        fs::write(dir.join("screen.ansi"), self.screen().contents_formatted())?;
        fs::write(dir.join("actions.log"), self.actions.join("\n"))?;
        fs::write(
            dir.join("report.txt"),
            format!(
                "{label}\nsize: {:?}\ncursor: {:?}\n",
                self.screen().size(),
                self.screen().cursor_position()
            ),
        )?;
        Ok(dir)
    }

    fn fail(&self, message: &str) -> Result<()> {
        let path = self
            .capture(message)
            .context("saving PTY failure artifacts")?;
        bail!(
            "{message}; artifacts: {}\n{}",
            path.display(),
            self.screen().contents()
        )
    }
}
