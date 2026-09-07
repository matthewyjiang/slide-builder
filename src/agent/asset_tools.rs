//! Slide-aware asset tools. SVG authoring stays with the model; storage and gates are native.
use super::{
    assets::{validate_and_render, AssetRecord, AssetStore, CreateAsset, ValidatedSvg},
    deck_engine::DeckEngine,
};
use anyhow::{bail, Result};
use rho_sdk::{
    model::ToolSpec,
    tool::{
        OperationKind, Tool, ToolAsset, ToolContext, ToolError, ToolErrorKind, ToolFuture,
        ToolInvocation, ToolMetadata, ToolOutput,
    },
    CapabilityRequest, CapabilitySource, PathScope,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

#[path = "asset_tool_schema.rs"]
mod schema;

#[derive(Clone, Copy)]
enum Action {
    Create,
    List,
    Inspect,
    Place,
}

impl Action {
    fn name(self) -> &'static str {
        match self {
            Self::Create => "asset_create_svg",
            Self::List => "asset_list",
            Self::Inspect => "asset_inspect",
            Self::Place => "asset_place",
        }
    }
}

struct AssetTool {
    action: Action,
    engine: DeckEngine,
}

pub(super) fn tools(engine: DeckEngine) -> Vec<Arc<dyn Tool>> {
    [Action::Create, Action::List, Action::Inspect, Action::Place]
        .into_iter()
        .map(|action| {
            Arc::new(AssetTool {
                action,
                engine: engine.clone(),
            }) as Arc<dyn Tool>
        })
        .collect()
}

impl Tool for AssetTool {
    fn spec(&self) -> ToolSpec {
        schema::spec(self.action)
    }

    fn start_metadata(&self, _: &Value) -> ToolMetadata {
        let store = AssetStore::for_deck(self.engine.path());
        let (operation, path) = match self.action {
            Action::Create => (OperationKind::Write, store.root()),
            Action::List | Action::Inspect => (OperationKind::Read, store.root()),
            Action::Place => (OperationKind::Write, self.engine.path()),
        };
        ToolMetadata::new().operation(operation).affected_path(path)
    }

    fn call<'a>(&'a self, inv: ToolInvocation, context: ToolContext) -> ToolFuture<'a> {
        Box::pin(async move {
            let store = AssetStore::for_deck(self.engine.path());
            let target = match self.action {
                Action::Create => Some(store.root()),
                Action::Place => Some(self.engine.path()),
                Action::List | Action::Inspect => None,
            };
            if let Some(target) = target {
                context
                    .authorize(CapabilityRequest::write_path(
                        target,
                        PathScope::GrantedRoot {
                            root: self
                                .engine
                                .path()
                                .parent()
                                .expect("absolute deck path")
                                .to_owned(),
                        },
                        CapabilitySource::host_tool(self.action.name()),
                    ))
                    .await
                    .map_err(|error| ToolError::policy_denied(&error))?;
            }
            if context.cancellation().is_cancelled() {
                return Err(ToolError::cancelled());
            }
            // Do not abandon a committing write on cancellation. Its completed result must
            // accurately report whether the candidate or deck was saved.
            let (value, preview) = execute(self.action, &self.engine, inv.into_arguments())
                .await
                .map_err(execution_error)?;
            let mut metadata = self.start_metadata(&Value::Null);
            if let Some(png) = preview {
                metadata = metadata.asset(ToolAsset::new("image/png", png));
            }
            Ok(
                ToolOutput::text(serde_json::to_string_pretty(&value).map_err(execution_error)?)
                    .metadata(metadata),
            )
        })
    }
}

fn execution_error(error: impl std::fmt::Display) -> ToolError {
    ToolError::new(ToolErrorKind::Execution, error.to_string())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetId {
    id: Uuid,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Placement {
    id: Uuid,
    slide: usize,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

async fn execute(
    action: Action,
    engine: &DeckEngine,
    args: Value,
) -> Result<(Value, Option<Vec<u8>>)> {
    let store = AssetStore::for_deck(engine.path());
    match action {
        Action::Create => {
            let request: CreateAsset = serde_json::from_value(args)?;
            let (record, rendered) =
                tokio::task::spawn_blocking(move || store.create(request)).await??;
            Ok((inspection(&record, &rendered), Some(rendered.png)))
        }
        Action::List => {
            let _: Empty = serde_json::from_value(args)?;
            let assets = tokio::task::spawn_blocking(move || store.list()).await??;
            Ok((serde_json::to_value(assets)?, None))
        }
        Action::Inspect => {
            let request: AssetId = serde_json::from_value(args)?;
            let (record, rendered) = load_preview(store, request.id).await?;
            let mut result = inspection(&record, &rendered);
            result["svg"] = json!(record.svg);
            Ok((result, Some(rendered.png)))
        }
        Action::Place => {
            let placement: Placement = serde_json::from_value(args)?;
            placement.validate()?;
            let (record, rendered) = load_preview(store, placement.id).await?;
            let bounds = placement.contain(rendered.aspect_ratio);
            let properties = HashMap::from([
                ("x".into(), format!("{}in", bounds[0])),
                ("y".into(), format!("{}in", bounds[1])),
                ("width".into(), format!("{}in", bounds[2])),
                ("height".into(), format!("{}in", bounds[3])),
                (
                    "name".into(),
                    format!("{} [asset:{}]", record.name, record.id),
                ),
                ("alt".into(), record.brief.alt_text.clone()),
            ]);
            let warnings = rendered.warnings.clone();
            let result = engine
                .add_svg_picture(placement.slide, properties, rendered)
                .await?;
            Ok((
                json!({
                    "asset_id": record.id, "sha256": record.sha256, "mutation": result,
                    "placement_inches": {"x":bounds[0],"y":bounds[1],"width":bounds[2],"height":bounds[3]},
                    "warnings": warnings,
                    "visual_review": "required", "next_step": "Call render_deck and inspect the composed slide. Check crop, contrast, legibility, hierarchy, style consistency, and meaning. The asset preview alone does not approve this placement. PowerPoint export fidelity needs a target-application check."
                }),
                None,
            ))
        }
    }
}

async fn load_preview(store: AssetStore, id: Uuid) -> Result<(AssetRecord, ValidatedSvg)> {
    tokio::task::spawn_blocking(move || {
        let record = store.load(id)?;
        let rendered = validate_and_render(&record.svg)?;
        Ok((record, rendered))
    })
    .await?
}

fn inspection(record: &AssetRecord, rendered: &ValidatedSvg) -> Value {
    let mut result = record.summary();
    result["validation"] = json!({"status":"passed", "scope":"static_svg_and_visible_preview", "warnings":rendered.warnings});
    result["preview"] = json!({"width":rendered.width,"height":rendered.height,"aspect_ratio":rendered.aspect_ratio,"attached":true});
    result["review_guidance"] = json!("Inspect this preview for defects and compliance with the brief. After placement, call render_deck and inspect the composed slide. Passing validation does not establish visual quality or factual accuracy.");
    result
}

impl Placement {
    fn validate(&self) -> Result<()> {
        if self.slide == 0 {
            bail!("slide must be a positive one-based index");
        }
        if ![self.x, self.y, self.width, self.height]
            .into_iter()
            .all(f64::is_finite)
            || self.x < 0.0
            || self.y < 0.0
            || self.width <= 0.0
            || self.height <= 0.0
        {
            bail!("asset placement requires finite, nonnegative x/y and positive width/height in inches");
        }
        Ok(())
    }

    /// Center the complete artwork inside the requested box without distortion or crop.
    fn contain(&self, ratio: f64) -> [f64; 4] {
        let width = self.width.min(self.height * ratio);
        let height = width / ratio;
        [
            self.x + (self.width - width) / 2.0,
            self.y + (self.height - height) / 2.0,
            width,
            height,
        ]
    }
}

#[cfg(test)]
#[path = "asset_tools_tests.rs"]
mod tests;
