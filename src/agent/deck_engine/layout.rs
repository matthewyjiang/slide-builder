//! Locked snapshot/plan/commit entry points for the native layout contract.
use super::{
    commit,
    layout_contract::{Contract, Metadata, Rect},
    layout_plan::{self, Item, Operation},
    layout_xml::{self, Element, Node},
    mutation, open, BoundsCheck, DeckEngine, DeckMutation, MutationResult, MAX_TEXT_BYTES,
};
use anyhow::{anyhow, bail, Context, Result};
use handler_common::{
    output_format::{RawOptions, ViewOptions},
    DocumentHandler,
};
use pptx_handler::PptxHandler;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(super) struct State {
    pub presentation: Element,
    pub metadata: Metadata,
    pub size: (f64, f64),
    pub items: BTreeMap<String, Item>,
    pub slides: BTreeMap<String, Element>,
}
impl State {
    pub fn read(handler: &PptxHandler) -> Result<Self> {
        let presentation = layout_xml::presentation(handler)?;
        let size = layout_xml::slide_size(&presentation)?;
        let metadata = layout_xml::metadata(&presentation).with_context(|| format!(
            "invalid layout metadata in {} extension {}; inspect and repair or remove this extension with an explicit deck_advanced raw_set edit",
            layout_xml::PRESENTATION, layout_xml::NAMESPACE
        ))?;
        let outline = handler.view_as_outline_json()?;
        let mut items = BTreeMap::new();
        let mut slides = BTreeMap::new();
        for (index, slide) in outline["slides"]
            .as_array()
            .ok_or_else(|| anyhow!("outline has no slides"))?
            .iter()
            .enumerate()
        {
            let slide_id = slide["slide_id"]
                .as_str()
                .ok_or_else(|| anyhow!("slide has no stable ID"))?;
            let part = mutation::slide_part(handler, index + 1)?;
            let xml = layout_xml::parse(&handler.raw(&part, RawOptions::default())?)?;
            let tree = xml
                .child("cSld")
                .and_then(|e| e.child("spTree"))
                .ok_or_else(|| anyhow!("slide has no shape tree"))?;
            for shape in tree.elements() {
                let kind = if shape.named("sp") {
                    "shape"
                } else if shape.named("pic") {
                    "picture"
                } else {
                    continue;
                };
                let xml_id = shape
                    .descendant("cNvPr")
                    .and_then(|e| e.attr("id"))
                    .ok_or_else(|| anyhow!("element has no OOXML ID"))?;
                let id = format!("slide:{slide_id}/{kind}:{xml_id}");
                let item = Item {
                    id: id.clone(),
                    part: part.clone(),
                    xml_id,
                    xml: shape.clone(),
                };
                if items.insert(id.clone(), item).is_some() {
                    bail!("duplicate stable ID `{id}`");
                }
            }
            slides.insert(part, xml);
        }
        Ok(Self {
            presentation,
            metadata,
            size,
            items,
            slides,
        })
    }
    fn inspect(&self) -> Value {
        json!({"contract":self.metadata.contract,"assignments":self.metadata.assignments,"geometry_rules":self.metadata.geometry_rules,"slide_size":size_value(self.size),"metadata":{"part":layout_xml::PRESENTATION,"namespace":layout_xml::NAMESPACE}})
    }
}
pub(super) fn size_value(size: (f64, f64)) -> Value {
    json!({"width":size.0,"height":size.1,"unit":"in"})
}

// Replace each affected slide once, preserving the original transform attributes.
fn geometry_mutations(state: &State, placements: Vec<(String, Rect)>) -> Result<Vec<DeckMutation>> {
    let mut changed = BTreeMap::new();
    for (id, rect) in placements {
        let item = &state.items[&id];
        let slide = changed
            .entry(item.part.clone())
            .or_insert_with(|| state.slides[&item.part].clone());
        let tree = slide
            .child_mut("cSld")
            .and_then(|e| e.child_mut("spTree"))
            .ok_or_else(|| anyhow!("missing shape tree"))?;
        let target = tree
            .children
            .iter_mut()
            .find_map(|node| match node {
                Node::Element(e)
                    if e.start.local_name().as_ref() == item.xml.start.local_name().as_ref()
                        && e.descendant("cNvPr").and_then(|e| e.attr("id")).as_deref()
                            == Some(&item.xml_id) =>
                {
                    Some(e)
                }
                _ => None,
            })
            .ok_or_else(|| anyhow!("missing geometry target {id}"))?;
        layout_xml::set_rect(target, rect)?;
    }
    changed
        .into_iter()
        .map(|(part, slide)| {
            Ok(DeckMutation::RawSet {
                part,
                xpath: "/sld".into(),
                action: "replace".into(),
                xml: Some(slide.xml()?),
            })
        })
        .collect()
}
fn payload(value: &Value) -> Result<()> {
    let asked = serde_json::to_vec(value)?.len();
    if asked > MAX_TEXT_BYTES {
        bail!("layout payload byte budget: limit {MAX_TEXT_BYTES}, asked {asked}");
    }
    Ok(())
}
impl DeckEngine {
    /// Exact presentation dimensions, in inches, without renderer rounding.
    pub async fn slide_size(&self) -> Result<(f64, f64)> {
        let _guard = self.lock.lock().await;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            layout_xml::slide_size(&layout_xml::presentation(&open(&path, false)?)?)
        })
        .await?
    }
    pub async fn layout_inspect(&self) -> Result<Value> {
        let _guard = self.lock.lock().await;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || Ok(State::read(&open(&path, false)?)?.inspect()))
            .await?
    }
    /// Replace the complete contract; assignments remain declared even if a style is removed.
    pub async fn layout_set(&self, contract: Value) -> Result<Value> {
        payload(&contract)?;
        let mut contract: Contract = serde_json::from_value(contract)?;
        let guard = self.lock.clone().lock_owned().await;
        let path = self.path.clone();
        let generation = self.generation.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<Value> {
            let _guard = guard;
            let handler = open(&path, false)?;
            let mut state = State::read(&handler)?;
            drop(handler);
            contract.normalize(state.size)?;
            state.metadata.contract = Some(contract);
            let ops = vec![layout_xml::metadata_mutation(
                state.presentation.clone(),
                &state.metadata,
            )?];
            let (_, generation) = commit(&path, &generation, ops, BoundsCheck::Advanced)?;
            let mut result = state.inspect();
            result["generation"] = json!(generation);
            Ok(result)
        })
        .await??;
        Ok(result)
    }
    /// Read, plan, validate and commit under one lock. No nested mutate/inspect calls.
    pub async fn layout_apply(&self, arguments: Value) -> Result<MutationResult> {
        payload(&arguments)?;
        let op: Operation = serde_json::from_value(arguments)?;
        let guard = self.lock.clone().lock_owned().await;
        let path = self.path.clone();
        let generation = self.generation.clone();
        let (transaction, affected, generation) =
            tokio::task::spawn_blocking(move || -> Result<_> {
                let _guard = guard;
                let handler = open(&path, false)?;
                let mut state = State::read(&handler)?;
                drop(handler);
                let planned = layout_plan::plan(
                    &op,
                    &state.items,
                    state.metadata.contract.as_ref(),
                    state.size,
                )?;
                let mut ops = match planned {
                    layout_plan::Plan::TextStyle(ops) => ops,
                    layout_plan::Plan::Geometry(placements) => {
                        geometry_mutations(&state, placements)?
                    }
                    layout_plan::Plan::Release => vec![],
                };
                super::layout_rules::record(&mut state.metadata, &op);
                let affected = op
                    .ids()
                    .iter()
                    .filter(|id| state.items.contains_key(*id))
                    .cloned()
                    .collect::<Vec<_>>();
                ops.push(layout_xml::metadata_mutation(
                    state.presentation,
                    &state.metadata,
                )?);
                let (transaction, generation) =
                    commit(&path, &generation, ops, BoundsCheck::Advanced)?;
                Ok((transaction, affected, generation))
            })
            .await??;
        Ok(MutationResult {
            generation,
            affected,
            validation_errors: vec![],
            post_state: transaction.post_state,
        })
    }
    pub async fn layout_audit(&self) -> Result<Value> {
        let _guard = self.lock.lock().await;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            super::layout_audit::audit(&State::read(&open(&path, false)?)?)
        })
        .await?
    }
    /// Structural validation plus XML parsing of every XML/relationship package part.
    pub async fn validate(&self) -> Result<Value> {
        let _guard = self.lock.lock().await;
        let path = self.path.clone();
        let generation = self.generation();
        tokio::task::spawn_blocking(move || -> Result<Value> {
            let handler = open(&path, false)?;
            let mut errors = handler
                .validate()?
                .into_iter()
                .map(|e| json!({"kind":"structure","message":format!("{e:?}")}))
                .collect::<Vec<_>>();
            let file = std::fs::File::open(path.as_ref())?;
            let mut package = zip::ZipArchive::new(file)?;
            use std::io::Read;
            for i in 0..package.len() {
                let mut part = package.by_index(i)?;
                let name = part.name().to_owned();
                if name.ends_with(".xml") || name.ends_with(".rels") {
                    let mut xml = String::new();
                    match part
                        .read_to_string(&mut xml)
                        .map_err(anyhow::Error::from)
                        .and_then(|_| layout_xml::parse(&xml).map(|_| ()))
                    {
                        Ok(()) => {}
                        Err(e) => {
                            errors.push(json!({"kind":"xml","part":name,"message":e.to_string()}))
                        }
                    }
                }
            }
            if let Err(e) = handler.view_as_html(ViewOptions::default()) {
                errors.push(json!({"kind":"render_structure","message":e.to_string()}));
            }
            if let Err(e) = State::read(&handler) {
                errors.push(json!({"kind":"layout_metadata","message":e.to_string()}));
            }
            Ok(json!({"valid":errors.is_empty(),"errors":errors,"generation":generation}))
        })
        .await?
    }
}
#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
