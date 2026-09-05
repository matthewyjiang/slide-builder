//! The explicitly selected Chromium CLI adapter.
use super::{percent_encode_path, CaptureOptions};
use anyhow::Result;
use std::ffi::OsString;
use std::path::Path;

pub(super) const CANDIDATES: &[&str] = &[
    "google-chrome-stable",
    "google-chrome",
    "chromium",
    "chromium-browser",
    "microsoft-edge-stable",
    "microsoft-edge",
];

pub(super) fn capture_args(
    html: &Path,
    output: &Path,
    profile: &Path,
    options: &CaptureOptions,
) -> Result<Vec<OsString>> {
    let url = format!("file://{}", percent_encode_path(html)?);
    Ok(vec![
        "--headless=new".into(),
        "--hide-scrollbars".into(),
        "--disable-background-networking".into(),
        "--disable-component-update".into(),
        "--disable-default-apps".into(),
        "--disable-domain-reliability".into(),
        "--disable-features=Translate,MediaRouter,OptimizationHints,AutofillServerCommunication"
            .into(),
        "--disable-sync".into(),
        "--metrics-recording-only".into(),
        "--no-first-run".into(),
        "--no-pings".into(),
        "--password-store=basic".into(),
        "--use-mock-keychain".into(),
        // Preserve Chromium's sandbox and do not expose a debugging interface.
        format!("--user-data-dir={}", profile.display()).into(),
        format!("--window-size={},{}", options.width, options.height).into(),
        format!("--force-device-scale-factor={}", options.scale).into(),
        format!("--virtual-time-budget={}", options.timeout.as_millis()).into(),
        format!("--screenshot={}", output.display()).into(),
        url.into(),
    ])
}
