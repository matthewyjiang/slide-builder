//! Offline HTML-to-PNG rendering pipeline.

use crate::render::browser::{Browser, CaptureOptions};
use crate::render::cache::{sha256_file, CacheKey, RenderCache, RenderManifest, SlideImage};
use anyhow::{bail, Context, Result};
use image::{DynamicImage, ImageReader, Rgba};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

// Bump when capture HTML/layout changes so stale blank or mis-scaled cache entries
// are not reused.
pub const RENDERER_VERSION: &str = "html-capture-v2";
pub const HANDLER_REVISION: &str = "officecli-acabe4959a37235dd587bbcc788565f19a824bb7";
const CAPTURE_ATTEMPTS: u32 = 3;

/// Count the top-level slide containers emitted by the pinned pptx-handler.
/// Keeping this selector contract beside capture injection makes dependency
/// drift fail clearly in tests instead of producing blank previews.
pub fn handler_slide_count(source: &str) -> u32 {
    source
        .match_indices("class=\"slide-container\" data-slide=\"")
        .count() as u32
}

#[derive(Clone)]
pub struct BrowserPipeline {
    browser: Browser,
    cache: RenderCache,
    options: CaptureOptions,
    max_concurrency: usize,
}

impl BrowserPipeline {
    pub fn new(
        browser: Browser,
        cache: RenderCache,
        options: CaptureOptions,
        max_concurrency: usize,
    ) -> Result<Self> {
        if max_concurrency == 0 || max_concurrency > 32 {
            bail!("render concurrency must be between 1 and 32");
        }
        Ok(Self {
            browser,
            cache,
            options,
            max_concurrency,
        })
    }

    /// Render a handler-produced HTML document. The entry is immutable and its
    /// manifest appears only after every PNG has been decoded and checksummed.
    pub async fn render(
        &self,
        generation: u64,
        deck_identity: &[u8],
        key: CacheKey,
        source_html: &str,
        slide_count: u32,
    ) -> Result<(PathBuf, RenderManifest)> {
        if slide_count == 0 || slide_count > 10_000 {
            bail!("slide count must be between 1 and 10000");
        }
        let final_dir = self.cache.entry_dir(deck_identity, &key);
        if let Some(mut manifest) = self.cache.load(&final_dir)? {
            if manifest.cache_key == key {
                // Generation is request state rather than cache identity. Reusing
                // identical deck bytes must still satisfy a newer Ctrl+R request.
                manifest.generation = generation;
                return Ok((final_dir, manifest));
            }
        }
        let parent = final_dir.parent().context("cache entry has no parent")?;
        fs::create_dir_all(parent)?;
        let staging = parent.join(format!(".tmp-{}-{}", std::process::id(), generation));
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        fs::create_dir(&staging)?;

        let result = self.render_into(&staging, source_html, slide_count).await;
        let slides = match result {
            Ok(slides) => slides,
            Err(error) => {
                let _ = fs::remove_dir_all(&staging);
                return Err(error);
            }
        };
        let manifest = RenderManifest::new(generation, key, slides)?;
        self.cache.publish(&staging, &manifest)?;
        match fs::rename(&staging, &final_dir) {
            Ok(()) => {}
            Err(_error) if final_dir.join("manifest.json").is_file() => {
                let _ = fs::remove_dir_all(&staging);
                let existing = self
                    .cache
                    .load(&final_dir)?
                    .context("concurrent cache entry is invalid")?;
                return Ok((final_dir, existing));
            }
            Err(error) => return Err(error).context("publish render cache directory"),
        }
        Ok((final_dir, manifest))
    }

    async fn render_into(
        &self,
        directory: &Path,
        source_html: &str,
        slide_count: u32,
    ) -> Result<Vec<SlideImage>> {
        // Per-slide HTML is sanitized and validated by build_capture_html.
        let semaphore = Arc::new(Semaphore::new(self.max_concurrency));
        let mut jobs = JoinSet::new();
        for index in 1..=slide_count {
            let permit = semaphore.clone().acquire_owned().await?;
            let html = build_capture_html(source_html, index, &self.options)?;
            let browser = self.browser.clone();
            let options = self.options.clone();
            let directory = directory.to_path_buf();
            jobs.spawn(async move {
                let _permit = permit;
                render_one(browser, options, directory, html, index).await
            });
        }
        let mut slides = Vec::with_capacity(slide_count as usize);
        while let Some(result) = jobs.join_next().await {
            match result {
                Ok(Ok(slide)) => slides.push(slide),
                Ok(Err(error)) => {
                    jobs.abort_all();
                    return Err(error);
                }
                Err(error) => {
                    jobs.abort_all();
                    return Err(error).context("render task failed");
                }
            }
        }
        slides.sort_by_key(|slide| slide.index);
        Ok(slides)
    }
}

async fn render_one(
    browser: Browser,
    options: CaptureOptions,
    directory: PathBuf,
    html: String,
    index: u32,
) -> Result<SlideImage> {
    let capture = directory.join(format!("capture-{index:04}.html"));
    let temporary_png = directory.join(format!(".slide-{index:04}.tmp.png"));
    let final_name = format!("slide-{index:04}.png");
    let final_png = directory.join(&final_name);
    let profile = directory.join(format!("profile-{index:04}"));
    fs::write(&capture, &html)?;

    let mut image = None;
    let mut last_error = None;
    for attempt in 1..=CAPTURE_ATTEMPTS {
        let _ = fs::remove_file(&temporary_png);
        let _ = fs::remove_dir_all(&profile);
        match browser
            .capture(&capture, &temporary_png, &profile, &options)
            .await
        {
            Ok(_) => {}
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        }
        let decoded = match ImageReader::open(&temporary_png)
            .and_then(|reader| reader.with_guessed_format())
            .map_err(|error| anyhow::anyhow!(error))
            .and_then(|reader| {
                if reader.format() != Some(image::ImageFormat::Png) {
                    bail!("slide {index} is not a PNG");
                }
                reader
                    .decode()
                    .with_context(|| format!("decode slide {index}"))
            }) {
            Ok(decoded) => decoded,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        };
        if decoded.width() == 0 || decoded.height() == 0 {
            last_error = Some(anyhow::anyhow!("slide {index} has empty dimensions"));
            continue;
        }
        // Headless Chromium occasionally emits a pure-white frame under load.
        // Retry those instead of caching a blank preview permanently.
        if attempt < CAPTURE_ATTEMPTS && image_is_blank_capture(&decoded) {
            last_error = Some(anyhow::anyhow!(
                "slide {index} capture was blank on attempt {attempt}"
            ));
            continue;
        }
        image = Some(decoded);
        break;
    }
    let _ = fs::remove_file(&capture);
    let _ = fs::remove_dir_all(&profile);
    let image = match image {
        Some(image) => image,
        None => {
            return Err(last_error
                .unwrap_or_else(|| anyhow::anyhow!("slide {index} capture produced no image")))
            .with_context(|| format!("capture slide {index}"));
        }
    };
    fs::rename(&temporary_png, &final_png)?;
    let sha256 = sha256_file(&final_png)?;
    Ok(SlideImage {
        index,
        file: final_name,
        width: image.width(),
        height: image.height(),
        sha256,
    })
}

/// Reject active/network content and return a capture-only document.
///
/// Scaling is applied with static CSS (not JS). Headless Chromium's
/// `--screenshot` path is racy when scripts measure layout and set
/// `transform: scale(...)` at runtime; under concurrency that often yields a
/// permanent pure-white frame that then gets cached.
pub fn build_capture_html(
    source: &str,
    slide_index: u32,
    options: &CaptureOptions,
) -> Result<String> {
    if slide_index == 0 {
        bail!("slide index is one-based");
    }
    if options.width == 0 || options.height == 0 {
        bail!("capture viewport must be non-zero");
    }
    let (mut html, dynamic_removed) = strip_active_content(source);
    validate_offline_html(&html)?;
    // A base element can redirect otherwise relative URLs. CSP is the primary
    // enforcement layer, while removing it keeps local resolution predictable.
    html = strip_void_tag(&html, "base");
    html = strip_void_tag(&html, "meta");
    let scale = capture_scale_for_html(&html, options.width, options.height);
    let scale_css = format!("{scale:.6}");
    let head = format!(
        r#"<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src 'self' data:; media-src 'none'; font-src 'self' data:; style-src 'unsafe-inline'; script-src 'none'; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'">
<style>
html,body{{margin:0!important;padding:0!important;overflow:hidden!important;background:#ffffff!important}}
body>*:not(.main){{display:none!important}}
.main{{display:block!important;margin:0!important;padding:0!important;overflow:hidden!important;width:100vw!important;height:100vh!important}}
.main>.slide-container{{display:none!important;margin:0!important;padding:0!important}}
.main>.slide-container[data-slide="{slide_index}"]{{display:block!important;margin:0!important;padding:0!important}}
.main>.slide-container[data-slide="{slide_index}"] .slide-wrapper{{display:block!important;margin:0!important;padding:0!important}}
.main>.slide-container[data-slide="{slide_index}"] .slide{{margin:0!important;box-shadow:none!important;border-radius:0!important;transform:scale({scale_css})!important;transform-origin:top left!important}}
.slide-builder-unsupported{{position:fixed;inset:auto 1rem 1rem 1rem;z-index:2147483647;padding:.5rem;background:#fff3cd;color:#5f4700;font:14px sans-serif}}
</style>"#
    );
    if let Some(position) = find_ascii_case_insensitive(&html, "</head>") {
        html.insert_str(position, &head);
    } else if let Some(position) = find_ascii_case_insensitive(&html, "<body") {
        html.insert_str(position, &format!("<head>{head}</head>"));
    } else {
        html.insert_str(0, &format!("<!doctype html><head>{head}</head><body>"));
        html.push_str("</body>");
    }
    // Keep the notice in the body. A div injected into <head> is invalid HTML and
    // can force the parser to reshuffle the document tree before capture.
    if dynamic_removed {
        let banner = "<div class=\"slide-builder-unsupported\">Unsupported dynamic content was removed for offline preview.</div>";
        if let Some(position) = find_body_content_start(&html) {
            html.insert_str(position, banner);
        } else {
            html.push_str(banner);
        }
    }
    Ok(html)
}

/// Fit the handler slide into the capture viewport without runtime JS.
fn capture_scale_for_html(html: &str, viewport_width: u32, viewport_height: u32) -> f64 {
    let Some((design_w, design_h)) = parse_slide_design_px(html) else {
        return 1.0;
    };
    if design_w <= 0.0 || design_h <= 0.0 {
        return 1.0;
    }
    let scale_x = f64::from(viewport_width) / design_w;
    let scale_y = f64::from(viewport_height) / design_h;
    let scale = scale_x.min(scale_y);
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// Read `--slide-design-w/h` from the handler stylesheet as CSS pixels.
fn parse_slide_design_px(html: &str) -> Option<(f64, f64)> {
    Some((
        parse_css_var_length_px(html, "--slide-design-w")?,
        parse_css_var_length_px(html, "--slide-design-h")?,
    ))
}

fn parse_css_var_length_px(html: &str, name: &str) -> Option<f64> {
    let index = html.find(name)?;
    let after = &html[index + name.len()..];
    let after = after.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    let value: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if value.is_empty() {
        return None;
    }
    let number: f64 = value.parse().ok()?;
    let unit = after[value.len()..].trim_start().to_ascii_lowercase();
    if unit.starts_with("pt") {
        // CSS absolute units: 1pt = 1/72in at the 96dpi px reference.
        Some(number * 96.0 / 72.0)
    } else if unit.starts_with("px") {
        Some(number)
    } else {
        None
    }
}

fn find_body_content_start(html: &str) -> Option<usize> {
    let start = find_ascii_case_insensitive(html, "<body")?;
    let after = &html[start..];
    let rel = after.find('>')?;
    Some(start + rel + 1)
}

/// True when every sampled pixel is essentially the same near-white color.
/// Used only as a capture flake detector, not as a content quality score.
fn image_is_blank_capture(image: &DynamicImage) -> bool {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return true;
    }
    let step_x = ((width / 64).max(1)) as usize;
    let step_y = ((height / 64).max(1)) as usize;
    let mut first: Option<Rgba<u8>> = None;
    for y in (0..height as usize).step_by(step_y) {
        for x in (0..width as usize).step_by(step_x) {
            let pixel = *rgba.get_pixel(x as u32, y as u32);
            if !is_near_white(pixel) {
                return false;
            }
            match first {
                None => first = Some(pixel),
                Some(origin) if !pixels_close(origin, pixel) => return false,
                Some(_) => {}
            }
        }
    }
    true
}

fn is_near_white(pixel: Rgba<u8>) -> bool {
    pixel[0] >= 250 && pixel[1] >= 250 && pixel[2] >= 250 && pixel[3] >= 250
}

fn pixels_close(a: Rgba<u8>, b: Rgba<u8>) -> bool {
    a.0.iter().zip(b.0.iter()).all(|(l, r)| l.abs_diff(*r) <= 2)
}

pub fn validate_offline_html(source: &str) -> Result<()> {
    if source.len() > 64 * 1024 * 1024 {
        bail!("renderer HTML exceeds 64 MiB");
    }
    let compact: String = source
        .chars()
        .filter(|c| !c.is_ascii_whitespace() && !c.is_control())
        .flat_map(char::to_lowercase)
        .collect();
    for forbidden in [
        "http:", "https:", "ftp:", "ftps:", "ws:", "wss:", "file:", "//", "@import", "srcset=",
    ] {
        if compact.contains(forbidden) {
            bail!("offline renderer rejected external resource marker {forbidden:?}");
        }
    }
    Ok(())
}

fn strip_active_content(source: &str) -> (String, bool) {
    let mut value = source.to_owned();
    let mut removed = false;
    for tag in [
        "script", "iframe", "frame", "object", "embed", "applet", "form",
    ] {
        loop {
            let Some(start) = find_ascii_case_insensitive(&value, &format!("<{tag}")) else {
                break;
            };
            let close = format!("</{tag}>");
            let end = find_ascii_case_insensitive(&value[start..], &close)
                .map(|p| start + p + close.len())
                .or_else(|| value[start..].find('>').map(|p| start + p + 1))
                .unwrap_or(value.len());
            value.replace_range(start..end, "");
            removed = true;
        }
    }
    (value, removed)
}
fn strip_void_tag(source: &str, tag: &str) -> String {
    let mut value = source.to_owned();
    loop {
        let Some(start) = find_ascii_case_insensitive(&value, &format!("<{tag}")) else {
            break;
        };
        let end = value[start..]
            .find('>')
            .map(|x| start + x + 1)
            .unwrap_or(value.len());
        value.replace_range(start..end, "");
    }
    value
}
fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if !needle.is_ascii() {
        return None;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_capture_options() -> CaptureOptions {
        CaptureOptions {
            width: 1600,
            height: 900,
            scale: 1.0,
            timeout: std::time::Duration::from_secs(20),
        }
    }

    #[test]
    fn rejects_network_and_file_urls() {
        for html in [
            "<img src=https://x>",
            "<style>@import url(x)</style>",
            "<img src='//host/x'>",
            "<img src=file:///etc/passwd>",
        ] {
            assert!(validate_offline_html(html).is_err(), "accepted {html}");
        }
        assert!(validate_offline_html("<img src='data:image/png;base64,AA'>").is_ok());
    }

    #[test]
    fn strips_active_content_and_injects_restrictive_csp() {
        let html = build_capture_html(
            "<html><head><script>alert(1)</script></head><body><div class='slide-container' data-slide='1'></div><iframe src=x></iframe></body></html>",
            1,
            &default_capture_options(),
        )
        .unwrap();
        assert!(!html.contains("alert(1)"));
        assert!(!html.contains("<iframe"));
        assert!(html.contains("default-src 'none'"));
        assert!(html.contains("script-src 'none'"));
        assert!(html.contains("data-slide=\"1\""));
        assert!(html.contains("Unsupported dynamic content"));
        assert!(html.contains("transform:scale("));
        assert!(!html.contains("<script"));
        // Banner must live in the body, not the head.
        let body = html.to_ascii_lowercase().find("<body").unwrap();
        let banner = html.find("Unsupported dynamic content").unwrap();
        assert!(banner > body);
    }

    #[test]
    fn computes_css_scale_from_handler_design_vars() {
        let html = r#":root { --slide-design-w: 960pt; --slide-design-h: 540pt; }"#;
        // 960pt = 1280 CSS px, 540pt = 720 CSS px; 1600x900 => scale 1.25.
        let scale = capture_scale_for_html(html, 1600, 900);
        assert!((scale - 1.25).abs() < 1e-6, "scale={scale}");
    }

    #[tokio::test]
    async fn pinned_handler_html_matches_capture_selectors() {
        let directory = tempfile::tempdir().unwrap();
        let deck = directory.path().join("fixture.pptx");
        std::fs::write(&deck, crate::agent::deck_engine::BLANK_DECK).unwrap();
        let snapshot = crate::agent::deck_engine::DeckEngine::new(&deck)
            .unwrap()
            .snapshot()
            .await
            .unwrap();
        let count = handler_slide_count(&snapshot.html);
        assert!(
            count > 0,
            "pinned handler emitted no recognized slide containers"
        );
        let capture = build_capture_html(&snapshot.html, 1, &default_capture_options()).unwrap();
        assert!(capture.contains(".main>.slide-container[data-slide=\"1\"]"));
        assert!(capture.contains("transform:scale("));
        assert!(capture.contains("transform-origin:top left"));
    }

    #[tokio::test]
    async fn chromium_captures_pinned_handler_slide_when_available() {
        let Ok(browser) = Browser::probe(None) else {
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let deck = directory.path().join("fixture.pptx");
        std::fs::write(&deck, crate::agent::deck_engine::BLANK_DECK).unwrap();
        let snapshot = crate::agent::deck_engine::DeckEngine::new(&deck)
            .unwrap()
            .snapshot()
            .await
            .unwrap();
        let options = CaptureOptions {
            width: 960,
            height: 720,
            scale: 1.0,
            timeout: std::time::Duration::from_secs(20),
        };
        let html = build_capture_html(&snapshot.html, 1, &options).unwrap();
        let html_path = directory.path().join("capture.html");
        // Keep the final extension after the temporary marker. Some Chromium
        // builds only produce output when the screenshot path ends in `.png`.
        let png_path = directory.path().join(".capture.tmp.png");
        let profile = directory.path().join("profile");
        std::fs::write(&html_path, html).unwrap();
        browser
            .capture(&html_path, &png_path, &profile, &options)
            .await
            .unwrap();
        let image = image::open(png_path).unwrap();
        assert_eq!((image.width(), image.height()), (960, 720));
    }

    #[tokio::test]
    async fn chromium_captures_deck_pptx_content_when_available() {
        let Ok(browser) = Browser::probe(None) else {
            return;
        };
        let deck = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("deck.pptx");
        if !deck.is_file() {
            return;
        }
        let snapshot = crate::agent::deck_engine::DeckEngine::new(&deck)
            .unwrap()
            .snapshot()
            .await
            .unwrap();
        let options = CaptureOptions {
            width: 1600,
            height: 900,
            scale: 1.0,
            timeout: std::time::Duration::from_secs(30),
        };
        let html = build_capture_html(&snapshot.html, 1, &options).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let html_path = directory.path().join("capture.html");
        let png_path = directory.path().join("slide.png");
        let profile = directory.path().join("profile");
        std::fs::write(&html_path, html).unwrap();
        browser
            .capture(&html_path, &png_path, &profile, &options)
            .await
            .unwrap();
        let image = image::open(png_path).unwrap();
        assert!(
            !image_is_blank_capture(&image),
            "deck.pptx slide capture was blank"
        );
    }

    #[test]
    fn rejects_zero_slide() {
        assert!(build_capture_html("<html></html>", 0, &default_capture_options()).is_err());
    }

    #[test]
    fn blank_detector_accepts_uniform_white_and_rejects_color() {
        let white = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            32,
            32,
            Rgba([255, 255, 255, 255]),
        ));
        assert!(image_is_blank_capture(&white));
        let mut colored = image::RgbaImage::from_pixel(32, 32, Rgba([255, 255, 255, 255]));
        colored.put_pixel(8, 8, Rgba([253, 184, 19, 255]));
        assert!(!image_is_blank_capture(&DynamicImage::ImageRgba8(colored)));
    }
}
