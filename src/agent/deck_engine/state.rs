use super::DeckMutation;
use anyhow::{anyhow, Result};
use std::collections::HashSet;

pub(super) fn resolve_mutation_selectors(
    outline: &serde_json::Value,
    op: &mut DeckMutation,
) -> Result<()> {
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

pub(super) fn stable_ids_for_mutation(
    outline: &serde_json::Value,
    op: &DeckMutation,
) -> Vec<String> {
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

fn elements_for_parent<'a>(
    outline: &'a serde_json::Value,
    parent: &str,
) -> Option<&'a Vec<serde_json::Value>> {
    slides(outline)
        .find(|slide| slide.get("path").and_then(serde_json::Value::as_str) == Some(parent))
        .and_then(|slide| slide.get("shapes"))
        .and_then(serde_json::Value::as_array)
}

pub(super) fn stable_ids_added(
    before: &serde_json::Value,
    after: &serde_json::Value,
    parent: &str,
    element_type: &str,
) -> Vec<String> {
    if element_type == "slide" {
        let previous = slides(before)
            .filter_map(|slide| slide.get("slide_id").and_then(serde_json::Value::as_str))
            .collect::<HashSet<_>>();
        return slides(after)
            .filter(|slide| {
                slide
                    .get("slide_id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| !previous.contains(id))
            })
            .filter_map(|slide| slide.get("path").and_then(serde_json::Value::as_str))
            .filter_map(|path| stable_id_for_path(after, path))
            .collect();
    }

    let previous = elements_for_parent(before, parent)
        .into_iter()
        .flatten()
        .filter_map(|element| element.get("id").and_then(serde_json::Value::as_str))
        .collect::<HashSet<_>>();
    elements_for_parent(after, parent)
        .into_iter()
        .flatten()
        .filter(|element| {
            element
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|id| !previous.contains(id))
        })
        .filter_map(|element| element.get("path").and_then(serde_json::Value::as_str))
        .filter_map(|path| stable_id_for_path(after, path))
        .collect()
}

pub(super) fn stable_id_for_path(outline: &serde_json::Value, path: &str) -> Option<String> {
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

pub(super) fn append_pictures_to_slide_inspection(
    inspected: &mut serde_json::Value,
    outline: &serde_json::Value,
    path: &str,
) {
    let Some(slide) = slides(outline)
        .find(|slide| slide.get("path").and_then(serde_json::Value::as_str) == Some(path))
    else {
        return;
    };
    let Some(children) = inspected
        .get_mut("children")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    children.extend(
        slide
            .get("shapes")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|element| {
                element
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|path| path.contains("/picture["))
            })
            .cloned(),
    );
    inspected["child_count"] = children.len().into();
}

pub(super) fn value_for_path<'a>(
    outline: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    for slide in slides(outline) {
        if slide.get("path").and_then(serde_json::Value::as_str) == Some(path) {
            return Some(slide);
        }
        if let Some(element) = slide
            .get("shapes")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .find(|element| element.get("path").and_then(serde_json::Value::as_str) == Some(path))
        {
            return Some(element);
        }
    }
    None
}

pub(super) fn path_for_stable_id(outline: &serde_json::Value, stable_id: &str) -> Option<String> {
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
