use serde_json::{json, Value};
use std::collections::HashMap;

pub(super) fn enrich(inspected: &mut Value, html: &str) {
    add_missing_pictures(inspected, html);
    let geometry = geometry_by_path(html);
    add_geometry(inspected, &geometry);

    if let Some(object) = inspected.as_object_mut() {
        if let (Some(width), Some(height)) = (
            css_custom_pt(html, "--slide-design-w"),
            css_custom_pt(html, "--slide-design-h"),
        ) {
            object.insert(
                "slide_size".into(),
                json!({
                    "width": pt_to_inches(width),
                    "height": pt_to_inches(height),
                    "unit": "in"
                }),
            );
        }
    }
}

fn add_missing_pictures(inspected: &mut Value, html: &str) {
    let picture_paths = geometry_by_path(html)
        .into_keys()
        .filter(|path| path.contains("/picture["))
        .collect::<Vec<_>>();
    let Some(slides) = inspected.get_mut("slides").and_then(Value::as_array_mut) else {
        return;
    };
    for slide in slides {
        let Some(slide_path) = slide.get("path").and_then(Value::as_str) else {
            continue;
        };
        let prefix = format!("{slide_path}/picture[");
        let pictures = picture_paths
            .iter()
            .filter(|path| path.starts_with(&prefix))
            .filter_map(|path| {
                let id = path.strip_prefix(&prefix)?.strip_suffix(']')?;
                Some(json!({
                    "path": path,
                    "type": "image",
                    "name": "Picture",
                    "id": id,
                    "paragraph_count": 0,
                    "text_preview": ""
                }))
            })
            .collect::<Vec<_>>();
        let Some(object) = slide.as_object_mut() else {
            continue;
        };
        let shapes = object
            .entry("shapes")
            .or_insert_with(|| Value::Array(Vec::new()));
        let count = if let Some(shapes) = shapes.as_array_mut() {
            for picture in pictures {
                let path = picture.get("path").and_then(Value::as_str);
                if !shapes
                    .iter()
                    .any(|shape| shape.get("path").and_then(Value::as_str) == path)
                {
                    shapes.push(picture);
                }
            }
            Some(shapes.len())
        } else {
            None
        };
        if let Some(count) = count {
            object.insert("shape_count".into(), json!(count));
        }
    }
}

fn add_geometry(value: &mut Value, geometry: &HashMap<String, Value>) {
    match value {
        Value::Object(object) => {
            if let Some(path) = object.get("path").and_then(Value::as_str) {
                if let Some(shape_geometry) = geometry.get(path) {
                    object.insert("geometry".into(), shape_geometry.clone());
                }
            }
            for child in object.values_mut() {
                add_geometry(child, geometry);
            }
        }
        Value::Array(values) => {
            for child in values {
                add_geometry(child, geometry);
            }
        }
        _ => {}
    }
}

fn geometry_by_path(html: &str) -> HashMap<String, Value> {
    const PATH_ATTRIBUTE: &str = "data-path=\"";
    let mut result = HashMap::new();
    let mut cursor = 0;
    while let Some(relative) = html[cursor..].find(PATH_ATTRIBUTE) {
        let attribute_start = cursor + relative;
        let value_start = attribute_start + PATH_ATTRIBUTE.len();
        let Some(value_end_relative) = html[value_start..].find('"') else {
            break;
        };
        let value_end = value_start + value_end_relative;
        let path = &html[value_start..value_end];
        let Some(tag_start) = html[..attribute_start].rfind('<') else {
            cursor = value_end + 1;
            continue;
        };
        let Some(tag_end_relative) = html[value_end..].find('>') else {
            break;
        };
        let tag_end = value_end + tag_end_relative;
        let tag = &html[tag_start..=tag_end];
        if let Some(style) = html_attribute(tag, "style") {
            if let (Some(x), Some(y), Some(width), Some(height)) = (
                css_pt(style, "left"),
                css_pt(style, "top"),
                css_pt(style, "width"),
                css_pt(style, "height"),
            ) {
                result.insert(
                    path.to_owned(),
                    json!({
                        "x": pt_to_inches(x),
                        "y": pt_to_inches(y),
                        "width": pt_to_inches(width),
                        "height": pt_to_inches(height),
                        "unit": "in"
                    }),
                );
            }
        }
        cursor = tag_end + 1;
    }
    result
}

fn html_attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("{name}=\"");
    let start = tag.find(&marker)? + marker.len();
    let end = start + tag[start..].find('"')?;
    Some(&tag[start..end])
}

fn css_pt(style: &str, property: &str) -> Option<f64> {
    style.split(';').find_map(|declaration| {
        let (name, value) = declaration.split_once(':')?;
        if name.trim() != property {
            return None;
        }
        value.trim().strip_suffix("pt")?.parse().ok()
    })
}

fn css_custom_pt(html: &str, property: &str) -> Option<f64> {
    let start = html.find(property)? + property.len();
    let value = html[start..].trim_start().strip_prefix(':')?.trim_start();
    let end = value.find("pt")?;
    value[..end].trim().parse().ok()
}

fn pt_to_inches(points: f64) -> f64 {
    ((points / 72.0) * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
#[path = "inspection_geometry_tests.rs"]
mod tests;
