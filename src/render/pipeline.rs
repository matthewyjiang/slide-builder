//! Offline HTML-to-PNG rendering pipeline.

use crate::render::browser::{Browser, CaptureOptions};
use crate::render::cache::{sha256_file, CacheKey, RenderCache, RenderManifest, SlideImage};
use anyhow::{bail, Context, Result};
use image::ImageReader;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub const RENDERER_VERSION: &str = "html-capture-v1";
const CAPTURE_NONCE: &str = "slide-builder-capture-v1";

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
        // Sanitize once before starting any browser processes. Per-slide HTML
        // differs only in the app-owned selector script.
        validate_offline_html(source_html)?;
        let semaphore = Arc::new(Semaphore::new(self.max_concurrency));
        let mut jobs = JoinSet::new();
        for index in 1..=slide_count {
            let permit = semaphore.clone().acquire_owned().await?;
            let html = build_capture_html(source_html, index)?;
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
    let temporary_png = directory.join(format!(".slide-{index:04}.png.tmp"));
    let final_name = format!("slide-{index:04}.png");
    let final_png = directory.join(&final_name);
    let profile = directory.join(format!("profile-{index:04}"));
    fs::write(&capture, html)?;
    let capture_result = browser
        .capture(&capture, &temporary_png, &profile, &options)
        .await;
    let _ = fs::remove_file(&capture);
    let _ = fs::remove_dir_all(&profile);
    capture_result.with_context(|| format!("capture slide {index}"))?;

    let reader = ImageReader::open(&temporary_png)?.with_guessed_format()?;
    if reader.format() != Some(image::ImageFormat::Png) {
        bail!("slide {index} is not a PNG");
    }
    let image = reader
        .decode()
        .with_context(|| format!("decode slide {index}"))?;
    if image.width() == 0 || image.height() == 0 {
        bail!("slide {index} has empty dimensions");
    }
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

/// Reject active/network content and return a capture-only document with a
/// restrictive CSP. Existing scripts and plugin/frame elements are removed.
pub fn build_capture_html(source: &str, slide_index: u32) -> Result<String> {
    if slide_index == 0 {
        bail!("slide index is one-based");
    }
    validate_offline_html(source)?;
    let (mut html, dynamic_removed) = strip_active_content(source);
    // A base element can redirect otherwise relative URLs. CSP is the primary
    // enforcement layer, while removing it keeps local resolution predictable.
    html = strip_void_tag(&html, "base");
    html = strip_void_tag(&html, "meta");
    let placeholder = if dynamic_removed {
        "<div class=\"slide-builder-unsupported\">Unsupported dynamic content was removed for offline preview.</div>"
    } else {
        ""
    };
    let head = format!(
        r#"<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src 'self' data:; media-src 'none'; font-src 'self' data:; style-src 'unsafe-inline'; script-src 'nonce-{CAPTURE_NONCE}'; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'">
<style>
html,body{{margin:0!important;padding:0!important;overflow:hidden!important;background:transparent!important}}
body>*:not(.slide-container){{display:none!important}}
.slide-container{{display:none!important;margin:0!important}}
.slide-container[data-slide="{slide_index}"]{{display:block!important}}
.slide-builder-unsupported{{position:fixed;inset:auto 1rem 1rem 1rem;z-index:2147483647;padding:.5rem;background:#fff3cd;color:#5f4700;font:14px sans-serif}}
</style>{placeholder}"#
    );
    let script = format!(
        r#"<script nonce="{CAPTURE_NONCE}">
(()=>{{const s=document.querySelector('.slide-container[data-slide="{slide_index}"]');if(!s){{document.body.textContent='Slide {slide_index} is unavailable';}}Promise.resolve(document.fonts&&document.fonts.ready).catch(()=>{{}}).finally(()=>{{requestAnimationFrame(()=>requestAnimationFrame(()=>document.documentElement.dataset.slideBuilderReady='true'));}});}})();
</script>"#
    );
    if let Some(position) = find_ascii_case_insensitive(&html, "</head>") {
        html.insert_str(position, &head);
    } else if let Some(position) = find_ascii_case_insensitive(&html, "<body") {
        html.insert_str(position, &format!("<head>{head}</head>"));
    } else {
        html.insert_str(0, &format!("<!doctype html><head>{head}</head><body>"));
        html.push_str("</body>");
    }
    if let Some(position) = find_ascii_case_insensitive(&html, "</body>") {
        html.insert_str(position, &script);
    } else {
        html.push_str(&script);
    }
    Ok(html)
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
        let html = build_capture_html("<html><head><script>alert(1)</script></head><body><div class='slide-container' data-slide='1'></div><iframe src=x></iframe></body></html>", 1).unwrap();
        assert!(!html.contains("alert(1)"));
        assert!(!html.contains("<iframe"));
        assert!(html.contains("default-src 'none'"));
        assert!(html.contains("data-slide=\"1\""));
        assert!(html.contains("Unsupported dynamic content"));
    }
    #[test]
    fn rejects_zero_slide() {
        assert!(build_capture_html("<html></html>", 0).is_err());
    }
}
