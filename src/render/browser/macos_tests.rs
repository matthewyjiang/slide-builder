use super::*;

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
    let command = sandbox
        .command(Path::new("/usr/bin/true"), &html, &output)
        .unwrap();
    let args: Vec<_> = command.as_std().get_args().collect();
    assert!(args.contains(&std::ffi::OsStr::new(PROFILE)));
    assert!(!PROFILE.contains(&directory.path().to_string_lossy().to_string()));
    fs::remove_file(&output).unwrap();
    std::os::unix::fs::symlink(&html, &output).unwrap();
    assert!(sandbox
        .command(Path::new("/usr/bin/true"), &html, &output)
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
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let _connection = std::net::TcpStream::connect(address).unwrap();
    assert!(
        launch_services_available(),
        "host LaunchServices positive control failed"
    );
    assert!(
        process_arguments_readable(),
        "host process-argument positive control failed"
    );
    let sandbox = Sandbox::probe(Path::new("auto")).unwrap();
    let executable = std::env::current_exe().unwrap();
    let mut command = sandbox.command(&executable, &html, &output).unwrap();
    command
        .args([
            "--exact",
            "render::browser::obscura::tests::macos_sandbox_probe_child",
            "--nocapture",
        ])
        .env("SLIDE_BUILDER_TEST_INPUT", fs::canonicalize(&html).unwrap())
        .env(
            "SLIDE_BUILDER_TEST_OUTPUT",
            fs::canonicalize(&output).unwrap(),
        )
        .env(
            "SLIDE_BUILDER_TEST_SECRET_PATH",
            fs::canonicalize(&secret).unwrap(),
        )
        .env("SLIDE_BUILDER_TEST_ADDRESS", address.to_string());
    crate::render::browser::process::run(command, CaptureOptions::default().timeout)
        .await
        .unwrap();
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
    assert!(fs::read(&secret).is_err());
    assert!(fs::read("/etc/passwd").is_err());
    assert!(fs::read_dir(secret.parent().unwrap()).is_err());
    assert!(fs::write(&secret, "overwrite").is_err());
    assert!(fs::write(output.with_file_name("extra"), "escape").is_err());
    assert!(
        std::net::TcpStream::connect(std::env::var("SLIDE_BUILDER_TEST_ADDRESS").unwrap()).is_err()
    );
    assert!(std::process::Command::new("/bin/sh")
        .args(["-c", "exit 0"])
        .status()
        .is_err());
    assert!(std::env::var_os("HOME").is_none());
    assert!(std::env::var_os("PATH").is_none());
    assert!(
        !launch_services_available(),
        "sandbox unexpectedly obtained a LaunchServices port"
    );
    assert!(
        !process_arguments_readable(),
        "sandbox unexpectedly allowed process-argument sysctl"
    );
    fs::write(output, "isolation passed").unwrap();
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

fn process_arguments_readable() -> bool {
    let mut mib = [
        libc::CTL_KERN,
        libc::KERN_PROCARGS2,
        std::process::id() as i32,
    ];
    let mut size = 0;
    unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) == 0
    }
}
