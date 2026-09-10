//! Text-fit warnings from declared font size and explicit breaks, not wrapping.
use super::super::{layout::State, layout_contract::emu, layout_xml::Element};
use super::{rotated, visible_text};
use anyhow::Result;
use serde_json::{json, Value};

/// Warn when explicit lines times declared font size exceed inner height.
/// Skip directions, columns, autofit, rotation, and custom spacing; those are unmeasured.
pub(super) fn audit(state: &State, warnings: &mut Vec<Value>) -> Result<()> {
    for item in state.items.values() {
        let Ok(rect) = item.rect() else {
            continue;
        };
        if rotated(&item.xml) {
            continue;
        }
        let Some(body) = item.xml.child("txBody") else {
            continue;
        };
        if !visible_text(body) || unsupported_text_layout(body) {
            continue;
        }
        let Some((required, lines)) = required_height_emu(body)? else {
            continue;
        };
        let body_properties = body.child("bodyPr");
        let inset = |name| -> Result<i64> {
            match body_properties.and_then(|properties| properties.attr(name)) {
                Some(value) => Ok(value.parse::<i64>()?),
                None => Ok(0),
            }
        };
        // Missing insets are treated as zero so theme defaults cannot invent a warning.
        let available =
            i128::from(emu(rect.height)?) - i128::from(inset("tIns")?) - i128::from(inset("bIns")?);
        if i128::from(required) <= available {
            continue;
        }
        warnings.push(json!({
            "kind":"possible_text_overflow",
            "id":item.id,
            "geometry":rect,
            "explicit_lines":lines,
            "estimated_height":required as f64 / 914_400.0,
            "available_height":available as f64 / 914_400.0,
            "unit":"in",
            "basis":"declared_font_size_and_explicit_breaks"
        }));
    }
    Ok(())
}

fn unsupported_text_layout(body: &Element) -> bool {
    if body.descendant("lnSpc").is_some() {
        return true;
    }
    let Some(properties) = body.child("bodyPr") else {
        return false;
    };
    if properties.child("normAutofit").is_some() || properties.child("spAutoFit").is_some() {
        return true;
    }
    if properties
        .attr("numCol")
        .and_then(|value| value.parse::<i64>().ok())
        .is_some_and(|columns| columns > 1)
    {
        return true;
    }
    if properties
        .attr("rot")
        .and_then(|value| value.parse::<i64>().ok())
        .is_some_and(|rotation| rotation != 0)
    {
        return true;
    }
    if properties.attr("vert").is_some_and(|value| value != "horz") {
        return true;
    }
    false
}

fn required_height_emu(body: &Element) -> Result<Option<(i64, usize)>> {
    let mut total = 0_i128;
    let mut lines = 0_usize;
    let mut measured = false;
    for paragraph in body.elements().filter(|element| element.named("p")) {
        let paragraph_fallback = paragraph
            .child("pPr")
            .and_then(|properties| properties.child("defRPr"))
            .and_then(font_size_points);
        let mut line_size: Option<f64> = None;
        let mut flush_line = |size: Option<f64>| -> Result<()> {
            lines += 1;
            let Some(size) = size.or(paragraph_fallback) else {
                return Ok(());
            };
            measured = true;
            total += i128::from(emu(size / 72.0)?);
            Ok(())
        };
        for child in paragraph.elements() {
            if child.named("r") || child.named("fld") {
                let size = font_size_points(child).or(paragraph_fallback);
                if let Some(size) = size {
                    line_size = Some(line_size.map_or(size, |current| current.max(size)));
                }
                // Native text additions retain newlines inside a:t rather than
                // emitting separate a:br nodes. Count those breaks as well.
                if let Some(text) = child.child("t") {
                    for node in &text.children {
                        if let super::super::layout_xml::Node::Other(
                            quick_xml::events::Event::Text(value),
                        ) = node
                        {
                            for _ in value.unescape()?.matches('\n') {
                                flush_line(line_size)?;
                                line_size = size;
                            }
                        }
                    }
                }
            } else if child.named("br") {
                flush_line(line_size)?;
                line_size = child.child("rPr").and_then(font_size_points);
            }
        }
        if line_size.is_none() {
            line_size = paragraph.child("endParaRPr").and_then(font_size_points);
        }
        flush_line(line_size)?;
    }
    if !measured {
        return Ok(None);
    }
    Ok(Some((i64::try_from(total)?, lines)))
}

fn font_size_points(element: &Element) -> Option<f64> {
    let properties =
        if element.named("rPr") || element.named("defRPr") || element.named("endParaRPr") {
            element
        } else {
            element.child("rPr")?
        };
    let size = properties.attr("sz")?.parse::<i64>().ok()?;
    (size > 0).then_some(size as f64 / 100.0)
}
