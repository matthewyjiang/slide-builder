//! Exercise the real workspace loop against an isolated Herdr protocol peer.
#![cfg(target_os = "linux")]

use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    os::{
        fd::FromRawFd,
        unix::{net::UnixListener, process::CommandExt},
    },
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

struct TerminalChild {
    child: Child,
    master: File,
}

// Ratatui's diff renderer emits cursor moves instead of unchanged spaces.
fn terminal_words(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut chars = text.chars().peekable();
    let mut visible = String::new();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for code in chars.by_ref() {
                if ('@'..='~').contains(&code) {
                    break;
                }
            }
        } else if !ch.is_whitespace() {
            visible.push(ch);
        }
    }
    visible
}

impl Drop for TerminalChild {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

fn spawn_terminal(command: &mut Command, width: u16) -> TerminalChild {
    let mut master = -1;
    let mut slave = -1;
    let size = libc::winsize {
        ws_row: 35,
        ws_col: width,
        ws_xpixel: 880,
        ws_ypixel: 560,
    };
    // SAFETY: openpty writes two owned descriptors and reads the initialized size.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        },
        0
    );
    // SAFETY: each descriptor returned by openpty is transferred exactly once.
    let master = unsafe { File::from_raw_fd(master) };
    let slave = unsafe { File::from_raw_fd(slave) };
    command
        .stdin(Stdio::from(slave.try_clone().unwrap()))
        .stdout(Stdio::from(slave.try_clone().unwrap()))
        .stderr(Stdio::from(slave));
    // SAFETY: only async-signal-safe libc calls execute between fork and exec.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 || libc::ioctl(libc::STDIN_FILENO, libc::TIOCSCTTY, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    TerminalChild {
        child: command.spawn().unwrap(),
        master,
    }
}

#[test]
fn native_reports_follow_workspace_dialog_and_shutdown() {
    exercise_workspace_connections(110);
}

#[test]
fn connection_flows_return_to_a_narrow_workspace() {
    exercise_workspace_connections(48);
}

fn exercise_workspace_connections(width: u16) {
    let home = tempfile::tempdir().unwrap();
    let config_dir = home.path().join("config/slide-builder");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("config.toml"),
        "provider = \"anthropic\"\nmodel = \"claude-sonnet-4-20250514\"\n[preview]\nenabled = false\nprotocol = \"auto\"\n").unwrap();
    let socket_path = home.path().join("herdr.sock");
    let listener = UnixListener::bind(&socket_path).unwrap();
    let (requests, received) = mpsc::channel();
    let server = std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            // Generous test tripwire compared with the client's 500 ms request deadline.
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            let response = if request["method"] == "pane.graphics.info" {
                json!({"id": request["id"], "error": {"code": "feature_disabled", "message": "test fallback"}})
            } else {
                json!({"id": request["id"], "result": {"type": "ok"}})
            };
            writeln!(stream, "{response}").unwrap();
            let release = request["method"] == "pane.release_agent";
            if requests.send(request).is_err() || release {
                break;
            }
        }
    });
    let mut command = Command::new(env!("CARGO_BIN_EXE_slide-builder"));
    command
        .arg(home.path().join("deck.pptx"))
        .current_dir(home.path())
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("XDG_CACHE_HOME", home.path().join("cache"))
        .env("ANTHROPIC_API_KEY", "test-only-not-a-real-key")
        .env("TERM", "xterm-256color")
        .env("HERDR_ENV", "1")
        .env("HERDR_SOCKET_PATH", &socket_path)
        .env("HERDR_PANE_ID", "test:p1");
    let mut terminal = spawn_terminal(&mut command, width);
    let mut reader = terminal.master.try_clone().unwrap();
    let (output_tx, output_rx) = mpsc::channel();
    let output = std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut buffer = [0; 8192];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 {
                break;
            }
            output.extend_from_slice(&buffer[..count]);
            let _ = output_tx.send(buffer[..count].to_vec());
        }
        output
    });
    let mut seen = Vec::new();
    let mut wait = |method: &str, state: Option<&str>| loop {
        let request = received
            .recv_timeout(Duration::from_secs(10))
            .expect("TUI did not report within 10 seconds; check startup/configuration");
        let matched = request["method"] == method
            && state.is_none_or(|state| request["params"]["state"] == state);
        seen.push(request);
        if matched {
            break;
        }
    };
    wait("pane.report_agent", Some("idle"));
    let wait_for_any_text = |texts: &[&str]| {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut captured = Vec::new();
        loop {
            let chunk = output_rx
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .unwrap_or_else(|_| {
                    panic!(
                        "TUI did not display {texts:?}: {}",
                        String::from_utf8_lossy(&captured)
                    )
                });
            captured.extend(chunk);
            if let Some(index) = texts.iter().position(|text| {
                terminal_words(&captured).contains(&text.split_whitespace().collect::<String>())
            }) {
                break index;
            }
        }
    };
    let wait_for_text = |text: &str| {
        wait_for_any_text(&[text]);
    };
    terminal.master.write_all(b"/login\r").unwrap();
    wait_for_text("Connect a provider");
    // Select a known API-key provider without relying on registry ordering.
    let openai = rho_providers::provider::providers()
        .iter()
        .position(|provider| provider.name == "openai")
        .unwrap();
    for _ in 0..openai {
        terminal.master.write_all(b"\x1b[B").unwrap();
    }
    terminal.master.write_all(b"\r").unwrap();
    wait_for_text("Authentication");
    terminal.master.write_all(b"\x1b").unwrap();
    wait_for_text("Connect a provider");
    terminal.master.write_all(b"\x1b").unwrap();
    wait_for_text("SLIDE BUILDER");
    terminal.master.write_all(b"/logout\r").unwrap();
    // The OS keyring is not isolated by HOME. Never select or delete a real credential.
    if wait_for_any_text(&[
        "Disconnect a provider",
        "No saved connections",
        "Could not update provider connection",
    ]) == 0
    {
        terminal.master.write_all(b"\x1b").unwrap();
        wait_for_text("SLIDE BUILDER");
    }
    terminal.master.write_all(b"/help\r").unwrap();
    wait("pane.report_agent", Some("blocked"));
    terminal.master.write_all(b"\x1b").unwrap();
    wait("pane.report_agent", Some("idle"));
    terminal.master.write_all(b"/quit\r").unwrap();
    wait("pane.release_agent", None);
    let exit = terminal.child.wait().unwrap();
    // Command retains its configured slave descriptors until dropped.
    drop(command);
    let output = output.join().unwrap();
    assert!(exit.success(), "{}", String::from_utf8_lossy(&output));
    server.join().unwrap();
    assert_eq!(seen[0]["method"], "pane.graphics.info");
    let reports: Vec<_> = seen
        .iter()
        .filter(|request| request["method"] == "pane.report_agent")
        .collect();
    let session = &reports[0]["params"]["agent_session_id"];
    assert!(!session.as_str().unwrap().is_empty());
    for report in reports {
        let params = &report["params"];
        assert_eq!(
            (
                params["agent"].as_str(),
                params["source"].as_str(),
                params["pane_id"].as_str()
            ),
            (
                Some("slide-builder"),
                Some("herdr:slide-builder"),
                Some("test:p1")
            )
        );
        assert_eq!(&params["agent_session_id"], session);
    }
}
