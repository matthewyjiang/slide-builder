use super::{validate_and_render, ValidatedSvg};
use crate::agent::deck_engine::{MAX_MEDIA_BYTES, MAX_TEXT_BYTES};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::agent) struct AssetBrief {
    pub purpose: String,
    pub style: String,
    pub alt_text: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::agent) struct CreateAsset {
    pub name: String,
    pub brief: AssetBrief,
    pub svg: String,
    pub revises: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::agent) struct AssetRecord {
    pub id: Uuid,
    pub name: String,
    pub brief: AssetBrief,
    pub revises: Option<Uuid>,
    pub sha256: String,
    pub svg: String,
}

impl AssetRecord {
    pub fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id, "name": self.name, "brief": self.brief,
            "revises": self.revises, "sha256": self.sha256,
            "provenance": "agent_authored_svg", "visual_review": "required_per_placement"
        })
    }
}

#[derive(Clone)]
pub(in crate::agent) struct AssetStore {
    root: PathBuf,
}

#[derive(Debug, Default, Serialize)]
pub(in crate::agent) struct AssetListing {
    pub assets: Vec<serde_json::Value>,
    pub warnings: Vec<String>,
}

impl AssetStore {
    pub fn for_deck(deck: &Path) -> Self {
        // Resolve the existing deck directory, including platform aliases such as
        // macOS /var, before checking the managed directory itself for redirection.
        let deck = match (deck.parent(), deck.file_name()) {
            (Some(parent), Some(name)) => parent
                .canonicalize()
                .map(|p| p.join(name))
                .unwrap_or_else(|_| deck.to_owned()),
            _ => deck.to_owned(),
        };
        let mut root = deck.as_os_str().to_owned();
        root.push(".assets");
        Self {
            root: PathBuf::from(root),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Check existing ancestors without following a redirected asset directory.
    /// The tool authorizes this exact path before any directory/file creation.
    pub fn check_path(&self) -> Result<()> {
        for path in self.root.ancestors() {
            match fs::symlink_metadata(path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    bail!(
                        "asset store path must not contain symlinks: {}",
                        path.display()
                    );
                }
                Ok(metadata) if !metadata.is_dir() => {
                    bail!("asset store path is not a directory: {}", path.display());
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub fn create(&self, request: CreateAsset) -> Result<(AssetRecord, ValidatedSvg)> {
        validate_brief(&request.name, &request.brief)?;
        let rendered = validate_and_render(&request.svg)?;
        self.check_path()?;
        if let Some(id) = request.revises {
            self.load(id)
                .context("revision must reference an existing asset")?;
        }
        let record = AssetRecord {
            id: Uuid::new_v4(),
            name: request.name,
            brief: request.brief,
            revises: request.revises,
            sha256: digest(&request.svg),
            svg: request.svg,
        };
        let bytes = serde_json::to_vec_pretty(&record)?;
        check_size(bytes.len() as u64)?;
        fs::create_dir_all(&self.root)?;
        let mut file = tempfile::NamedTempFile::new_in(&self.root)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist_noclobber(self.path(record.id))?;
        Ok((record, rendered))
    }

    pub fn load(&self, id: Uuid) -> Result<AssetRecord> {
        self.check_path()?;
        let path = self.path(id);
        let metadata =
            fs::symlink_metadata(&path).with_context(|| format!("asset {id} is unavailable"))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            bail!("asset {id} is not a regular file");
        }
        check_size(metadata.len())?;
        let mut bytes = Vec::new();
        fs::File::open(&path)?
            .take(MAX_MEDIA_BYTES + 1)
            .read_to_end(&mut bytes)?;
        check_size(bytes.len() as u64)?;
        let record: AssetRecord = serde_json::from_slice(&bytes)
            .with_context(|| format!("asset {id} has an invalid record"))?;
        if record.id != id || record.sha256 != digest(&record.svg) {
            bail!("asset {id} integrity check failed; create a new candidate instead of editing stored files");
        }
        validate_brief(&record.name, &record.brief)?;
        Ok(record)
    }

    pub fn list(&self) -> Result<AssetListing> {
        self.check_path()?;
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(AssetListing::default())
            }
            Err(error) => return Err(error.into()),
        };
        let mut listing = AssetListing::default();
        let mut ids = Vec::new();
        for entry in entries {
            let path = entry?.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                match path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                {
                    Some(id) => ids.push(id),
                    None => listing.warnings.push(format!(
                        "Skipped {}: expected a UUID asset filename",
                        path.display()
                    )),
                }
            }
        }
        ids.sort();
        for id in ids {
            match self.load(id) {
                Ok(record) => listing.assets.push(record.summary()),
                Err(error) => listing
                    .warnings
                    .push(format!("Skipped asset {id}: {error:#}")),
            }
        }
        listing.warnings.sort();
        Ok(listing)
    }

    fn path(&self, id: Uuid) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }
}

fn digest(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

fn check_size(asked: u64) -> Result<()> {
    if asked > MAX_MEDIA_BYTES {
        bail!("asset record byte budget is {MAX_MEDIA_BYTES}; requested {asked}");
    }
    Ok(())
}

fn validate_brief(name: &str, brief: &AssetBrief) -> Result<()> {
    for (field, value) in [
        ("name", name),
        ("purpose", &brief.purpose),
        ("style", &brief.style),
        ("alt_text", &brief.alt_text),
    ] {
        if value.trim().is_empty() {
            bail!("asset {field} must not be blank");
        }
    }
    let metadata_bytes = serde_json::to_vec(&(name, brief))?.len();
    if metadata_bytes > MAX_TEXT_BYTES {
        bail!("asset metadata text byte budget is {MAX_TEXT_BYTES}; requested {metadata_bytes}");
    }
    Ok(())
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
