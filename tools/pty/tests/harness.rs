#![cfg(unix)]
use slide_builder_pty::{IsolatedWorkspace, Key, PtyHarness, PtySize};
use std::{path::Path, time::Duration};

const TIMEOUT: Duration = Duration::from_secs(3);

fn shell(workspace: &IsolatedWorkspace, script: &str) -> PtyHarness {
    PtyHarness::spawn(
        Path::new("/bin/sh"),
        &["-c", script],
        PtySize::new(24, 80),
        &workspace.environment(),
        &workspace.cwd(),
        &workspace.root().join("artifacts"),
    )
    .unwrap()
}

#[test]
fn input_resize_and_visible_screen() {
    let workspace = IsolatedWorkspace::new().unwrap();
    let mut h = shell(&workspace, "stty -echo; printf '\\033[2J\\033[Hready'; read input; printf '\\033[2J\\033[Hreceived:%s\r\n' \"$input\"; stty size; read done");
    h.wait_for_text("ready", TIMEOUT).unwrap();
    h.resize(PtySize::new(30, 70)).unwrap();
    h.send(b"hello").unwrap();
    h.key(Key::Enter).unwrap();
    h.wait_for_text("received:hello", TIMEOUT).unwrap();
    h.wait_for_text("30 70", TIMEOUT).unwrap();
    h.wait_for_text_gone("ready", TIMEOUT).unwrap();
    h.key(Key::Enter).unwrap();
    h.expect_exit(0, TIMEOUT).unwrap();
}

#[test]
fn failures_preserve_artifacts_and_exit_status() {
    let workspace = IsolatedWorkspace::new().unwrap();
    let mut h = shell(&workspace, "printf goodbye; exit 7");
    let error = h.wait_for_text("missing", TIMEOUT).unwrap_err();
    assert!(error.to_string().contains("child exited"));
    h.expect_exit(7, TIMEOUT).unwrap();
    let captures = std::fs::read_dir(workspace.root().join("artifacts")).unwrap();
    let capture = captures.into_iter().next().unwrap().unwrap().path();
    assert_eq!(
        std::fs::read_to_string(capture.join("screen.txt")).unwrap(),
        "goodbye"
    );
    assert_eq!(std::fs::read(capture.join("raw.pty")).unwrap(), b"goodbye");
}

#[test]
fn timeout_is_bounded_and_drop_kills_child() {
    let workspace = IsolatedWorkspace::new().unwrap();
    let mut h = shell(
        &workspace,
        "echo $$ > child.pid; printf ready; exec sleep 30",
    );
    h.wait_for_text("ready", TIMEOUT).unwrap();
    assert!(h
        .wait_for_text("missing", Duration::from_millis(50))
        .unwrap_err()
        .to_string()
        .contains("timeout"));
    let pid: i32 = std::fs::read_to_string(workspace.cwd().join("child.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    drop(h);
    // SAFETY: signal zero checks existence without signaling the process.
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
}

#[test]
fn child_has_isolated_home_and_no_inherited_credentials() {
    let workspace = IsolatedWorkspace::new().unwrap();
    let mut h = shell(
        &workspace,
        "printf '%s\\n' \"$HOME\"; test -z \"$HERDR_ENV$OPENAI_API_KEY$ANTHROPIC_API_KEY\"",
    );
    h.wait_for_text(
        &workspace.root().join("home").display().to_string(),
        TIMEOUT,
    )
    .unwrap();
    h.expect_exit(0, TIMEOUT).unwrap();
}
