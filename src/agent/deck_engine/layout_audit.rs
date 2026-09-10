//! Mechanical checks of bounds, margins, declared geometry, explicit text styles,
//! axis-aligned overlaps, and declared text fit.
use super::{
    layout::State,
    layout_contract::{emu, Alignment, Rect, TextStyle},
    layout_xml::Element,
};
use anyhow::Result;
use serde_json::{json, Value};

#[path = "layout_audit_overlap.rs"]
mod overlap;
#[path = "layout_audit_text.rs"]
mod text;

fn departures(element: &Element, style: &TextStyle, out: &mut Vec<Value>) {
    if element.named("p") {
        let actual = element.child("pPr").and_then(|e| e.attr("algn"));
        let expected = match style.alignment {
            Alignment::Left => "l",
            Alignment::Center => "ctr",
            Alignment::Right => "r",
            Alignment::Justify => "just",
        };
        if actual.as_deref() != Some(expected) {
            out.push(json!({"property":"alignment","expected":expected,"actual":actual}));
        }
    }
    if element.named("rPr") || element.named("defRPr") || element.named("endParaRPr") {
        for (key, expected) in [
            ("sz", format!("{:.0}", style.font_size * 100.0)),
            ("b", if style.bold { "1" } else { "0" }.into()),
            ("i", if style.italic { "1" } else { "0" }.into()),
        ] {
            let actual = element.attr(key);
            if actual.as_deref() != Some(&expected) {
                out.push(json!({"property":key,"expected":expected,"actual":actual}));
            }
        }
        for name in ["latin", "ea", "cs"] {
            let actual = element.child(name).and_then(|e| e.attr("typeface"));
            if actual.as_deref() != Some(&style.font_family) {
                out.push(json!({"property":format!("font_family:{name}"),"expected":style.font_family,"actual":actual}));
            }
        }
        let actual = element
            .child("solidFill")
            .and_then(|e| e.child("srgbClr"))
            .and_then(|e| e.attr("val"));
        if actual.as_deref().map(str::to_ascii_uppercase).as_deref() != Some(&style.color) {
            out.push(json!({"property":"color","expected":style.color,"actual":actual}));
        }
    }
    if (element.named("r") || element.named("fld") || element.named("br"))
        && element.child("rPr").is_none()
    {
        out.push(
            json!({"property":"run_properties","expected":"explicit named style","actual":null}),
        );
    }
    for child in element.elements() {
        departures(child, style, out);
    }
}

pub(super) fn audit(state: &State) -> Result<Value> {
    let mut issues = vec![];
    let mut warnings = vec![];
    if let Some(c) = &state.metadata.contract {
        if let Err(error) = c.clone().normalize(state.size) {
            issues.push(json!({"kind":"invalid_contract","message":error.to_string()}));
        }
    }
    for item in state.items.values() {
        match item.rect() {
            Ok(r) => {
                if let Err(error) = r.validate(state.size) {
                    issues.push(json!({"kind":"slide_bounds_departure","id":item.id,"geometry":r,"message":error.to_string()}));
                }
                if let Some(c) = &state.metadata.contract {
                    let outside = emu(r.x)? < emu(c.margins.left)?
                        || emu(r.y)? < emu(c.margins.top)?
                        || emu(r.x + r.width)? > emu(state.size.0 - c.margins.right)?
                        || emu(r.y + r.height)? > emu(state.size.1 - c.margins.bottom)?;
                    if outside {
                        issues.push(json!({"kind":"margin_departure","id":item.id,"geometry":r}));
                    }
                }
            }
            Err(e) => issues
                .push(json!({"kind":"geometry_unavailable","id":item.id,"message":e.to_string()})),
        }
    }
    for (id, name) in &state.metadata.assignments {
        let Some(item) = state.items.get(id) else {
            issues.push(json!({"kind":"missing_assigned_element","id":id,"style":name}));
            continue;
        };
        let Some(style) = state
            .metadata
            .contract
            .as_ref()
            .and_then(|c| c.text_styles.get(name))
        else {
            issues.push(json!({"kind":"missing_assigned_style","id":id,"style":name}));
            continue;
        };
        let Some(body) = item.xml.child("txBody") else {
            issues.push(json!({"kind":"nontext_assignment","id":id,"style":name}));
            continue;
        };
        let mut differences = vec![];
        departures(body, style, &mut differences);
        if !differences.is_empty() {
            issues.push(json!({"kind":"text_style_departure","id":id,"style":name,"departures":differences}));
        }
    }
    super::layout_rules::audit(state, &mut issues)?;
    overlap::audit(state, &mut warnings)?;
    text::audit(state, &mut warnings)?;
    Ok(json!({
        "valid":issues.is_empty(),"issues":issues,"warnings":warnings,
        "contract_present":state.metadata.contract.is_some(),
        "slide_size":super::layout::size_value(state.size),
        "checks":["slide_bounds","margins","assigned_text_styles","declared_geometry_rules","element_overlap","text_fit"],
        "limitations":[
            "Direct slide shapes and pictures only; grouped elements and graphic frames are not inspected.",
            "Text checks compare explicit DrawingML properties, not theme-inherited appearance.",
            "Geometry checks use unrotated transform rectangles. Rotated shapes are omitted from overlap and text-fit warnings because those rectangles are not visual bounds.",
            "Overlap warnings are axis-aligned intersections, not proven collisions. Text on a filled or picture background is ignored when contained. Intentional stacking such as shadows is not distinguished from accidental overlap.",
            "Text-fit warnings compare declared font size times explicit paragraph or line breaks with inner box height. That is not line height. Wrapping, glyph width, kerning, columns, non-horizontal text, text rotation, autofit, custom line spacing, and font substitution are not measured.",
            "valid reflects saved bounds, margins, styles, and geometry rules only. Overlap and text-fit findings are warnings. Rendered review is still required."
        ]
    }))
}

fn bounds(rect: Rect) -> Result<Bounds> {
    Ok(Bounds {
        x: emu(rect.x)?,
        y: emu(rect.y)?,
        width: emu(rect.width)?,
        height: emu(rect.height)?,
    })
}

#[derive(Clone, Copy)]
struct Bounds {
    x: i64,
    y: i64,
    width: i64,
    height: i64,
}

impl Bounds {
    fn right(self) -> i128 {
        i128::from(self.x) + i128::from(self.width)
    }
    fn bottom(self) -> i128 {
        i128::from(self.y) + i128::from(self.height)
    }
    fn intersects(self, other: Self) -> bool {
        i128::from(self.x) < other.right()
            && i128::from(other.x) < self.right()
            && i128::from(self.y) < other.bottom()
            && i128::from(other.y) < self.bottom()
    }
    fn contains(self, other: Self) -> bool {
        i128::from(self.x) <= i128::from(other.x)
            && i128::from(self.y) <= i128::from(other.y)
            && self.right() >= other.right()
            && self.bottom() >= other.bottom()
    }
}

fn visible_text(element: &Element) -> bool {
    if element.named("t") {
        for child in &element.children {
            if let super::layout_xml::Node::Other(quick_xml::events::Event::Text(value)) = child {
                if value
                    .unescape()
                    .is_ok_and(|text| text.chars().any(|c| !c.is_whitespace()))
                {
                    return true;
                }
            }
        }
    }
    element.elements().any(visible_text)
}

fn painted(xml: &Element) -> bool {
    if xml.named("pic") {
        return true;
    }
    let Some(properties) = xml.child("spPr") else {
        return false;
    };
    if properties.child("noFill").is_some() {
        return false;
    }
    ["solidFill", "gradFill", "blipFill", "pattFill", "grpFill"]
        .into_iter()
        .any(|name| properties.child(name).is_some())
}

fn connector(xml: &Element) -> bool {
    xml.child("spPr")
        .and_then(|properties| properties.child("prstGeom"))
        .and_then(|geometry| geometry.attr("prst"))
        .is_some_and(|preset| {
            preset == "line"
                || preset.starts_with("straightConnector")
                || preset.starts_with("bentConnector")
                || preset.starts_with("curvedConnector")
        })
}

fn rotated(xml: &Element) -> bool {
    xml.child("spPr")
        .and_then(|properties| properties.child("xfrm"))
        .and_then(|transform| transform.attr("rot"))
        .and_then(|value| value.parse::<i64>().ok())
        .is_some_and(|rotation| rotation != 0)
}

#[cfg(test)]
#[path = "layout_audit_tests.rs"]
mod tests;
