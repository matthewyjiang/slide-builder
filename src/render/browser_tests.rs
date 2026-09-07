use super::*;
use crate::render::cache::CacheKey;
#[cfg(target_os = "linux")]
use std::os::unix::fs::symlink;

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
fn missing_sandbox_fails_closed_and_scale_is_validated() {
    let config = RenderConfig {
        engine: RenderEngine::Obscura,
        sandbox_path: "/missing/slide-builder-bwrap".into(),
        ..RenderConfig::default()
    };
    let error = Browser::probe(&config).unwrap_err();
    assert!(format!("{error:#}").contains("Unsandboxed rendering is not allowed"));
    let mut browser = Browser::from_path(Path::new("/usr/bin/true")).unwrap();
    browser.engine = Engine::Obscura(obscura::Sandbox::probe(Path::new("/usr/bin/true")).unwrap());
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
    browser.engine = Engine::Obscura(obscura::Sandbox::probe(Path::new("/usr/bin/true")).unwrap());
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

#[test]
#[cfg(target_os = "linux")]
fn sandbox_does_not_bind_parent_directories_or_reuse_output_symlinks() {
    let directory = tempfile::tempdir().unwrap();
    let html = directory.path().join("input.html");
    let output = directory.path().join("output.png");
    fs::write(&html, "capture").unwrap();
    let sandbox = obscura::Sandbox::probe(Path::new("/bin/true")).unwrap();
    let command = sandbox
        .command(Path::new("/bin/true"), &html, &output)
        .unwrap();
    let args: Vec<_> = command.as_std().get_args().collect();
    assert!(!args.contains(&directory.path().as_os_str()));
    for required in [
        "--unshare-all",
        "--unshare-user",
        "--disable-userns",
        "--clearenv",
        "--die-with-parent",
    ] {
        assert!(args.contains(&std::ffi::OsStr::new(required)));
    }
    fs::remove_file(&output).unwrap();
    symlink(&html, &output).unwrap();
    assert!(sandbox
        .command(Path::new("/bin/true"), &html, &output,)
        .is_err());
    assert_eq!(fs::read_to_string(html).unwrap(), "capture");
}

#[tokio::test]
async fn process_failure_and_pipe_lifetime_are_bounded() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf renderer-failure >&2; exit 7"]);
    let error = process::run(command, Duration::from_secs(1))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("renderer-failure"));
    let mut command = Command::new("/bin/sh");
    // A background descendant keeps inherited pipes open after the parent exits.
    command.args(["-c", "echo awaiting-helper >&2; sleep 60 & exit 0"]);
    let error = process::run(command, Duration::from_millis(100))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("timed out after 100 ms"));
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

#[tokio::test]
#[cfg(target_os = "linux")]
#[ignore = "requires Linux user namespaces, bubblewrap and /usr/bin/python3"]
async fn sandbox_blocks_host_files_and_network() {
    let directory = tempfile::tempdir().unwrap();
    let html = directory.path().join("capture.html");
    let output = directory.path().join("capture.png");
    let secret = directory.path().join("sentinel.txt");
    fs::write(&html, "permitted input").unwrap();
    fs::write(&secret, "not permitted").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    // Positive control: the host listener is reachable before entering isolation.
    let _connection = std::net::TcpStream::connect(address).unwrap();
    let host_net = fs::read_link("/proc/self/ns/net").unwrap();
    let sandbox = obscura::Sandbox::probe(Path::new("auto")).unwrap();
    let mut command = sandbox
        .command(Path::new("/usr/bin/python3"), &html, &output)
        .unwrap();
    let script = format!(
        r#"
import os, pathlib, socket
assert pathlib.Path('/input/capture.html').read_text() == 'permitted input'
assert not pathlib.Path({secret:?}).exists()
assert not pathlib.Path('/input/sentinel.txt').exists()
assert not pathlib.Path('/etc/passwd').exists()
assert 'SLIDE_BUILDER_TEST_SECRET' not in os.environ
assert os.readlink('/proc/self/ns/net') != {host_net:?}
s = socket.socket()
s.settimeout(1)
try:
    assert s.connect_ex(('127.0.0.1', {port})) != 0
finally:
    s.close()
pathlib.Path('/output/capture.png').write_text('isolation passed')
pathlib.Path('/output/extra').write_text('private tmpfs only')
"#,
        secret = secret.to_string_lossy(),
        host_net = host_net.to_string_lossy(),
        port = address.port()
    );
    command
        .env("SLIDE_BUILDER_TEST_SECRET", "must-not-reach-renderer")
        .args(["-c", &script]);
    process::run(command, CaptureOptions::default().timeout)
        .await
        .unwrap();
    assert_eq!(fs::read_to_string(output).unwrap(), "isolation passed");
    assert_eq!(fs::read_to_string(secret).unwrap(), "not permitted");
    assert!(!directory.path().join("extra").exists());
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
