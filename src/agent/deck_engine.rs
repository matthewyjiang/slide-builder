//! Transactional adapter around the non-`Sync` OfficeCli PowerPoint handler.
use anyhow::{anyhow, bail, Context, Result};
use handler_common::{
    output_format::{RawOptions, ViewOptions},
    DocumentHandler, InsertPosition,
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

pub const MAX_MEDIA_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_TEXT_BYTES: usize = 1_000_000;
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
    pub affected: Vec<String>,
    pub validation_errors: Vec<String>,
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
        validate_payload(&op)?;
        let _guard = self.lock.lock().await;
        let path = self.path.clone();
        let next = self.generation() + 1;
        let affected = tokio::task::spawn_blocking(move || transact(path.as_ref(), op)).await??;
        self.generation.store(next, Ordering::Release);
        Ok(MutationResult {
            generation: next,
            affected,
            validation_errors: vec![],
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
            match path_query {
                Some(p) => Ok(serde_json::to_value(handler.get(&p, 4)?)?),
                None => handler.view_as_outline_json().map_err(Into::into),
            }
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
}

fn transact(path: &Path, op: DeckMutation) -> Result<Vec<String>> {
    if !path.exists() {
        bail!("deck does not exist: {}", path.display());
    }
    let parent = path.parent().ok_or_else(|| anyhow!("deck has no parent"))?;
    let file = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| anyhow!("invalid deck filename"))?;
    let temp = parent.join(format!(".{file}.{}.tmp.pptx", uuid::Uuid::new_v4()));
    std::fs::copy(path, &temp)?;
    let result = (|| -> Result<Vec<String>> {
        let handler = open(&temp, true)?;
        let affected = apply(&handler, op)?;
        let errors = handler.validate()?;
        if !errors.is_empty() {
            bail!("mutation validation failed; original unchanged: {errors:?}");
        }
        handler.save()?;
        drop(handler);
        let verified = open(&temp, false)?;
        let errors = verified.validate()?;
        if !errors.is_empty() {
            bail!("saved package failed reopen validation; original unchanged: {errors:?}");
        }
        drop(verified);
        std::fs::rename(&temp, path)?;
        Ok(affected)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn apply(handler: &PptxHandler, op: DeckMutation) -> Result<Vec<String>> {
    Ok(match op {
        DeckMutation::Add {
            parent,
            element_type,
            properties,
        } => vec![handler.add(
            &parent,
            &element_type,
            InsertPosition::Append,
            &properties,
            None,
        )?],
        DeckMutation::Set { path, properties } => handler.set(&path, &properties)?,
        DeckMutation::Remove { path } => {
            handler
                .get(&path, 0)
                .with_context(|| format!("selector `{path}` matched no element"))?;
            handler
                .remove(&path)?
                .map(|affected| vec![affected])
                .ok_or_else(|| anyhow!("selector `{path}` matched no element"))?
        }
        DeckMutation::Move {
            source,
            target_parent,
            index,
        } => vec![handler.move_element(
            &source,
            target_parent.as_deref(),
            index
                .map(InsertPosition::AtIndex)
                .unwrap_or(InsertPosition::Append),
        )?],
        DeckMutation::Copy {
            source,
            target_parent,
            index,
        } => vec![handler.copy_from(
            &source,
            &target_parent,
            index
                .map(InsertPosition::AtIndex)
                .unwrap_or(InsertPosition::Append),
        )?],
        DeckMutation::Swap { left, right } => {
            let (a, b) = handler.swap(&left, &right)?;
            vec![a, b]
        }
        DeckMutation::RawSet {
            part,
            xpath,
            action,
            xml,
        } => {
            reject_part(&part)?;
            handler.raw_set(&part, &xpath, &action, xml.as_deref())?;
            vec![part]
        }
    })
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
mod tests {
    use super::*;
    #[test]
    fn traversal_rejected() {
        assert!(DeckEngine::new("../bad.pptx").is_err());
        assert!(reject_part("../x").is_err());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_deck_rejected() {
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("target.pptx");
        std::fs::write(&target, BLANK_DECK).unwrap();
        let link = d.path().join("link.pptx");
        symlink(&target, &link).unwrap();
        assert!(DeckEngine::new(link).is_err());
    }
    #[tokio::test]
    async fn fixture_opens_and_snapshots() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.pptx");
        let e = DeckEngine::create(&p, None).await.unwrap();
        let s = e.snapshot().await.unwrap();
        assert!(s.html.contains("html"));
    }
    #[tokio::test]
    async fn failed_mutation_preserves_original() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.pptx");
        let e = DeckEngine::create(&p, None).await.unwrap();
        let before = std::fs::read(&p).unwrap();
        assert!(e
            .mutate(DeckMutation::Remove {
                path: "/slide[999]".into()
            })
            .await
            .is_err());
        assert_eq!(before, std::fs::read(&p).unwrap());
    }
}
