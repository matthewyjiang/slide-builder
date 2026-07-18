//! Immutable render cache entries and atomic manifests.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MANIFEST_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CacheKey {
    pub digest: String,
    pub renderer_version: String,
    pub handler_revision: String,
    pub width: u32,
    pub height: u32,
    /// Decimal scale is retained in the manifest for diagnostics. The digest
    /// uses its IEEE bits, avoiding locale and formatting instability.
    pub scale: String,
}

impl CacheKey {
    pub fn new(
        deck_bytes: &[u8],
        handler_revision: &str,
        renderer_version: &str,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<Self> {
        if width == 0 || height == 0 || !scale.is_finite() || scale <= 0.0 {
            bail!("invalid render geometry");
        }
        let mut hash = Sha256::new();
        hash.update(b"slide-builder-render-key-v1\0");
        hash.update((deck_bytes.len() as u64).to_le_bytes());
        hash.update(deck_bytes);
        put_field(&mut hash, handler_revision.as_bytes());
        put_field(&mut hash, renderer_version.as_bytes());
        hash.update(width.to_le_bytes());
        hash.update(height.to_le_bytes());
        hash.update(scale.to_bits().to_le_bytes());
        Ok(Self {
            digest: format!("{:x}", hash.finalize()),
            renderer_version: renderer_version.into(),
            handler_revision: handler_revision.into(),
            width,
            height,
            scale: scale.to_string(),
        })
    }

    pub fn short_digest(&self) -> &str {
        &self.digest[..16.min(self.digest.len())]
    }
}

fn put_field(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_le_bytes());
    hash.update(value);
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SlideImage {
    pub index: u32,
    /// Relative basename only, such as `slide-0001.png`.
    pub file: String,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RenderManifest {
    pub version: u32,
    pub generation: u64,
    pub cache_key: CacheKey,
    pub created_unix_ms: u64,
    pub slides: Vec<SlideImage>,
}

impl RenderManifest {
    pub fn new(generation: u64, cache_key: CacheKey, slides: Vec<SlideImage>) -> Result<Self> {
        validate_slides(&slides)?;
        Ok(Self {
            version: MANIFEST_VERSION,
            generation,
            cache_key,
            created_unix_ms: now_ms()?,
            slides,
        })
    }

    pub fn validate(&self, directory: &Path, verify_hashes: bool) -> Result<()> {
        if self.version != MANIFEST_VERSION {
            bail!("unsupported render manifest version {}", self.version);
        }
        if self.cache_key.digest.len() != 64 || !is_lower_hex(&self.cache_key.digest) {
            bail!("invalid cache digest");
        }
        validate_slides(&self.slides)?;
        for slide in &self.slides {
            let path = directory.join(&slide.file);
            if !path.is_file() {
                bail!("cached slide is missing: {}", path.display());
            }
            if verify_hashes {
                let actual = sha256_file(&path)?;
                if actual != slide.sha256 {
                    bail!("cached slide checksum mismatch: {}", path.display());
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct RenderCache {
    root: PathBuf,
    keep_generations: usize,
}

impl RenderCache {
    pub fn new(root: PathBuf, keep_generations: usize) -> Result<Self> {
        if !root.is_absolute() {
            bail!("render cache root must be absolute");
        }
        if keep_generations == 0 {
            bail!("keep_generations must be at least one");
        }
        Ok(Self {
            root,
            keep_generations,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// A stable path with no user-controlled path components.
    pub fn entry_dir(&self, deck_identity: &[u8], key: &CacheKey) -> PathBuf {
        let deck_hash = format!("{:x}", Sha256::digest(deck_identity));
        let profile = format!(
            "{}-{}x{}@{}",
            safe_version(&key.renderer_version),
            key.width,
            key.height,
            key.scale
        );
        self.root
            .join(&deck_hash[..16])
            .join(profile)
            .join(&key.digest)
    }

    pub fn load(&self, directory: &Path) -> Result<Option<RenderManifest>> {
        ensure_descendant(&self.root, directory)?;
        let path = directory.join("manifest.json");
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        let manifest: RenderManifest =
            serde_json::from_slice(&bytes).context("parse render manifest")?;
        manifest.validate(directory, false)?;
        Ok(Some(manifest))
    }

    /// Publishes the manifest last. Callers must already have atomically renamed
    /// all slide PNGs into `directory`.
    pub fn publish(&self, directory: &Path, manifest: &RenderManifest) -> Result<()> {
        ensure_descendant(&self.root, directory)?;
        fs::create_dir_all(directory)?;
        manifest.validate(directory, true)?;
        let bytes = serde_json::to_vec_pretty(manifest)?;
        let tmp = directory.join(format!(
            ".manifest-{}-{}.tmp",
            std::process::id(),
            now_ms()?
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, directory.join("manifest.json"))?;
        if let Ok(dir) = fs::File::open(directory) {
            let _ = dir.sync_all();
        }
        Ok(())
    }

    /// Removes abandoned `.tmp-*` directories and old complete key entries.
    pub fn cleanup(&self) -> Result<()> {
        if !self.root.exists() {
            return Ok(());
        }
        cleanup_tree(&self.root, self.keep_generations)
    }
}

pub fn sha256_file(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

fn validate_slides(slides: &[SlideImage]) -> Result<()> {
    for (position, slide) in slides.iter().enumerate() {
        if slide.index as usize != position + 1 {
            bail!("slide indices must be contiguous and one-based");
        }
        let expected = format!("slide-{:04}.png", slide.index);
        if slide.file != expected {
            bail!("invalid slide filename {:?}", slide.file);
        }
        if slide.width == 0
            || slide.height == 0
            || slide.sha256.len() != 64
            || !is_lower_hex(&slide.sha256)
        {
            bail!("invalid metadata for slide {}", slide.index);
        }
    }
    Ok(())
}
fn is_lower_hex(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn safe_version(value: &str) -> String {
    let value: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if value.is_empty() {
        "renderer".into()
    } else {
        value
    }
}
fn ensure_descendant(root: &Path, child: &Path) -> Result<()> {
    if !child.is_absolute() || !child.starts_with(root) || child == root {
        bail!("cache path escapes render root");
    }
    if child
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        bail!("cache path contains traversal");
    }
    // Do not follow an attacker-planted symlink out of the app-owned cache.
    // The root itself may legitimately be reached through an XDG symlink.
    let relative = child.strip_prefix(root).context("cache path prefix")?;
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        cursor.push(component);
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!("cache path contains symlink: {}", cursor.display());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("inspect cache path {}", cursor.display()))
            }
        }
    }
    Ok(())
}
fn cleanup_tree(root: &Path, keep: usize) -> Result<()> {
    for deck in fs::read_dir(root)? {
        let deck = deck?;
        if !deck.file_type()?.is_dir() {
            continue;
        }
        for profile in fs::read_dir(deck.path())? {
            let profile = profile?;
            if !profile.file_type()?.is_dir() {
                continue;
            }
            let mut complete = Vec::new();
            for entry in fs::read_dir(profile.path())? {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let name = entry.file_name();
                if name.to_string_lossy().starts_with(".tmp-")
                    || !entry.path().join("manifest.json").is_file()
                {
                    fs::remove_dir_all(entry.path())?;
                } else {
                    let modified = entry.metadata()?.modified().unwrap_or(UNIX_EPOCH);
                    complete.push((modified, entry.path()));
                }
            }
            complete.sort_by_key(|x| std::cmp::Reverse(x.0));
            for (_, path) in complete.into_iter().skip(keep) {
                fs::remove_dir_all(path)?;
            }
        }
    }
    Ok(())
}
fn now_ms() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before epoch")?
        .as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_changes_for_every_render_input() {
        let a = CacheKey::new(b"deck", "handler", "renderer", 10, 10, 1.0).unwrap();
        assert_ne!(
            a.digest,
            CacheKey::new(b"deck2", "handler", "renderer", 10, 10, 1.0)
                .unwrap()
                .digest
        );
        assert_ne!(
            a.digest,
            CacheKey::new(b"deck", "handler2", "renderer", 10, 10, 1.0)
                .unwrap()
                .digest
        );
        assert_ne!(
            a.digest,
            CacheKey::new(b"deck", "handler", "renderer", 11, 10, 1.0)
                .unwrap()
                .digest
        );
    }
    #[test]
    fn manifest_rejects_path_like_slide_names() {
        let key = CacheKey::new(b"d", "h", "r", 1, 1, 1.0).unwrap();
        let bad = SlideImage {
            index: 1,
            file: "../x.png".into(),
            width: 1,
            height: 1,
            sha256: "0".repeat(64),
        };
        assert!(RenderManifest::new(1, key, vec![bad]).is_err());
    }
}
