//! Deterministic selection operations. Plans use the original selection, never intermediate edits.
use super::{
    layout_contract::{emu, nonnegative, Contract, Rect},
    layout_xml::{self, Element},
    DeckMutation,
};
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Operation {
    Align {
        ids: Vec<String>,
        edge: Edge,
        #[serde(skip_serializing_if = "Option::is_none")]
        reference: Option<String>,
    },
    Distribute {
        ids: Vec<String>,
        axis: Axis,
        #[serde(skip_serializing_if = "Option::is_none")]
        gap: Option<f64>,
    },
    MatchSize {
        ids: Vec<String>,
        reference: String,
        dimension: Dimension,
    },
    Place {
        ids: Vec<String>,
        region: String,
        axis: Axis,
        #[serde(skip_serializing_if = "Option::is_none")]
        gap: Option<f64>,
    },
    TextStyle {
        ids: Vec<String>,
        style: String,
    },
    Release {
        ids: Vec<String>,
    },
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Edge {
    Left,
    Right,
    Top,
    Bottom,
    CenterX,
    CenterY,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Axis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Dimension {
    Width,
    Height,
    Both,
}
#[derive(Clone)]
pub(super) struct Item {
    pub id: String,
    pub part: String,
    pub xml_id: String,
    pub xml: Element,
}
pub(super) enum Plan {
    Geometry(Vec<(String, Rect)>),
    TextStyle(Vec<DeckMutation>),
    Release,
}
impl Item {
    pub fn rect(&self) -> Result<Rect> {
        layout_xml::rect(&self.xml)
    }
}
impl Operation {
    pub fn ids(&self) -> &[String] {
        match self {
            Self::Align { ids, .. }
            | Self::Distribute { ids, .. }
            | Self::MatchSize { ids, .. }
            | Self::Place { ids, .. }
            | Self::TextStyle { ids, .. }
            | Self::Release { ids } => ids,
        }
    }
}
fn validate_ids(ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        bail!("layout operation requires at least one element ID");
    }
    let mut seen = HashSet::new();
    for id in ids {
        if id.trim().is_empty() {
            bail!("element IDs must not be empty");
        }
        if !seen.insert(id) {
            bail!("duplicate element ID `{id}`");
        }
    }
    Ok(())
}
fn selected<'a>(ids: &[String], items: &'a BTreeMap<String, Item>) -> Result<Vec<&'a Item>> {
    validate_ids(ids)?;
    ids.iter()
        .map(|id| {
            items
                .get(id)
                .ok_or_else(|| anyhow!("stable element ID `{id}` was not found"))
        })
        .collect()
}
impl Axis {
    fn start(self, r: Rect) -> f64 {
        match self {
            Self::Horizontal => r.x,
            Self::Vertical => r.y,
        }
    }
    fn extent(self, r: Rect) -> f64 {
        match self {
            Self::Horizontal => r.width,
            Self::Vertical => r.height,
        }
    }
    fn position(self, r: &mut Rect, value: f64) {
        match self {
            Self::Horizontal => r.x = value,
            Self::Vertical => r.y = value,
        }
    }
}
pub(super) fn plan(
    op: &Operation,
    items: &BTreeMap<String, Item>,
    contract: Option<&Contract>,
    size: (f64, f64),
) -> Result<Plan> {
    if matches!(op, Operation::Release { .. }) {
        validate_ids(op.ids())?;
        return Ok(Plan::Release);
    }
    let selection = selected(op.ids(), items)?;
    if let Operation::TextStyle { style, .. } = op {
        let style = contract
            .and_then(|c| c.text_styles.get(style))
            .ok_or_else(|| anyhow!("unknown text style `{style}`"))?;
        return selection
            .iter()
            .map(|item| {
                if !item.xml.named("sp") || item.xml.child("txBody").is_none() {
                    bail!("element `{}` is not a text shape", item.id);
                }
                Ok(DeckMutation::Set {
                    path: item.id.clone(),
                    properties: style.properties(),
                })
            })
            .collect::<Result<Vec<_>>>()
            .map(Plan::TextStyle);
    }
    let part = &selection[0].part;
    if selection.iter().any(|i| &i.part != part) {
        bail!("geometry operations require elements on the same slide");
    }
    let mut rects = selection
        .iter()
        .map(|i| i.rect())
        .collect::<Result<Vec<_>>>()?;
    let reference = |id: &str| -> Result<Rect> {
        let item = items
            .get(id)
            .ok_or_else(|| anyhow!("reference ID `{id}` was not found"))?;
        if &item.part != part {
            bail!("reference must be on the selection's slide");
        }
        item.rect()
    };
    let bounds = |rects: &[Rect]| {
        let x = rects.iter().map(|r| r.x).fold(f64::INFINITY, f64::min);
        let y = rects.iter().map(|r| r.y).fold(f64::INFINITY, f64::min);
        Rect {
            x,
            y,
            width: rects
                .iter()
                .map(|r| r.x + r.width)
                .fold(f64::NEG_INFINITY, f64::max)
                - x,
            height: rects
                .iter()
                .map(|r| r.y + r.height)
                .fold(f64::NEG_INFINITY, f64::max)
                - y,
        }
    };
    match op {
        Operation::Align {
            edge,
            reference: ref_id,
            ..
        } => {
            let target = match ref_id {
                Some(id) => reference(id)?,
                None => bounds(&rects),
            };
            for r in &mut rects {
                match edge {
                    Edge::Left => r.x = target.x,
                    Edge::Right => r.x = target.x + target.width - r.width,
                    Edge::Top => r.y = target.y,
                    Edge::Bottom => r.y = target.y + target.height - r.height,
                    Edge::CenterX => r.x = target.x + (target.width - r.width) / 2.0,
                    Edge::CenterY => r.y = target.y + (target.height - r.height) / 2.0,
                }
            }
        }
        Operation::MatchSize {
            reference: ref_id,
            dimension,
            ..
        } => {
            let target = reference(ref_id)?;
            for r in &mut rects {
                match dimension {
                    Dimension::Width => r.width = target.width,
                    Dimension::Height => r.height = target.height,
                    Dimension::Both => {
                        r.width = target.width;
                        r.height = target.height;
                    }
                }
            }
        }
        Operation::Distribute { axis, gap, .. } => {
            if rects.len() < 2 {
                bail!("distribute requires at least two elements");
            }
            let mut order = (0..rects.len()).collect::<Vec<_>>();
            order.sort_by(|a, b| {
                axis.start(rects[*a])
                    .total_cmp(&axis.start(rects[*b]))
                    .then_with(|| selection[*a].id.cmp(&selection[*b].id))
            });
            let first = rects[order[0]];
            let last = rects[*order.last().unwrap()];
            let gap = match gap {
                Some(gap) => *gap,
                None => {
                    // Integer EMUs avoid rejecting zero free space through float cancellation.
                    let total = rects
                        .iter()
                        .map(|r| emu(axis.extent(*r)).map(i128::from))
                        .collect::<Result<Vec<_>>>()?
                        .into_iter()
                        .sum::<i128>();
                    let free = i128::from(emu(axis.start(last))?)
                        + i128::from(emu(axis.extent(last))?)
                        - i128::from(emu(axis.start(first))?)
                        - total;
                    free as f64 / 914_400.0 / (rects.len() - 1) as f64
                }
            };
            nonnegative(gap, "distribution gap (available free space)")?;
            let mut cursor = axis.start(first);
            for index in order {
                axis.position(&mut rects[index], cursor);
                cursor += axis.extent(rects[index]) + gap;
            }
        }
        Operation::Place {
            region, axis, gap, ..
        } => {
            let c = contract.ok_or_else(|| anyhow!("place requires a layout contract"))?;
            let target = *c
                .regions
                .get(region)
                .ok_or_else(|| anyhow!("unknown layout region `{region}`"))?;
            let gap = gap.unwrap_or(c.gutter);
            nonnegative(gap, "placement gap")?;
            let slot = (axis.extent(target) - gap * (rects.len() - 1) as f64) / rects.len() as f64;
            if !slot.is_finite() || slot <= 0.0 {
                bail!("placement leaves no positive slot space in region `{region}`");
            }
            for (index, r) in rects.iter_mut().enumerate() {
                *r = target;
                axis.position(r, axis.start(target) + index as f64 * (slot + gap));
                match axis {
                    Axis::Horizontal => r.width = slot,
                    Axis::Vertical => r.height = slot,
                }
            }
        }
        Operation::TextStyle { .. } | Operation::Release { .. } => unreachable!(),
    }
    for r in &rects {
        r.validate(size)?;
    }
    Ok(Plan::Geometry(
        selection
            .iter()
            .zip(rects)
            .map(|(item, r)| (item.id.clone(), r))
            .collect(),
    ))
}
