//! Transactional adapter around the non-`Sync` OfficeCli PowerPoint handler.

use super::inspection_geometry;
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
        validate_payload(&op)?;
        let _guard = self.lock.lock().await;
        let path = self.path.clone();
        let next = self.generation() + 1;
        let transaction =
            tokio::task::spawn_blocking(move || transact(path.as_ref(), op)).await??;
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
            let mut inspected = match path_query {
                Some(p) => serde_json::to_value(handler.get(&p, 4)?)?,
                None => handler.view_as_outline_json()?,
            };
            let html = handler.view_as_html(ViewOptions::default())?;
            inspection_geometry::enrich(&mut inspected, &html);
            Ok(inspected)
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

struct TransactionResult {
    affected: Vec<String>,
    post_state: serde_json::Value,
}

fn transact(path: &Path, mut op: DeckMutation) -> Result<TransactionResult> {
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
    let result = (|| -> Result<TransactionResult> {
        let handler = open(&temp, true)?;
        let mut before = handler.view_as_outline_json()?;
        let before_html = handler.view_as_html(ViewOptions::default())?;
        inspection_geometry::enrich(&mut before, &before_html);
        resolve_mutation_selectors(&before, &mut op)?;
        let result_op = op.clone();
        let use_pre_mutation_ids = matches!(
            op,
            DeckMutation::Set { .. }
                | DeckMutation::Remove { .. }
                | DeckMutation::Move { .. }
                | DeckMutation::Swap { .. }
        );
        let stable_before = stable_ids_for_mutation(&before, &op);
        let handler_affected = apply(&handler, op)?;
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
        // The pinned handler's structural validator does not parse every slide.
        // Rendering does, so require it to succeed before the temporary package
        // can replace the original.
        let html = verified
            .view_as_html(ViewOptions::default())
            .context("saved package failed slide XML validation; original unchanged")?;
        let mut post_state = verified.view_as_outline_json()?;
        inspection_geometry::enrich(&mut post_state, &html);
        let mut stable_after = handler_affected
            .iter()
            .filter_map(|path| stable_id_for_path(&post_state, path))
            .collect::<Vec<_>>();
        if stable_after.is_empty() {
            if let DeckMutation::Add {
                parent,
                element_type,
                ..
            } = &result_op
            {
                if let Some(added) =
                    stable_id_for_added_element(&before, &post_state, parent, element_type)
                {
                    stable_after.push(added);
                }
            }
        }
        if matches!(result_op, DeckMutation::Add { .. }) && stable_after.is_empty() {
            bail!("add mutation created no inspectable element; original unchanged");
        }
        drop(verified);
        std::fs::rename(&temp, path)?;
        Ok(TransactionResult {
            affected: if use_pre_mutation_ids || stable_after.is_empty() {
                stable_before
            } else {
                stable_after
            },
            post_state,
        })
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
        } => {
            let handler_parent = if element_type == "slide" {
                parent.clone()
            } else {
                ensure_drawing_namespace(handler, &parent)?;
                physical_slide_path(handler, &parent)?
            };
            let reported = handler.add(
                &handler_parent,
                &element_type,
                InsertPosition::Append,
                &properties,
                None,
            )?;
            // Shape adders in the pinned revision calculate the returned index after
            // insertion and are therefore one past the actual shape. Recover the
            // real path from the post-add outline.
            let added = if element_type == "slide" {
                reported
            } else {
                last_shape_path(handler, &parent).unwrap_or(reported)
            };
            if let Some(preset) = properties.get("preset") {
                set_shape_preset(handler, &added, preset)?;
            }
            // The pinned handler applies geometry while creating rectangles, but only
            // applies text-run formatting through `set`.
            let formatting = properties
                .iter()
                .filter(|(key, _)| {
                    matches!(
                        key.as_str(),
                        "bold"
                            | "italic"
                            | "font"
                            | "fontName"
                            | "fontSize"
                            | "color"
                            | "fontColor"
                            | "alignment"
                    )
                })
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<HashMap<_, _>>();
            if !formatting.is_empty() {
                handler.set(&added, &formatting)?;
            }
            vec![added]
        }
        DeckMutation::Set {
            path,
            mut properties,
        } => {
            let geometry = take_geometry(&mut properties);
            if !geometry.is_empty() {
                let physical_path = physical_shape_path(handler, &path)?;
                set_shape_geometry(handler, &physical_path, &geometry)?;
            }
            // Handler semantic paths follow presentation order, so text/style
            // updates must retain the logical path after a slide reorder.
            if !properties.is_empty() {
                handler.set(&path, &properties)?;
            }
            vec![path]
        }
        DeckMutation::Remove { path } => {
            handler
                .get(&path, 0)
                .with_context(|| format!("selector `{path}` matched no element"))?;
            if path.starts_with("/slide[") && !path.contains("/shape[") {
                let index = slide_index(&path)?;
                rewrite_slide_id_list(handler, |entries| {
                    if index > entries.len() {
                        bail!("selector `{path}` matched no slide");
                    }
                    entries.remove(index - 1);
                    Ok(())
                })?;
                vec![path]
            } else {
                handler
                    .remove(&path)?
                    .map(|affected| vec![affected])
                    .ok_or_else(|| anyhow!("selector `{path}` matched no element"))?
            }
        }
        DeckMutation::Move {
            source,
            target_parent: _,
            index,
        } => {
            let from = slide_index(&source)?;
            let target = index.ok_or_else(|| anyhow!("slide move requires a target index"))?;
            rewrite_slide_id_list(handler, |entries| {
                if from == 0 || from > entries.len() || target == 0 || target > entries.len() {
                    bail!("slide reorder indices must be within 1..={}", entries.len());
                }
                let entry = entries.remove(from - 1);
                entries.insert(target - 1, entry);
                Ok(())
            })?;
            vec![format!("/slide[{target}]")]
        }
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
            let a = slide_index(&left)?;
            let b = slide_index(&right)?;
            rewrite_slide_id_list(handler, |entries| {
                if a == 0 || b == 0 || a > entries.len() || b > entries.len() {
                    bail!("slide swap indices must be within 1..={}", entries.len());
                }
                if a == b {
                    bail!("slide swap requires two different slides");
                }
                entries.swap(a - 1, b - 1);
                Ok(())
            })?;
            vec![left, right]
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

fn take_geometry(properties: &mut HashMap<String, String>) -> HashMap<String, String> {
    [
        "x", "left", "y", "top", "width", "w", "cx", "height", "h", "cy",
    ]
    .into_iter()
    .filter_map(|key| properties.remove(key).map(|value| (key.to_owned(), value)))
    .collect()
}

fn value_as_emu(value: &str) -> Result<i64> {
    let value = value.trim();
    let (number, multiplier) = if let Some(number) = value.strip_suffix("in") {
        (number, 914_400.0)
    } else if let Some(number) = value.strip_suffix("cm") {
        (number, 360_000.0)
    } else if let Some(number) = value.strip_suffix("mm") {
        (number, 36_000.0)
    } else if let Some(number) = value.strip_suffix("pt") {
        (number, 12_700.0)
    } else if let Some(number) = value.strip_suffix("px") {
        (number, 9_525.0)
    } else {
        return value
            .parse::<i64>()
            .with_context(|| format!("invalid geometry value `{value}`"));
    };
    let number = number
        .parse::<f64>()
        .with_context(|| format!("invalid geometry value `{value}`"))?;
    if !number.is_finite() {
        bail!("geometry must be finite");
    }
    Ok((number * multiplier).round() as i64)
}

fn xml_attr(element: &str, name: &str) -> Option<i64> {
    let marker = format!("{name}=\"");
    let start = element.find(&marker)? + marker.len();
    let end = start + element[start..].find('"')?;
    element[start..end].parse().ok()
}

fn set_shape_geometry(
    handler: &PptxHandler,
    path: &str,
    properties: &HashMap<String, String>,
) -> Result<()> {
    let slide = slide_index(path)?;
    let shape = path
        .split("/shape[")
        .nth(1)
        .and_then(|value| value.strip_suffix(']'))
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| anyhow!("geometry updates require /slide[N]/shape[M]"))?;
    let part = format!("ppt/slides/slide{slide}.xml");
    let mut xml = handler.raw(&part, RawOptions::default())?;
    let mut cursor = 0;
    let mut shape_start = None;
    for _ in 0..shape {
        let relative = xml[cursor..]
            .find("<p:sp>")
            .ok_or_else(|| anyhow!("shape `{path}` was not found"))?;
        shape_start = Some(cursor + relative);
        cursor += relative + "<p:sp>".len();
    }
    let shape_start = shape_start.unwrap();
    let shape_end = shape_start
        + xml[shape_start..]
            .find("</p:sp>")
            .ok_or_else(|| anyhow!("malformed shape `{path}`"))?;
    let relative_xfrm = xml[shape_start..shape_end]
        .find("<a:xfrm>")
        .ok_or_else(|| anyhow!("shape `{path}` has no transform"))?;
    let xfrm_start = shape_start + relative_xfrm;
    let xfrm_end = xfrm_start
        + xml[xfrm_start..shape_end]
            .find("</a:xfrm>")
            .ok_or_else(|| anyhow!("shape `{path}` has a malformed transform"))?
        + "</a:xfrm>".len();
    let old = &xml[xfrm_start..xfrm_end];
    let pick = |keys: &[&str], old_value: i64| -> Result<i64> {
        keys.iter()
            .find_map(|key| properties.get(*key))
            .map(|value| value_as_emu(value))
            .unwrap_or(Ok(old_value))
    };
    let x = pick(&["x", "left"], xml_attr(old, "x").unwrap_or(0))?;
    let y = pick(&["y", "top"], xml_attr(old, "y").unwrap_or(0))?;
    let cx = pick(
        &["width", "w", "cx"],
        xml_attr(old, "cx").unwrap_or(914_400),
    )?;
    let cy = pick(
        &["height", "h", "cy"],
        xml_attr(old, "cy").unwrap_or(914_400),
    )?;
    let replacement =
        format!("<a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>");
    xml.replace_range(xfrm_start..xfrm_end, &replacement);
    replace_slide_document(handler, &part, &xml)?;
    Ok(())
}

fn set_shape_preset(handler: &PptxHandler, path: &str, preset: &str) -> Result<()> {
    if preset.is_empty()
        || !preset
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        bail!("invalid shape preset `{preset}`");
    }
    let physical_path = physical_shape_path(handler, path)?;
    let slide = slide_index(&physical_path)?;
    let shape = physical_path
        .split("/shape[")
        .nth(1)
        .and_then(|value| value.strip_suffix(']'))
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| anyhow!("preset updates require /slide[N]/shape[M]"))?;
    let part = format!("ppt/slides/slide{slide}.xml");
    let mut xml = handler.raw(&part, RawOptions::default())?;
    let mut cursor = 0;
    let mut shape_start = None;
    for _ in 0..shape {
        let relative = xml[cursor..]
            .find("<p:sp>")
            .ok_or_else(|| anyhow!("shape `{path}` was not found"))?;
        shape_start = Some(cursor + relative);
        cursor += relative + "<p:sp>".len();
    }
    let shape_start = shape_start.unwrap();
    let shape_end = shape_start
        + xml[shape_start..]
            .find("</p:sp>")
            .ok_or_else(|| anyhow!("malformed shape `{path}`"))?;
    let geometry_start = shape_start
        + xml[shape_start..shape_end]
            .find("<a:prstGeom")
            .ok_or_else(|| anyhow!("shape `{path}` has no preset geometry"))?;
    let marker = "prst=\"";
    let value_start = geometry_start
        + xml[geometry_start..shape_end]
            .find(marker)
            .ok_or_else(|| anyhow!("shape `{path}` has malformed preset geometry"))?
        + marker.len();
    let value_end = value_start
        + xml[value_start..shape_end]
            .find('"')
            .ok_or_else(|| anyhow!("shape `{path}` has malformed preset geometry"))?;
    xml.replace_range(value_start..value_end, preset);
    replace_slide_document(handler, &part, &xml)?;
    Ok(())
}

/// Replace a slide root without duplicating the package XML declaration.
fn replace_slide_document(handler: &PptxHandler, part: &str, xml: &str) -> Result<()> {
    let root_start = xml
        .find("<p:sld")
        .ok_or_else(|| anyhow!("slide part `{part}` has no p:sld root"))?;
    handler.raw_set(part, "/sld", "replace", Some(&xml[root_start..]))?;
    Ok(())
}

fn slide_index(path: &str) -> Result<usize> {
    path.strip_prefix("/slide[")
        .and_then(|rest| rest.split(']').next())
        .and_then(|index| index.parse().ok())
        .filter(|index| *index > 0)
        .ok_or_else(|| anyhow!("expected /slide[N], got `{path}`"))
}

fn xml_string_attr<'a>(element: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("{name}=\"");
    let start = element.find(&marker)? + marker.len();
    let end = start + element[start..].find('"')?;
    Some(&element[start..end])
}

fn slide_part(handler: &PptxHandler, index: usize) -> Result<String> {
    let presentation = handler.raw("ppt/presentation.xml", RawOptions::default())?;
    let entries = presentation
        .split("<p:sldId ")
        .skip(1)
        .filter_map(|entry| entry.split("/>").next())
        .collect::<Vec<_>>();
    let relationship_id = entries
        .get(index - 1)
        .and_then(|entry| xml_string_attr(entry, "r:id"))
        .ok_or_else(|| anyhow!("slide {index} has no presentation relationship"))?;
    let relationships = handler.raw("ppt/_rels/presentation.xml.rels", RawOptions::default())?;
    for relationship in relationships.split("<Relationship ").skip(1) {
        let relationship = relationship.split("/>").next().unwrap_or(relationship);
        if xml_string_attr(relationship, "Id") == Some(relationship_id) {
            let target = xml_string_attr(relationship, "Target")
                .ok_or_else(|| anyhow!("slide {index} relationship has no target"))?;
            return Ok(if target.starts_with("/ppt/") {
                target.trim_start_matches('/').to_owned()
            } else {
                format!("ppt/{}", target.trim_start_matches('/'))
            });
        }
    }
    bail!("slide {index} relationship target was not found")
}

fn physical_slide_path(handler: &PptxHandler, path: &str) -> Result<String> {
    let logical_index = slide_index(path)?;
    let part = slide_part(handler, logical_index)?;
    let physical_index = part
        .strip_prefix("ppt/slides/slide")
        .and_then(|value| value.strip_suffix(".xml"))
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| anyhow!("unsupported slide part target `{part}`"))?;
    Ok(format!("/slide[{physical_index}]"))
}

fn physical_shape_path(handler: &PptxHandler, path: &str) -> Result<String> {
    if !path.contains("/shape[") {
        return Ok(path.to_owned());
    }
    let logical_index = slide_index(path)?;
    let physical_slide = physical_slide_path(handler, path)?;
    Ok(path.replacen(&format!("/slide[{logical_index}]"), &physical_slide, 1))
}

fn ensure_drawing_namespace(handler: &PptxHandler, slide_path: &str) -> Result<()> {
    let part = slide_part(handler, slide_index(slide_path)?)?;
    let mut xml = handler.raw(&part, RawOptions::default())?;
    let root_start = xml
        .find("<p:sld")
        .ok_or_else(|| anyhow!("slide part `{part}` has no p:sld root"))?;
    let root_end = root_start
        + xml[root_start..]
            .find('>')
            .ok_or_else(|| anyhow!("slide part `{part}` has a malformed p:sld root"))?;
    let mut declarations = String::new();
    for (prefix, namespace) in [
        ("a", "http://schemas.openxmlformats.org/drawingml/2006/main"),
        (
            "r",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        ),
    ] {
        if !xml[root_start..root_end].contains(&format!("xmlns:{prefix}=")) {
            declarations.push_str(&format!(" xmlns:{prefix}=\"{namespace}\""));
        }
    }
    if !declarations.is_empty() {
        xml.insert_str(root_end, &declarations);
        replace_slide_document(handler, &part, &xml)?;
    }
    Ok(())
}

fn rewrite_slide_id_list(
    handler: &PptxHandler,
    edit: impl FnOnce(&mut Vec<String>) -> Result<()>,
) -> Result<()> {
    let part = "ppt/presentation.xml";
    let xml = handler.raw(part, RawOptions::default())?;
    let list_start = xml
        .find("<p:sldIdLst")
        .ok_or_else(|| anyhow!("presentation has no slide ID list"))?;
    let open_end = xml[list_start..]
        .find('>')
        .map(|offset| list_start + offset + 1)
        .ok_or_else(|| anyhow!("malformed slide ID list"))?;
    let close_start = xml[open_end..]
        .find("</p:sldIdLst>")
        .map(|offset| open_end + offset)
        .ok_or_else(|| anyhow!("malformed slide ID list"))?;
    let mut entries = Vec::new();
    let mut cursor = open_end;
    while let Some(relative) = xml[cursor..close_start].find("<p:sldId ") {
        let start = cursor + relative;
        let end = xml[start..close_start]
            .find("/>")
            .map(|offset| start + offset + 2)
            .ok_or_else(|| anyhow!("malformed slide ID entry"))?;
        entries.push(xml[start..end].to_owned());
        cursor = end;
    }
    edit(&mut entries)?;
    let replacement = format!("<p:sldIdLst>{}</p:sldIdLst>", entries.join(""));
    handler.raw_set(
        part,
        "/presentation/sldIdLst",
        "replace",
        Some(&replacement),
    )?;
    Ok(())
}

fn last_shape_path(handler: &PptxHandler, slide_path: &str) -> Option<String> {
    handler
        .view_as_outline_json()
        .ok()?
        .get("slides")?
        .as_array()?
        .iter()
        .find(|slide| slide.get("path").and_then(serde_json::Value::as_str) == Some(slide_path))?
        .get("shapes")?
        .as_array()?
        .last()?
        .get("path")?
        .as_str()
        .map(str::to_owned)
}

fn resolve_mutation_selectors(outline: &serde_json::Value, op: &mut DeckMutation) -> Result<()> {
    let resolve = |value: &mut String| -> Result<()> {
        if !value.starts_with('/') {
            *value = path_for_stable_id(outline, value)
                .ok_or_else(|| anyhow!("stable element ID `{value}` was not found"))?;
        }
        Ok(())
    };
    match op {
        DeckMutation::Set { path, .. } | DeckMutation::Remove { path } => resolve(path)?,
        DeckMutation::Move {
            source,
            target_parent,
            ..
        } => {
            resolve(source)?;
            if let Some(parent) = target_parent {
                resolve(parent)?;
            }
        }
        DeckMutation::Copy {
            source,
            target_parent,
            ..
        } => {
            resolve(source)?;
            resolve(target_parent)?;
        }
        DeckMutation::Swap { left, right } => {
            resolve(left)?;
            resolve(right)?;
        }
        DeckMutation::Add { parent, .. } => resolve(parent)?,
        DeckMutation::RawSet { .. } => {}
    }
    Ok(())
}

fn stable_ids_for_mutation(outline: &serde_json::Value, op: &DeckMutation) -> Vec<String> {
    let paths: Vec<&str> = match op {
        DeckMutation::Set { path, .. } | DeckMutation::Remove { path } => vec![path],
        DeckMutation::Move { source, .. } | DeckMutation::Copy { source, .. } => vec![source],
        DeckMutation::Swap { left, right } => vec![left, right],
        DeckMutation::Add { .. } | DeckMutation::RawSet { .. } => vec![],
    };
    paths
        .into_iter()
        .filter_map(|path| stable_id_for_path(outline, path))
        .collect()
}

fn slides(outline: &serde_json::Value) -> impl Iterator<Item = &serde_json::Value> {
    outline
        .get("slides")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
}

fn stable_id_for_added_element(
    before: &serde_json::Value,
    after: &serde_json::Value,
    parent: &str,
    element_type: &str,
) -> Option<String> {
    if !matches!(element_type, "image" | "picture" | "img") {
        return None;
    }
    let previous_ids = slides(before)
        .find(|slide| slide.get("path").and_then(serde_json::Value::as_str) == Some(parent))
        .and_then(|slide| slide.get("shapes"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|element| {
            element
                .get("path")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|path| path.contains("/picture["))
        })
        .filter_map(|element| element.get("id").and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>();
    let slide = slides(after)
        .find(|slide| slide.get("path").and_then(serde_json::Value::as_str) == Some(parent))?;
    let picture = slide
        .get("shapes")?
        .as_array()?
        .iter()
        .rev()
        .find(|element| {
            let is_picture = element
                .get("path")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|path| path.contains("/picture["));
            let is_new = element
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|id| !previous_ids.contains(&id));
            is_picture && is_new
        })?;
    stable_id_for_path(after, picture.get("path")?.as_str()?)
}

fn stable_id_for_path(outline: &serde_json::Value, path: &str) -> Option<String> {
    for slide in slides(outline) {
        let slide_path = slide.get("path")?.as_str()?;
        let slide_id = slide.get("slide_id")?.as_str()?;
        if path == slide_path {
            return Some(format!("slide:{slide_id}"));
        }
        for shape in slide
            .get("shapes")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            if shape.get("path")?.as_str()? == path {
                let kind = if path.contains("/picture[") {
                    "picture"
                } else {
                    "shape"
                };
                return Some(format!(
                    "slide:{slide_id}/{kind}:{}",
                    shape.get("id")?.as_str()?
                ));
            }
        }
    }
    None
}

fn path_for_stable_id(outline: &serde_json::Value, stable_id: &str) -> Option<String> {
    for slide in slides(outline) {
        let slide_id = slide.get("slide_id")?.as_str()?;
        if stable_id == format!("slide:{slide_id}") {
            return slide.get("path")?.as_str().map(str::to_owned);
        }
        for kind in ["shape", "picture"] {
            let prefix = format!("slide:{slide_id}/{kind}:");
            if let Some(element_id) = stable_id.strip_prefix(&prefix) {
                return slide
                    .get("shapes")?
                    .as_array()?
                    .iter()
                    .find(|shape| {
                        shape.get("id").and_then(serde_json::Value::as_str) == Some(element_id)
                            && shape
                                .get("path")
                                .and_then(serde_json::Value::as_str)
                                .is_some_and(|path| {
                                    (kind == "picture") == path.contains("/picture[")
                                })
                    })?
                    .get("path")?
                    .as_str()
                    .map(str::to_owned);
            }
        }
    }
    None
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
    #[test]
    fn stable_id_resolution_tracks_reordered_paths() {
        let outline = serde_json::json!({"slides": [
            {"path":"/slide[1]", "slide_id":"300", "shapes":[
                {"path":"/slide[1]/shape[1]", "id":"7"}
            ]}
        ]});
        assert_eq!(
            stable_id_for_path(&outline, "/slide[1]/shape[1]").as_deref(),
            Some("slide:300/shape:7")
        );
        assert_eq!(
            path_for_stable_id(&outline, "slide:300/shape:7").as_deref(),
            Some("/slide[1]/shape[1]")
        );
    }

    #[tokio::test]
    async fn malformed_xml_mutation_is_rolled_back_without_advancing_generation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("malformed.pptx");
        let engine = DeckEngine::create(&path, None).await.unwrap();
        let before = std::fs::read(&path).unwrap();
        let generation = engine.generation();

        let result = engine
            .mutate(DeckMutation::RawSet {
                part: "ppt/slides/slide1.xml".into(),
                xpath: "/sld/cSld".into(),
                action: "replace".into(),
                xml: Some("<p:cSld><unclosed></p:cSld>".into()),
            })
            .await;

        assert!(result.is_err());
        assert_eq!(engine.generation(), generation);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        engine.snapshot().await.unwrap();
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

#[cfg(test)]
#[path = "deck_engine_inspection_tests.rs"]
mod inspection_tests;
