//! A host process containing only nonsecret sentinels for kernel-read probes.
use std::io::Read;
use std::process::Stdio;
use tokio::io::AsyncBufReadExt;
use tokio::process::{Child, Command};

const FIXTURE: &str = "render::browser::obscura::tests::procargs::host_fixture";

pub(super) struct HostFixture {
    child: Child,
    pub pid: u32,
    pub argument: String,
    pub environment: String,
}

impl HostFixture {
    pub async fn start() -> Self {
        let argument = format!("nonsecret-argument-{}", uuid::Uuid::new_v4());
        let environment = format!("nonsecret-environment-{}", uuid::Uuid::new_v4());
        let mut child = Command::new(std::env::current_exe().unwrap())
            .env_clear()
            .env("SLIDE_BUILDER_HOST_SENTINEL", &environment)
            .args([
                "--exact",
                FIXTURE,
                "--nocapture",
                "--test-threads=1",
                "--skip",
                &argument,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let pid = child.id().unwrap();
        let mut lines = tokio::io::BufReader::new(child.stdout.as_mut().unwrap()).lines();
        tokio::time::timeout(super::CaptureOptions::default().timeout, async {
            loop {
                let line = lines
                    .next_line()
                    .await
                    .unwrap()
                    .expect("host fixture exited before readiness");
                if line.contains("SLIDE_BUILDER_HOST_READY") {
                    break;
                }
            }
        })
        .await
        .expect("host fixture did not become ready within the capture deadline");
        drop(lines);
        let bytes = read_arguments(pid).unwrap();
        assert!(contains(&bytes, &argument), "host argument control missing");
        assert!(
            contains(&bytes, &environment),
            "host environment control missing"
        );
        Self {
            child,
            pid,
            argument,
            environment,
        }
    }

    pub async fn finish(mut self) {
        drop(self.child.stdin.take());
        assert!(self.child.wait().await.unwrap().success());
    }
}

#[test]
fn host_fixture() {
    if std::env::var_os("SLIDE_BUILDER_HOST_SENTINEL").is_none() {
        return;
    }
    println!("SLIDE_BUILDER_HOST_READY");
    // Keep the fixture alive until the parent closes its private stdin pipe.
    let _ = std::io::stdin().read(&mut [0_u8]);
}

pub(super) fn assert_denied(pid: u32, argument: &str, environment: &str) {
    assert_ne!(
        pid,
        std::process::id(),
        "probe must target the host fixture, not itself"
    );
    match read_arguments(pid) {
        Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied),
        Ok(bytes) => panic!(
            "sandbox copied {} host process bytes; nonsecret argument sentinel exposed: {}; nonsecret environment sentinel exposed: {}",
            bytes.len(), contains(&bytes, argument), contains(&bytes, environment),
        ),
    }
}

fn contains(bytes: &[u8], text: &str) -> bool {
    bytes
        .windows(text.len())
        .any(|part| part == text.as_bytes())
}

fn read_arguments(pid: u32) -> std::io::Result<Vec<u8>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as i32];
    let mut size = 0;
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error());
    }
    // The kernel supplies the allocation size. A size-only query is not evidence
    // that process bytes are accessible; always perform the actual read too.
    let mut bytes = vec![0_u8; size];
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error());
    }
    bytes.truncate(size);
    Ok(bytes)
}
