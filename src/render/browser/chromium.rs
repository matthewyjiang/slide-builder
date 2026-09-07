//! Chromium CLI adapter and platform-specific executable discovery.
use super::{percent_encode_path, CaptureOptions};
use anyhow::Result;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub(super) const CANDIDATES: &[&str] = &[
    "google-chrome-stable",
    "google-chrome",
    "chromium",
    "chromium-browser",
    "brave-browser",
    "microsoft-edge-stable",
    "microsoft-edge",
];

pub(super) fn discover(configured: &Path) -> Result<PathBuf> {
    // An explicit path is authoritative, including when it is invalid.
    let result = super::executable_path(configured, CANDIDATES);
    if configured != Path::new("auto") || result.is_ok() {
        return result;
    }
    #[cfg(target_os = "macos")]
    {
        let roots = std::iter::once(PathBuf::from("/Applications"))
            .chain(crate::paths::home_dir().map(|home| home.join("Applications")));
        if let Some(path) = discover_bundles(roots) {
            return Ok(path);
        }
    }
    result
}

#[cfg(any(target_os = "macos", test))]
pub(super) fn discover_bundles(roots: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    const BUNDLES: &[&str] = &[
        "Google Chrome.app/Contents/MacOS/Google Chrome",
        "Chromium.app/Contents/MacOS/Chromium",
        "Brave Browser.app/Contents/MacOS/Brave Browser",
        "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    ];
    roots.into_iter().find_map(|root| {
        BUNDLES
            .iter()
            .find_map(|bundle| super::validate_executable(&root.join(bundle)).ok())
    })
}

pub(super) fn capture_args(
    html: &Path,
    output: &Path,
    profile: &Path,
    options: &CaptureOptions,
) -> Result<Vec<OsString>> {
    let url = format!("file://{}", percent_encode_path(html)?);
    let mut args = vec![
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
    ];
    if cfg!(target_os = "macos") {
        // Use software capture on headless Macs. Native CI reported display-link
        // errors and failed to terminate after painting with GPU rendering.
        args.push("--disable-gpu".into());
    }
    args.push(url.into());
    Ok(args)
}
