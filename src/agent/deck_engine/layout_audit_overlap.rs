//! Axis-aligned overlap warnings. Containment of text on a background is ignored.
use super::{bounds, connector, painted, rotated, visible_text, Bounds};
use anyhow::Result;
use serde_json::{json, Value};

struct Candidate {
    id: String,
    part: String,
    geometry: Value,
    bounds: Bounds,
    has_text: bool,
    painted: bool,
    line: bool,
    rotated: bool,
}

/// Same-slide unrotated intersections only. These are not collision verdicts.
pub(super) fn audit(state: &super::super::layout::State, warnings: &mut Vec<Value>) -> Result<()> {
    let mut candidates = Vec::new();
    for item in state.items.values() {
        let Ok(rect) = item.rect() else {
            continue;
        };
        candidates.push(Candidate {
            id: item.id.clone(),
            part: item.part.clone(),
            geometry: json!(rect),
            bounds: bounds(rect)?,
            has_text: visible_text(&item.xml),
            painted: painted(&item.xml),
            line: connector(&item.xml),
            rotated: rotated(&item.xml),
        });
    }
    for (index, left) in candidates.iter().enumerate() {
        for right in &candidates[index + 1..] {
            if left.part != right.part || left.line || right.line || left.rotated || right.rotated {
                continue;
            }
            if !left.bounds.intersects(right.bounds) {
                continue;
            }
            let contained =
                left.bounds.contains(right.bounds) || right.bounds.contains(left.bounds);
            let coincident =
                left.bounds.contains(right.bounds) && right.bounds.contains(left.bounds);
            let mut ids = [left.id.as_str(), right.id.as_str()];
            ids.sort_unstable();
            let reason = if coincident {
                if left.has_text && right.has_text {
                    "coincident_bounds"
                } else {
                    continue;
                }
            } else if contained {
                if left.has_text && right.has_text {
                    "contained_text"
                } else {
                    continue;
                }
            } else if left.has_text && right.has_text {
                "partial_text_intersection"
            } else if left.painted || right.painted || left.has_text || right.has_text {
                "partial_intersection"
            } else {
                continue;
            };
            warnings.push(json!({
                "kind":"possible_overlap",
                "reason":reason,
                "ids":ids,
                "geometries":[left.geometry.clone(), right.geometry.clone()]
            }));
        }
    }
    Ok(())
}
