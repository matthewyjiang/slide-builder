use super::*;
use crate::render::cache::CacheKey;
use crate::render::process;
use std::os::unix::fs::PermissionsExt;

#[test]
fn chromium_arguments_preserve_sandbox_and_paths() {
    let args = chromium::capture_args(
        Path::new("/tmp/a b.html"),
        Path::new("/tmp/o.png"),
        Path::new("/tmp/profile"),
        &CaptureOptions::default(),
    )
    .unwrap();
    let args: Vec<_> = args.iter().map(|arg| arg.to_string_lossy()).collect();
    assert!(!args
        .iter()
        .any(|arg| arg.contains("--no-sandbox") || arg.contains("remote-debugging")));
    assert!(args.iter().any(|arg| arg == "file:///tmp/a%20b.html"));
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn embedded_capture_arguments_use_sandbox_visible_paths() {
    let directory = tempfile::Builder::new()
        .prefix("capture spaces ")
        .tempdir()
        .unwrap();
    let html = directory.path().join("input.html");
    let output = directory.path().join("output.png");
    fs::write(&html, "capture").unwrap();
    let executable = Path::new("/usr/bin/true");
    let sandbox = Sandbox::probe(executable).unwrap();
    let options = CaptureOptions {
        width: 640,
        height: 360,
        scale: 2.0,
        timeout: Duration::from_millis(1500),
    };
    let command = capture_command(&sandbox, executable, &html, &output, &options).unwrap();
    let args: Vec<_> = command
        .as_std()
        .get_args()
        .map(std::ffi::OsStr::to_owned)
        .collect();
    let worker = args
        .iter()
        .position(|arg| arg == super::super::worker::WORKER_ARGUMENT)
        .unwrap();
    #[cfg(target_os = "linux")]
    let (input, output) = (
        PathBuf::from("/input/capture.html"),
        PathBuf::from("/output/capture.png"),
    );
    #[cfg(target_os = "macos")]
    let (input, output) = (
        fs::canonicalize(html).unwrap(),
        fs::canonicalize(output).unwrap(),
    );
    assert_eq!(
        args[worker + 1..],
        [
            "640".into(),
            "360".into(),
            "2".into(),
            "1500".into(),
            input.into_os_string(),
            output.into_os_string(),
        ]
    );
}

#[test]
fn missing_sandbox_fails_closed_and_scale_is_validated() {
    let config = RenderConfig {
        engine: RenderEngine::Obscura,
        sandbox_path: "/missing/slide-builder-bwrap".into(),
        ..RenderConfig::default()
    };
    let error = Browser::probe(&config).unwrap_err();
    assert!(format!("{error:#}").contains("Unsandboxed rendering is not allowed"));
    let mut browser = Browser::from_path(Path::new("/usr/bin/true")).unwrap();
    browser.engine = Engine::Obscura(Sandbox::probe(Path::new("/usr/bin/true")).unwrap());
    assert!(browser.validate_options(&CaptureOptions::default()).is_ok());
    let scaled = CaptureOptions {
        scale: 2.0,
        ..CaptureOptions::default()
    };
    browser.validate_options(&scaled).unwrap();
    let invalid = CaptureOptions {
        scale: f32::NAN,
        ..CaptureOptions::default()
    };
    assert!(browser.validate_options(&invalid).is_err());
}

#[test]
fn renderer_identity_separates_cache_entries() {
    let key = CacheKey::new(b"deck", "handler", "capture", 1280, 720, 1.0).unwrap();
    let old = key.clone().with_renderer("obscura-binary-one");
    assert_ne!(
        old.digest,
        key.clone().with_renderer("chromium-binary-one").digest
    );
    assert_ne!(
        old.digest,
        key.clone().with_renderer("obscura-binary-two").digest
    );
    assert_eq!(old, key.with_renderer("obscura-binary-one"));
}

#[tokio::test]
async fn capture_budgets_fail_before_creating_output_or_launching_worker() {
    let directory = tempfile::tempdir().unwrap();
    let mut browser = Browser::from_path(Path::new("/usr/bin/true")).unwrap();
    browser.engine = Engine::Obscura(Sandbox::probe(Path::new("/usr/bin/true")).unwrap());
    for (width, height, scale, asked) in [
        (4096, 2304, 2.0, "8192x4608 output pixels"),
        (16384, 1, 4.0, "65536x4 output pixels"),
        (8192, 4096, 0.25, "8192x4096 CSS pixels"),
    ] {
        let options = CaptureOptions {
            width,
            height,
            scale,
            ..Default::default()
        };
        let output = directory.path().join("output.png");
        let error = browser
            .capture(
                &directory.path().join("missing.html"),
                &output,
                &directory.path().join("profile"),
                &options,
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains(asked), "{error}");
        assert!(
            error.contains("32768") && error.contains("16777216"),
            "{error}"
        );
        assert!(!output.exists());
        options.validate_for_engine(RenderEngine::Chromium).unwrap();
    }
    for scale in [0.0, 0.24, 4.01, f32::NAN, f32::INFINITY] {
        let options = CaptureOptions {
            scale,
            ..Default::default()
        };
        for engine in [RenderEngine::Obscura, RenderEngine::Chromium] {
            assert!(options
                .validate_for_engine(engine)
                .unwrap_err()
                .to_string()
                .contains("between 0.25 and 4.0"));
        }
    }
}

#[test]
fn replacing_an_executable_invalidates_its_cache_identity() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("renderer");
    fs::write(&path, "first").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    let first = Browser::from_path(&path).unwrap();
    let replacement = directory.path().join("replacement");
    fs::write(&replacement, "other").unwrap();
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755)).unwrap();
    fs::rename(replacement, &path).unwrap();
    let second = Browser::from_path(&path).unwrap();
    assert_ne!(first.cache_identity(), second.cache_identity());
}

#[tokio::test]
async fn process_failure_preserves_stderr() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf renderer-failure >&2; exit 7"]);
    let error = process::run(command, Duration::from_secs(1))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("renderer-failure"));
}

#[tokio::test]
async fn process_deadline_is_bounded() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "sleep 60"]);
    let error = process::run(command, Duration::from_millis(100))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("timed out after 100 ms"));
}

#[tokio::test]
async fn process_timeout_preserves_diagnostics_when_helpers_hold_pipes() {
    let mut command = Command::new("/bin/sh");
    // A background descendant keeps inherited pipes open after the parent exits.
    command.args(["-c", "echo awaiting-helper >&2; sleep 60 & exit 0"]);
    // Allow five seconds for shell startup and exit on loaded CI runners. The
    // separate deadline test exercises a short timeout without requiring progress.
    let error = process::run(command, Duration::from_secs(5))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("timed out after 5000 ms"));
    assert!(error
        .to_string()
        .contains("helper output pipes remained open"));
    assert!(error.to_string().contains("awaiting-helper"));
}

#[tokio::test]
async fn cancelling_capture_kills_parent_and_helpers() {
    let directory = tempfile::tempdir().unwrap();
    let ready = directory.path().join("pids");
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "echo $$ > \"$1\"; sleep 60 & echo $! >> \"$1\"; wait",
            "capture-test",
        ])
        .arg(&ready);
    let task = tokio::spawn(process::run(command, CaptureOptions::default().timeout));
    // Wait for an explicit ready signal, not an assumed process startup delay.
    let pids = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(text) = tokio::fs::read_to_string(&ready).await {
                let pids: Vec<u32> = text.lines().filter_map(|line| line.parse().ok()).collect();
                if pids.len() == 2 {
                    break pids;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("capture helpers did not report ready within 5 seconds");
    assert!(pids.iter().all(|pid| process_is_running(*pid)));
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let running = pids.iter().any(|pid| process_is_running(*pid));
            if !running {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled capture left a parent or helper running after 5 seconds");
}

fn process_is_running(pid: u32) -> bool {
    // ps works on Linux and macOS and distinguishes orphaned zombies from
    // live helpers. Missing /proc must never count as successful cancellation.
    let output = std::process::Command::new("/bin/ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .expect("inspect capture process with ps");
    assert!(output.status.success() || output.status.code() == Some(1));
    let status = std::str::from_utf8(&output.stdout).unwrap().trim();
    !status.is_empty() && !status.starts_with(['Z', 'X'])
}
