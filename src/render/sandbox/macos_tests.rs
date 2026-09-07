use super::*;
use crate::render::browser::CaptureOptions;
use std::io::{ErrorKind, Write};

#[path = "macos_procargs_tests.rs"]
mod procargs;

#[test]
fn sandbox_parameters_are_not_profile_source_and_output_cannot_be_redirected() {
    let directory = tempfile::Builder::new()
        .prefix("capture \" ) ")
        .tempdir()
        .unwrap();
    let html = directory.path().join("input.html");
    let output = directory.path().join("output.png");
    fs::write(&html, "input").unwrap();
    let sandbox = Sandbox::probe(Path::new("/usr/bin/sandbox-exec")).unwrap();
    let executable = fs::canonicalize("/usr/bin/true").unwrap();
    let launch = sandbox
        .command(Request {
            executable: &executable,
            input: &html,
            output: &output,
        })
        .unwrap();
    let args: Vec<_> = launch
        .command
        .as_std()
        .get_args()
        .map(std::ffi::OsStr::to_owned)
        .collect();
    let parameter = |name: &str, path: &Path| {
        let mut value = std::ffi::OsString::from(format!("{name}="));
        value.push(path);
        value
    };
    assert_eq!(
        args,
        vec![
            "-D".into(),
            parameter("EXECUTABLE", &executable),
            "-D".into(),
            parameter("INPUT", &launch.input),
            "-D".into(),
            parameter("OUTPUT", &launch.output),
            "-p".into(),
            PROFILE.into(),
            executable.into_os_string(),
        ]
    );
    fs::remove_file(&output).unwrap();
    std::os::unix::fs::symlink(&html, &output).unwrap();
    assert!(sandbox
        .command(Request {
            executable: Path::new("/usr/bin/true"),
            input: &html,
            output: &output
        })
        .is_err());
    assert_eq!(fs::read_to_string(html).unwrap(), "input");
}

#[tokio::test]
#[ignore = "requires native macOS sandbox-exec"]
async fn macos_sandbox_blocks_host_files_network_and_services() {
    let directory = tempfile::Builder::new()
        .prefix("capture \" ) ")
        .tempdir()
        .unwrap();
    let html = directory.path().join("input.html");
    let output = directory.path().join("output.png");
    let secret = directory.path().join("credentials");
    fs::write(&html, "permitted input").unwrap();
    fs::write(&secret, "host credential sentinel").unwrap();
    fs::read("/etc/passwd").unwrap();
    fs::read_dir(directory.path()).unwrap();
    fs::write(directory.path().join("extra"), "host write control").unwrap();
    fs::remove_file(directory.path().join("extra")).unwrap();
    assert!(std::process::Command::new("/bin/sh")
        .args(["-c", "exit 0"])
        .status()
        .unwrap()
        .success());
    fork_once().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let _connection = std::net::TcpStream::connect(address).unwrap();
    assert!(
        launch_services_available(),
        "host LaunchServices positive control failed"
    );
    let host = procargs::HostFixture::start().await;
    let sandbox = Sandbox::probe(Path::new("auto")).unwrap();
    // Distinguish a launcher/profile failure from native initialization failures.
    let launch = sandbox
        .command(Request {
            executable: Path::new("/usr/bin/true"),
            input: &html,
            output: &directory.path().join("true-output"),
        })
        .unwrap();
    crate::render::browser::process::run(launch.command, CaptureOptions::default().timeout)
        .await
        .expect("minimal executable failed under sandbox profile");
    let executable = std::env::current_exe().unwrap();
    let mut launch = sandbox
        .command(Request {
            executable: &executable,
            input: &html,
            output: &output,
        })
        .unwrap();
    launch
        .command
        .args([
            "--exact",
            "render::sandbox::platform::tests::macos_sandbox_probe_child",
            "--nocapture",
        ])
        .env("SLIDE_BUILDER_TEST_INPUT", &launch.input)
        .env("SLIDE_BUILDER_TEST_OUTPUT", &launch.output)
        .env(
            "SLIDE_BUILDER_TEST_SECRET_PATH",
            fs::canonicalize(&secret).unwrap(),
        )
        .env("SLIDE_BUILDER_TEST_ADDRESS", address.to_string())
        .env("SLIDE_BUILDER_TEST_FIXTURE_PID", host.pid.to_string())
        .env("SLIDE_BUILDER_TEST_ARGUMENT_SENTINEL", &host.argument)
        .env("SLIDE_BUILDER_TEST_ENVIRONMENT_SENTINEL", &host.environment);
    crate::render::browser::process::run(launch.command, CaptureOptions::default().timeout)
        .await
        .unwrap();
    host.finish().await;
    assert_eq!(fs::read_to_string(output).unwrap(), "isolation passed");
    assert_eq!(
        fs::read_to_string(secret).unwrap(),
        "host credential sentinel"
    );
}

#[test]
fn macos_sandbox_probe_child() {
    let Some(input) = std::env::var_os("SLIDE_BUILDER_TEST_INPUT") else {
        return;
    };
    let output = PathBuf::from(std::env::var_os("SLIDE_BUILDER_TEST_OUTPUT").unwrap());
    let secret = PathBuf::from(std::env::var_os("SLIDE_BUILDER_TEST_SECRET_PATH").unwrap());
    assert_eq!(fs::read_to_string(input).unwrap(), "permitted input");
    assert_eq!(
        fs::read(&secret).unwrap_err().kind(),
        ErrorKind::PermissionDenied
    );
    // dyld's literal root-directory grant must not authorize openat traversal.
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    let root = fs::File::open("/").unwrap();
    let relative =
        std::ffi::CString::new(secret.strip_prefix("/").unwrap().as_os_str().as_bytes()).unwrap();
    let fd = unsafe { libc::openat(root.as_raw_fd(), relative.as_ptr(), libc::O_RDONLY) };
    assert_eq!(fd, -1);
    assert_eq!(
        std::io::Error::last_os_error().kind(),
        ErrorKind::PermissionDenied
    );
    assert_eq!(
        fs::read("/etc/passwd").unwrap_err().kind(),
        ErrorKind::PermissionDenied
    );
    assert_eq!(
        fs::read_dir(secret.parent().unwrap()).unwrap_err().kind(),
        ErrorKind::PermissionDenied
    );
    assert_eq!(
        fs::write(&secret, "overwrite").unwrap_err().kind(),
        ErrorKind::PermissionDenied
    );
    assert_eq!(
        fs::write(output.with_file_name("extra"), "escape")
            .unwrap_err()
            .kind(),
        ErrorKind::PermissionDenied
    );
    assert_eq!(
        std::net::TcpStream::connect(std::env::var("SLIDE_BUILDER_TEST_ADDRESS").unwrap())
            .unwrap_err()
            .kind(),
        ErrorKind::PermissionDenied
    );
    assert_eq!(fork_once().unwrap_err().kind(), ErrorKind::PermissionDenied);
    // After process-fork is denied, Rust's spawn fallback cannot inspect the
    // executable and returns ENOENT. The host executed this exact /bin/sh above;
    // native CI also records process-fork and /bin/sh metadata denials.
    assert_eq!(
        std::process::Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .status()
            .unwrap_err()
            .kind(),
        ErrorKind::NotFound
    );
    assert!(std::env::var_os("HOME").is_none());
    assert!(std::env::var_os("PATH").is_none());
    assert!(
        !launch_services_available(),
        "sandbox unexpectedly obtained a LaunchServices port"
    );
    procargs::assert_denied(
        std::env::var("SLIDE_BUILDER_TEST_FIXTURE_PID")
            .unwrap()
            .parse()
            .unwrap(),
        &std::env::var("SLIDE_BUILDER_TEST_ARGUMENT_SENTINEL").unwrap(),
        &std::env::var("SLIDE_BUILDER_TEST_ENVIRONMENT_SENTINEL").unwrap(),
    );
    OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(output)
        .unwrap()
        .write_all(b"isolation passed")
        .unwrap();
}

// LaunchServices can launch unsandboxed applications on a caller's behalf.
// The worker has no reason to obtain this or any other Mach service port.
fn launch_services_available() -> bool {
    unsafe extern "C" {
        static bootstrap_port: u32;
        static mach_task_self_: u32;
        fn mach_port_deallocate(task: u32, port: u32) -> i32;
        fn bootstrap_look_up(port: u32, name: *const std::ffi::c_char, service: *mut u32) -> i32;
    }
    let mut service = 0;
    let result = unsafe {
        bootstrap_look_up(
            bootstrap_port,
            c"com.apple.coreservices.launchservicesd".as_ptr(),
            &mut service,
        )
    };
    if result == 0 {
        unsafe {
            mach_port_deallocate(mach_task_self_, service);
        }
    }
    result == 0
}

fn fork_once() -> std::io::Result<()> {
    // The forked child runs only async-signal-safe _exit, never Rust cleanup.
    let pid = unsafe { libc::fork() };
    if pid == -1 {
        return Err(std::io::Error::last_os_error());
    }
    if pid == 0 {
        unsafe { libc::_exit(0) };
    }
    loop {
        if unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) } == pid {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != ErrorKind::Interrupted {
            return Err(error);
        }
    }
}
