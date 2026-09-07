//! Mechanical checks of bounds, margins, declared geometry, and explicit text styles.
use super::{
    layout::State,
    layout_contract::{emu, Alignment, TextStyle},
    layout_xml::Element,
};
use anyhow::Result;
use serde_json::{json, Value};

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
    Ok(json!({
        "valid":issues.is_empty(),"issues":issues,
        "contract_present":state.metadata.contract.is_some(),
        "slide_size":super::layout::size_value(state.size),
        "checks":["slide_bounds","margins","assigned_text_styles","declared_geometry_rules"],
        "limitations":[
            "Direct slide shapes and pictures only; grouped elements and graphic frames are not inspected.",
            "Text checks compare explicit DrawingML properties, not theme-inherited appearance.",
            "Geometry checks use transform rectangles, not rotated visual bounds or optical alignment.",
            "Overlaps, text clipping, font availability, and visual hierarchy are not inferred."
        ]
    }))
}
