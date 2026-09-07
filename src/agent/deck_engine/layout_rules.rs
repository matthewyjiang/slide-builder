//! Retain declared relationships for audit, without inferring layout intent from proximity.
use super::{
    layout::State,
    layout_contract::{emu, Metadata, Rect},
    layout_plan::{self, Axis, Dimension, Edge, Operation, Plan},
};
use anyhow::Result;
use serde_json::{json, Value};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    X,
    Y,
    Width,
    Height,
}

fn fields(operation: &Operation) -> &'static [Field] {
    match operation {
        Operation::Align { edge, .. } => match edge {
            Edge::Left | Edge::Right | Edge::CenterX => &[Field::X],
            Edge::Top | Edge::Bottom | Edge::CenterY => &[Field::Y],
        },
        Operation::Distribute { axis, .. } => match axis {
            Axis::Horizontal => &[Field::X],
            Axis::Vertical => &[Field::Y],
        },
        Operation::MatchSize { dimension, .. } => match dimension {
            Dimension::Width => &[Field::Width],
            Dimension::Height => &[Field::Height],
            Dimension::Both => &[Field::Width, Field::Height],
        },
        Operation::Place { .. } => &[Field::X, Field::Y, Field::Width, Field::Height],
        Operation::TextStyle { .. } | Operation::Release { .. } => &[],
    }
}

fn reference(operation: &Operation) -> Option<&str> {
    match operation {
        Operation::Align { reference, .. } => reference.as_deref(),
        Operation::MatchSize { reference, .. } => Some(reference),
        Operation::Distribute { .. }
        | Operation::Place { .. }
        | Operation::TextStyle { .. }
        | Operation::Release { .. } => None,
    }
}

/// The most recent operation owns each controlled coordinate on overlapping selections.
/// Reference-only changes do not replace a rule; the audit should detect the resulting drift.
pub(super) fn record(metadata: &mut Metadata, operation: &Operation) {
    match operation {
        Operation::TextStyle { ids, style } => {
            for id in ids {
                metadata.assignments.insert(id.clone(), style.clone());
            }
        }
        Operation::Release { ids } => {
            metadata.assignments.retain(|id, _| !ids.contains(id));
            metadata.geometry_rules.retain(|rule| {
                !rule.ids().iter().any(|id| ids.contains(id))
                    && !reference(rule).is_some_and(|id| ids.iter().any(|selected| selected == id))
            });
        }
        Operation::Align { .. }
        | Operation::Distribute { .. }
        | Operation::MatchSize { .. }
        | Operation::Place { .. } => {
            metadata.geometry_rules.retain(|previous| {
                !previous.ids().iter().any(|id| operation.ids().contains(id))
                    || !fields(previous)
                        .iter()
                        .any(|field| fields(operation).contains(field))
            });
            metadata.geometry_rules.push(operation.clone());
        }
    }
}

fn differences(actual: Rect, expected: Rect) -> Result<Vec<Value>> {
    let mut differences = vec![];
    for (property, actual, expected) in [
        ("x", actual.x, expected.x),
        ("y", actual.y, expected.y),
        ("width", actual.width, expected.width),
        ("height", actual.height, expected.height),
    ] {
        // A target may fall between integer EMUs. Replanning an evenly divided
        // region from rounded coordinates can differ by one EMU, the file quantum.
        if (i128::from(emu(actual)?) - i128::from(emu(expected)?)).abs() > 1 {
            differences
                .push(json!({"property":property,"actual":actual,"expected":expected,"unit":"in"}));
        }
    }
    Ok(differences)
}

pub(super) fn audit(state: &State, issues: &mut Vec<Value>) -> Result<()> {
    for rule in &state.metadata.geometry_rules {
        match layout_plan::plan(
            rule,
            &state.items,
            state.metadata.contract.as_ref(),
            state.size,
        ) {
            Ok(Plan::Geometry(placements)) => {
                for (id, expected) in placements {
                    let differences = differences(state.items[&id].rect()?, expected)?;
                    if !differences.is_empty() {
                        issues.push(json!({"kind":"geometry_rule_departure","id":id,"rule":rule,"departures":differences}));
                    }
                }
            }
            Ok(Plan::TextStyle(_)) | Ok(Plan::Release) => {
                issues.push(json!({"kind":"invalid_geometry_rule","rule":rule}));
            }
            Err(error) => issues.push(
                json!({"kind":"geometry_rule_unavailable","rule":rule,"message":error.to_string()}),
            ),
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "layout_rules_tests.rs"]
mod tests;
