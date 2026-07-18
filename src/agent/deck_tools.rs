use crate::agent::deck_engine::{DeckEngine, DeckMutation, MAX_MEDIA_BYTES};
use rho_sdk::{
    model::ToolSpec,
    tool::{
        OperationKind, Tool, ToolContext, ToolError, ToolErrorKind, ToolFuture, ToolInvocation,
        ToolMetadata, ToolOutput,
    },
    CapabilityRequest, CapabilitySource, PathScope,
};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone)]
pub struct DeckTool {
    name: &'static str,
    engine: DeckEngine,
}
impl std::fmt::Debug for DeckTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeckTool")
            .field("name", &self.name)
            .finish()
    }
}
impl DeckTool {
    pub fn new(name: &'static str, engine: DeckEngine) -> Self {
        Self { name, engine }
    }
}

pub fn semantic_tools(engine: DeckEngine) -> Vec<Arc<dyn Tool>> {
    [
        "slide_create",
        "slide_duplicate",
        "slide_delete",
        "slide_reorder",
        "text_add",
        "image_add",
        "shape_add",
        "element_update",
        "deck_inspect",
        "deck_validate",
        "deck_advanced",
    ]
    .into_iter()
    .map(|n| Arc::new(DeckTool::new(n, engine.clone())) as Arc<dyn Tool>)
    .collect()
}
fn schema(name: &str) -> Value {
    match name {
        "slide_create" => json!({"type":"object","properties":{},"additionalProperties":false}),
        "slide_duplicate" | "slide_delete" => {
            json!({"type":"object","required":["index"],"properties":{"index":{"type":"integer","minimum":1}}})
        }
        "slide_reorder" => {
            json!({"type":"object","required":["from","to"],"properties":{"from":{"type":"integer","minimum":1},"to":{"type":"integer","minimum":1}}})
        }
        "text_add" => {
            json!({"type":"object","required":["slide","text","x","y","width","height"],"properties":{"slide":{"type":"integer","minimum":1},"text":{"type":"string"},"x":{"type":"number"},"y":{"type":"number"},"width":{"type":"number"},"height":{"type":"number"},"font_size":{"type":"number"}}})
        }
        "image_add" => {
            json!({"type":"object","required":["slide","path","x","y","width","height"],"properties":{"slide":{"type":"integer","minimum":1},"path":{"type":"string"},"x":{"type":"number"},"y":{"type":"number"},"width":{"type":"number"},"height":{"type":"number"}}})
        }
        "shape_add" => {
            json!({"type":"object","required":["slide","kind","x","y","width","height"],"properties":{"slide":{"type":"integer","minimum":1},"kind":{"enum":["rectangle","ellipse","line","connector"]},"x":{"type":"number"},"y":{"type":"number"},"width":{"type":"number"},"height":{"type":"number"},"fill":{"type":"string"}}})
        }
        "element_update" => {
            json!({"type":"object","required":["id","properties"],"properties":{"id":{"type":"string"},"properties":{"type":"object","additionalProperties":{"type":"string"}}}})
        }
        "deck_inspect" => json!({"type":"object","properties":{"path":{"type":"string"}}}),
        "deck_validate" => json!({"type":"object","properties":{}}),
        _ => {
            json!({"type":"object","required":["mutation"],"properties":{"mutation":{"type":"object"}}})
        }
    }
}
impl Tool for DeckTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec{name:self.name.into(),description:format!("Native PowerPoint operation `{}` on the active deck. Mutations are atomic and validated.",self.name),input_schema:schema(self.name)}
    }
    fn start_metadata(&self, _: &Value) -> ToolMetadata {
        ToolMetadata::new()
            .operation(
                if self.name.starts_with("deck_inspect") || self.name == "deck_validate" {
                    OperationKind::Read
                } else {
                    OperationKind::Write
                },
            )
            .affected_path(self.engine.path())
    }
    fn call<'a>(&'a self, inv: ToolInvocation, context: ToolContext) -> ToolFuture<'a> {
        Box::pin(async move {
            let args = inv.into_arguments();
            if self.name != "deck_inspect" && self.name != "deck_validate" {
                context
                    .authorize(CapabilityRequest::write_path(
                        self.engine.path(),
                        PathScope::GrantedRoot {
                            root: self
                                .engine
                                .path()
                                .parent()
                                .unwrap_or_else(|| std::path::Path::new("/"))
                                .to_path_buf(),
                        },
                        CapabilitySource::host_tool(self.name),
                    ))
                    .await
                    .map_err(|e| ToolError::policy_denied(&e))?;
            }
            let result = execute(self.name, &self.engine, args).await.map_err(|e| {
                ToolError::new(
                    ToolErrorKind::Execution,
                    format!("{}; deck is unchanged", e),
                )
            })?;
            Ok(
                ToolOutput::text(serde_json::to_string_pretty(&result).unwrap())
                    .metadata(self.start_metadata(&Value::Null)),
            )
        })
    }
}
async fn execute(name: &str, e: &DeckEngine, a: Value) -> anyhow::Result<Value> {
    let n = |k: &str| {
        a.get(k)
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow::anyhow!("invalid field `{k}`: expected positive integer"))
            .map(|v| v as usize)
    };
    let f = |k: &str| {
        a.get(k)
            .and_then(Value::as_f64)
            .ok_or_else(|| anyhow::anyhow!("invalid field `{k}`: expected number"))
    };
    let mutation = match name {
        "deck_inspect" => {
            return e
                .inspect(a.get("path").and_then(Value::as_str).map(str::to_owned))
                .await
        }
        "deck_validate" => {
            let s = e.snapshot().await?;
            return Ok(json!({"valid":true,"generation":s.generation,"outline":s.outline}));
        }
        "slide_create" => DeckMutation::Add {
            parent: "/presentation".into(),
            element_type: "slide".into(),
            properties: HashMap::new(),
        },
        "slide_duplicate" => DeckMutation::Copy {
            source: format!("/slide[{}]", n("index")?),
            target_parent: "/presentation".into(),
            index: None,
        },
        "slide_delete" => DeckMutation::Remove {
            path: format!("/slide[{}]", n("index")?),
        },
        "slide_reorder" => DeckMutation::Move {
            source: format!("/slide[{}]", n("from")?),
            target_parent: Some("/presentation".into()),
            index: Some(n("to")?.saturating_sub(1)),
        },
        "element_update" => DeckMutation::Set {
            path: req_str(&a, "id")?.into(),
            properties: serde_json::from_value(
                a.get("properties").cloned().unwrap_or(Value::Null),
            )?,
        },
        "deck_advanced" => serde_json::from_value(
            a.get("mutation")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("missing mutation"))?,
        )?,
        "text_add" | "image_add" | "shape_add" => {
            let slide = n("slide")?;
            let (x, y, w, h) = (f("x")?, f("y")?, f("width")?, f("height")?);
            validate_geometry(x, y, w, h)?;
            let mut p = HashMap::from([
                ("x".into(), x.to_string()),
                ("y".into(), y.to_string()),
                ("width".into(), w.to_string()),
                ("height".into(), h.to_string()),
            ]);
            let typ = if name == "text_add" {
                p.insert("text".into(), req_str(&a, "text")?.into());
                if let Some(v) = a.get("font_size") {
                    p.insert("font_size".into(), v.to_string());
                }
                "textbox"
            } else if name == "image_add" {
                let path = req_str(&a, "path")?;
                let meta = std::fs::metadata(path)?;
                if meta.len() > MAX_MEDIA_BYTES {
                    anyhow::bail!("image exceeds {MAX_MEDIA_BYTES} byte limit")
                };
                p.insert("path".into(), path.into());
                "image"
            } else {
                let k = req_str(&a, "kind")?;
                if let Some(v) = a.get("fill").and_then(Value::as_str) {
                    p.insert("fill".into(), v.into());
                }
                k
            };
            DeckMutation::Add {
                parent: format!("/slide[{slide}]"),
                element_type: typ.into(),
                properties: p,
            }
        }
        _ => anyhow::bail!("unknown deck tool"),
    };
    Ok(serde_json::to_value(e.mutate(mutation).await?)?)
}
fn req_str<'a>(v: &'a Value, k: &str) -> anyhow::Result<&'a str> {
    v.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("invalid field `{k}`: expected string"))
}
fn validate_geometry(x: f64, y: f64, w: f64, h: f64) -> anyhow::Result<()> {
    if ![x, y, w, h].iter().all(|v| v.is_finite()) {
        anyhow::bail!("geometry must be finite")
    };
    if x < 0. || y < 0. || w <= 0. || h <= 0. || x + w > 13.334 || y + h > 7.5 {
        anyhow::bail!("geometry exceeds 13.333 x 7.5 inch slide bounds")
    };
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn geometry() {
        assert!(validate_geometry(0., 0., 1., 1.).is_ok());
        assert!(validate_geometry(13., 0., 1., 1.).is_err());
    }
}
