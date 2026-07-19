//! Transactional adapter around the non-`Sync` OfficeCli PowerPoint handler.

use super::inspection_geometry;
use anyhow::{anyhow, bail, Context, Result};
use handler_common::{
    output_format::{RawOptions, ViewOptions},
    DocumentHandler,
};
use pptx_handler::PptxHandler;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::sync::Mutex;

mod mutation;
mod state;

use mutation::transact;
use state::{append_pictures_to_slide_inspection, path_for_stable_id, value_for_path};

pub const MAX_MEDIA_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_TEXT_BYTES: usize = 1_000_000;
pub const MAX_MUTATIONS_PER_BATCH: usize = 100;
pub const BLANK_DECK: &[u8] = include_bytes!("../../assets/blank.pptx");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum DeckMutation {
    Add {
        parent: String,
        element_type: String,
        #[serde(default)]
        properties: HashMap<String, String>,
    },
    Set {
        path: String,
        properties: HashMap<String, String>,
    },
    Remove {
        path: String,
    },
    Move {
        source: String,
        target_parent: Option<String>,
        index: Option<usize>,
    },
    Copy {
        source: String,
        target_parent: String,
        index: Option<usize>,
    },
    Swap {
        left: String,
        right: String,
    },
    RawSet {
        part: String,
        xpath: String,
        action: String,
        xml: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationResult {
    pub generation: u64,
    /// Stable OOXML-backed identifiers, rather than positional handler paths.
    pub affected: Vec<String>,
    pub validation_errors: Vec<String>,
    /// The structured outline after the committed mutation.
    pub post_state: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct DeckSnapshot {
    pub generation: u64,
    pub html: String,
    pub outline: String,
}

#[derive(Clone)]
pub struct DeckEngine {
    path: Arc<PathBuf>,
    lock: Arc<Mutex<()>>,
    generation: Arc<AtomicU64>,
}

impl DeckEngine {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = absolute_clean(path.as_ref())?;
        Ok(Self {
            path: Arc::new(path),
            lock: Arc::new(Mutex::new(())),
            generation: Arc::new(AtomicU64::new(0)),
        })
    }
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Advance render correlation after the deck watcher observes a content change.
    pub fn record_file_change(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub async fn create(path: impl AsRef<Path>, template: Option<&Path>) -> Result<Self> {
        let engine = Self::new(path)?;
        let _guard = engine.lock.lock().await;
        let path = engine.path.clone();
        let template = template.map(Path::to_owned);
        tokio::task::spawn_blocking(move || -> Result<()> {
            if path.exists() {
                bail!("refusing to overwrite existing deck {}", path.display());
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if let Some(source) = template {
                if source
                    .extension()
                    .and_then(|v| v.to_str())
                    .map(|v| v.eq_ignore_ascii_case("pptx"))
                    != Some(true)
                {
                    bail!("template must be a .pptx file");
                }
                std::fs::copy(&source, path.as_ref())
                    .with_context(|| format!("copy template {}", source.display()))?;
            } else {
                std::fs::write(path.as_ref(), BLANK_DECK)?;
            }
            let handler = PptxHandler::open(
                path.to_str().ok_or_else(|| anyhow!("non-UTF8 deck path"))?,
                false,
            )?;
            let errors = handler.validate()?;
            if !errors.is_empty() {
                bail!("starter deck failed validation: {errors:?}");
            }
            Ok(())
        })
        .await??;
        drop(_guard);
        Ok(engine)
    }

    /// Copy-mutate-validate-save-reopen-rename transaction. The original is unchanged on error.
    pub async fn mutate(&self, op: DeckMutation) -> Result<MutationResult> {
        self.mutate_many(vec![op]).await
    }

    /// Apply multiple mutations in one transaction and advance generation once.
    pub async fn mutate_many(&self, ops: Vec<DeckMutation>) -> Result<MutationResult> {
        if ops.is_empty() {
            bail!("mutation batch must not be empty");
        }
        if ops.len() > MAX_MUTATIONS_PER_BATCH {
            bail!("mutation batch exceeds {MAX_MUTATIONS_PER_BATCH} operation limit");
        }
        if serde_json::to_vec(&ops)?.len() > MAX_TEXT_BYTES {
            bail!("mutation batch exceeds {MAX_TEXT_BYTES} byte limit");
        }
        for op in &ops {
            validate_payload(op)?;
        }
        let _guard = self.lock.lock().await;
        let path = self.path.clone();
        let next = self.generation() + 1;
        let transaction =
            tokio::task::spawn_blocking(move || transact(path.as_ref(), ops)).await??;
        self.generation.store(next, Ordering::Release);
        Ok(MutationResult {
            generation: next,
            affected: transaction.affected,
            validation_errors: vec![],
            post_state: transaction.post_state,
        })
    }

    pub async fn snapshot(&self) -> Result<DeckSnapshot> {
        let _guard = self.lock.lock().await;
        let path = self.path.clone();
        let generation = self.generation();
        tokio::task::spawn_blocking(move || {
            let handler = open(&path, false)?;
            Ok(DeckSnapshot {
                generation,
                html: handler.view_as_html(ViewOptions::default())?,
                outline: handler.view_as_outline()?,
            })
        })
        .await?
    }

    pub async fn inspect(&self, path_query: Option<String>) -> Result<serde_json::Value> {
        let _guard = self.lock.lock().await;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let handler = open(&path, false)?;
            let mut outline = handler.view_as_outline_json()?;
            let html = handler.view_as_html(ViewOptions::default())?;
            inspection_geometry::enrich(&mut outline, &html);
            if let Some(path) = path_query {
                if path.contains("/picture[") {
                    return value_for_path(&outline, &path)
                        .cloned()
                        .ok_or_else(|| anyhow!("selector `{path}` matched no element"));
                }
                let mut inspected = serde_json::to_value(handler.get(&path, 4)?)?;
                append_pictures_to_slide_inspection(&mut inspected, &outline, &path);
                return Ok(inspected);
            }
            Ok(outline)
        })
        .await?
    }

    pub async fn raw(&self, part: String) -> Result<String> {
        reject_part(&part)?;
        let _guard = self.lock.lock().await;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            open(&path, false)?
                .raw(&part, RawOptions::default())
                .map_err(Into::into)
        })
        .await?
    }

    /// Resolve a stable ID returned by a mutation to the handler's current path.
    /// Positional paths remain accepted for advanced-tool compatibility.
    pub async fn resolve_element(&self, id_or_path: String) -> Result<String> {
        if id_or_path.starts_with('/') {
            return Ok(id_or_path);
        }
        let _guard = self.lock.lock().await;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let handler = open(&path, false)?;
            let mut outline = handler.view_as_outline_json()?;
            let html = handler.view_as_html(ViewOptions::default())?;
            inspection_geometry::enrich(&mut outline, &html);
            path_for_stable_id(&outline, &id_or_path)
                .ok_or_else(|| anyhow!("stable element ID `{id_or_path}` was not found"))
        })
        .await?
    }
}

fn open(path: &Path, editable: bool) -> Result<PptxHandler> {
    PptxHandler::open(
        path.to_str().ok_or_else(|| anyhow!("non-UTF8 deck path"))?,
        editable,
    )
    .map_err(Into::into)
}
fn absolute_clean(path: &Path) -> Result<PathBuf> {
    let value = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    if value
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        bail!("deck path cannot contain parent traversal");
    }
    if value
        .extension()
        .and_then(|v| v.to_str())
        .map(|v| v.eq_ignore_ascii_case("pptx"))
        != Some(true)
    {
        bail!("deck path must end in .pptx");
    }
    if std::fs::symlink_metadata(&value)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        bail!("active deck cannot be a symlink");
    }
    if let (Some(parent), Some(name)) = (value.parent(), value.file_name()) {
        if parent.exists() {
            return Ok(parent.canonicalize()?.join(name));
        }
    }
    Ok(value)
}
fn reject_part(part: &str) -> Result<()> {
    if part.starts_with('/') || part.contains("..") || part.contains('\\') {
        bail!("unsafe package part path");
    }
    Ok(())
}
fn validate_payload(op: &DeckMutation) -> Result<()> {
    let encoded = serde_json::to_vec(op)?;
    if encoded.len() > MAX_TEXT_BYTES {
        bail!("tool payload exceeds {} bytes", MAX_TEXT_BYTES);
    }
    let strings: Vec<&str> = match op {
        DeckMutation::Add { parent, .. } => vec![parent],
        DeckMutation::Set { path, .. } | DeckMutation::Remove { path } => vec![path],
        DeckMutation::Move {
            source,
            target_parent,
            ..
        } => std::iter::once(source.as_str())
            .chain(target_parent.as_deref())
            .collect(),
        DeckMutation::Copy {
            source,
            target_parent,
            ..
        } => vec![source, target_parent],
        DeckMutation::Swap { left, right } => vec![left, right],
        DeckMutation::RawSet { part, .. } => {
            reject_part(part)?;
            vec![]
        }
    };
    if strings.iter().any(|s| s.contains("..") || s.contains('\0')) {
        bail!("unsafe selector: traversal or NUL is forbidden");
    }
    Ok(())
}

#[cfg(test)]
#[path = "deck_engine_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "deck_engine_inspection_tests.rs"]
mod inspection_tests;
