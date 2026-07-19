use super::{
    open, reject_part,
    state::{
        resolve_mutation_selectors, stable_id_for_path, stable_ids_added, stable_ids_for_mutation,
    },
    DeckMutation,
};
use crate::agent::inspection_geometry;
use anyhow::{anyhow, bail, Context, Result};
use handler_common::{
    output_format::{RawOptions, ViewOptions},
    DocumentHandler, InsertPosition,
};
use pptx_handler::PptxHandler;
use std::{collections::HashMap, path::Path};

pub(super) struct TransactionResult {
    pub(super) affected: Vec<String>,
    pub(super) post_state: serde_json::Value,
}

pub(super) fn transact(path: &Path, ops: Vec<DeckMutation>) -> Result<TransactionResult> {
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
        let mut current_state = handler.view_as_outline_json()?;
        let initial_html = handler.view_as_html(ViewOptions::default())?;
        inspection_geometry::enrich(&mut current_state, &initial_html);
        let mut affected = Vec::new();

        for (index, mut op) in ops.into_iter().enumerate() {
            resolve_mutation_selectors(&current_state, &mut op)?;
            let result_op = op.clone();
            let use_pre_mutation_ids = matches!(
                op,
                DeckMutation::Set { .. }
                    | DeckMutation::Remove { .. }
                    | DeckMutation::Move { .. }
                    | DeckMutation::Swap { .. }
            );
            let stable_before = stable_ids_for_mutation(&current_state, &op);
            let handler_affected = apply(&handler, op)?;
            let errors = handler.validate()?;
            if !errors.is_empty() {
                bail!(
                    "mutation {} validation failed; original unchanged: {errors:?}",
                    index + 1
                );
            }
            let mut post_state = handler.view_as_outline_json()?;
            // The pinned handler omits pictures from its outline. Render only when
            // pictures are present or newly added so intermediate selectors and
            // canonical Add deltas retain them. Shape-only batches avoid per-edit
            // full-deck renders.
            if state_contains_pictures(&current_state) || mutation_adds_picture(&result_op) {
                let html = handler
                    .view_as_html(ViewOptions::default())
                    .with_context(|| {
                        format!(
                            "mutation {} produced invalid slide XML; original unchanged",
                            index + 1
                        )
                    })?;
                inspection_geometry::enrich(&mut post_state, &html);
            }
            let stable_after = if let DeckMutation::Add {
                parent,
                element_type,
                ..
            } = &result_op
            {
                stable_ids_added(&current_state, &post_state, parent, element_type)
            } else {
                handler_affected
                    .iter()
                    .filter_map(|path| stable_id_for_path(&post_state, path))
                    .collect::<Vec<_>>()
            };
            if matches!(result_op, DeckMutation::Add { .. }) && stable_after.is_empty() {
                bail!(
                    "mutation {} created no inspectable element; original unchanged",
                    index + 1
                );
            }
            affected.extend(if use_pre_mutation_ids || stable_after.is_empty() {
                stable_before
            } else {
                stable_after
            });
            current_state = post_state;
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
        drop(verified);
        std::fs::rename(&temp, path)?;
        Ok(TransactionResult {
            affected,
            post_state,
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn mutation_adds_picture(op: &DeckMutation) -> bool {
    matches!(
        op,
        DeckMutation::Add { element_type, .. } if mutation_element_is_picture(element_type)
    )
}

fn mutation_element_is_picture(element_type: &str) -> bool {
    matches!(element_type, "image" | "picture" | "img")
}

fn state_contains_pictures(state: &serde_json::Value) -> bool {
    state["slides"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|slide| slide["shapes"].as_array().into_iter().flatten())
        .any(|element| {
            element["path"]
                .as_str()
                .is_some_and(|path| path.contains("/picture["))
        })
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
            if mutation_element_is_picture(&element_type) {
                // Pictures are absent from the pinned handler's outline, and its
                // reported shape path can point past the inserted element. The
                // transaction derives the authoritative picture ID from its state
                // delta, so do not use that path for shape-only post-processing.
                vec![reported]
            } else {
                // Shape adders in the pinned revision calculate the returned index
                // after insertion and are therefore one past the actual shape.
                let added = if element_type == "slide" {
                    reported
                } else {
                    last_shape_path(handler, &parent).unwrap_or(reported)
                };
                if let Some(preset) = properties.get("preset") {
                    set_shape_preset(handler, &added, preset)?;
                }
                // The pinned handler applies geometry while creating rectangles, but
                // only applies text-run formatting through `set`.
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
