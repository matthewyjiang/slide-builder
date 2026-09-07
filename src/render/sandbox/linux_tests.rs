use super::*;
use crate::render::process;
use std::fs;
use std::os::unix::fs::symlink;
use std::time::Duration;

#[test]
fn sandbox_does_not_bind_parent_directories_or_reuse_output_symlinks() {
    let directory = tempfile::tempdir().unwrap();
    let html = directory.path().join("input.html");
    let output = directory.path().join("output.png");
    fs::write(&html, "capture").unwrap();
    let sandbox = Sandbox::probe(Path::new("/bin/true")).unwrap();
    let launch = sandbox
        .command(Request {
            executable: Path::new("/bin/true"),
            input: &html,
            output: &output,
        })
        .unwrap();
    let args: Vec<_> = launch.command.as_std().get_args().collect();
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
        .command(Request {
            executable: Path::new("/bin/true"),
            input: &html,
            output: &output
        })
        .is_err());
    assert_eq!(fs::read_to_string(html).unwrap(), "capture");
}

#[tokio::test]
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
    let sandbox = Sandbox::probe(Path::new("auto")).unwrap();
    let mut launch = sandbox
        .command(Request {
            executable: Path::new("/usr/bin/python3"),
            input: &html,
            output: &output,
        })
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
    launch
        .command
        .env("SLIDE_BUILDER_TEST_SECRET", "must-not-reach-renderer")
        .args(["-c", &script]);
    process::run(launch.command, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(fs::read_to_string(output).unwrap(), "isolation passed");
    assert_eq!(fs::read_to_string(secret).unwrap(), "not permitted");
    assert!(!directory.path().join("extra").exists());
}
