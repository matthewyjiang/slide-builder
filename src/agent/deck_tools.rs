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
            json!({"type":"object","required":["slide","kind","x","y","width","height"],"properties":{"slide":{"type":"integer","minimum":1},"kind":{"enum":["rectangle","ellipse","hexagon","line","connector"]},"x":{"type":"number"},"y":{"type":"number"},"width":{"type":"number"},"height":{"type":"number"},"fill":{"type":"string"}}})
        }
        "element_update" => {
            json!({"type":"object","required":["id","properties"],"properties":{"id":{"type":"string"},"properties":{"type":"object","additionalProperties":{"type":"string"}}}})
        }
        "deck_inspect" => json!({"type":"object","properties":{"path":{"type":"string"}}}),
        "deck_validate" => json!({"type":"object","properties":{}}),
        "deck_advanced" => json!({
            "type":"object",
            "required":["mutation"],
            "properties":{"mutation":{"oneOf":[
                {
                    "type":"object",
                    "required":["operation","parent","element_type"],
                    "properties":{
                        "operation":{"const":"add"},
                        "parent":{"type":"string"},
                        "element_type":{"type":"string"},
                        "properties":{"type":"object","additionalProperties":{"type":"string"}}
                    },
                    "additionalProperties":false
                },
                {
                    "type":"object",
                    "required":["operation","path","properties"],
                    "properties":{
                        "operation":{"const":"set"},
                        "path":{"type":"string"},
                        "properties":{"type":"object","additionalProperties":{"type":"string"}}
                    },
                    "additionalProperties":false
                },
                {
                    "type":"object",
                    "required":["operation","path"],
                    "properties":{"operation":{"const":"remove"},"path":{"type":"string"}},
                    "additionalProperties":false
                },
                {
                    "type":"object",
                    "required":["operation","source"],
                    "properties":{
                        "operation":{"const":"move"},
                        "source":{"type":"string"},
                        "target_parent":{"type":["string","null"]},
                        "index":{"type":["integer","null"],"minimum":1}
                    },
                    "additionalProperties":false
                },
                {
                    "type":"object",
                    "required":["operation","source","target_parent"],
                    "properties":{
                        "operation":{"const":"copy"},
                        "source":{"type":"string"},
                        "target_parent":{"type":"string"},
                        "index":{"type":["integer","null"],"minimum":1}
                    },
                    "additionalProperties":false
                },
                {
                    "type":"object",
                    "required":["operation","left","right"],
                    "properties":{
                        "operation":{"const":"swap"},
                        "left":{"type":"string"},
                        "right":{"type":"string"}
                    },
                    "additionalProperties":false
                },
                {
                    "type":"object",
                    "required":["operation","part","xpath","action"],
                    "properties":{
                        "operation":{"const":"raw_set"},
                        "part":{"type":"string"},
                        "xpath":{"type":"string"},
                        "action":{"type":"string"},
                        "xml":{"type":["string","null"]}
                    },
                    "additionalProperties":false
                }
            ]}},
            "additionalProperties":false
        }),
        _ => json!({"type":"object"}),
    }
}
fn description(name: &str) -> &'static str {
    match name {
        "slide_create" => "Add one blank slide to the end of the active deck.",
        "slide_duplicate" => "Duplicate a one-based slide index.",
        "slide_delete" => "Delete a one-based slide index.",
        "slide_reorder" => "Move a slide from one one-based position to another.",
        "text_add" => "Add a text box to a slide using inch-based geometry.",
        "image_add" => "Add a local image to a slide using inch-based geometry.",
        "shape_add" => "Add a rectangle, ellipse, hexagon, line, or connector using inch-based geometry.",
        "element_update" => "Update an existing element by the stable ID returned by deck_inspect or an add operation. Put all changed values inside properties.",
        "deck_inspect" => "Inspect the active deck or one optional handler path before editing. Returns slide geometry, elements, and stable IDs.",
        "deck_validate" => "Validate the active deck after meaningful edits.",
        "deck_advanced" => "Apply one advanced add, set, remove, move, copy, swap, or raw_set mutation. Use semantic tools for normal edits and follow the exact nested mutation schema.",
        _ => "Operate on the active PowerPoint deck.",
    }
}

impl Tool for DeckTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name.into(),
            description: description(self.name).into(),
            input_schema: schema(self.name),
        }
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
        let value = a
            .get(k)
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow::anyhow!("invalid field `{k}`: expected positive integer"))?;
        if value == 0 || value > usize::MAX as u64 {
            anyhow::bail!("invalid field `{k}`: expected positive integer");
        }
        Ok(value as usize)
    };
    let f = |k: &str| {
        a.get(k)
            .and_then(Value::as_f64)
            .ok_or_else(|| anyhow::anyhow!("invalid field `{k}`: expected number"))
    };
    let mut mutation = match name {
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
            index: Some(n("to")?),
        },
        "element_update" => {
            let path = e.resolve_element(req_str(&a, "id")?.into()).await?;
            let mut properties: HashMap<String, String> =
                serde_json::from_value(a.get("properties").cloned().unwrap_or(Value::Null))?;
            normalize_update_properties(&mut properties)?;
            DeckMutation::Set { path, properties }
        }
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
                ("x".into(), format!("{x}in")),
                ("y".into(), format!("{y}in")),
                ("width".into(), format!("{w}in")),
                ("height".into(), format!("{h}in")),
            ]);
            let typ = if name == "text_add" {
                p.insert("text".into(), req_str(&a, "text")?.into());
                if let Some(v) = a.get("font_size") {
                    p.insert(
                        "fontSize".into(),
                        finite_number(v, "font_size")?.to_string(),
                    );
                }
                "rectangle"
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
                if k == "hexagon" {
                    p.insert("preset".into(), k.into());
                    "rectangle"
                } else {
                    k
                }
            };
            DeckMutation::Add {
                parent: format!("/slide[{slide}]"),
                element_type: typ.into(),
                properties: p,
            }
        }
        _ => anyhow::bail!("unknown deck tool"),
    };
    normalize_mutation_properties(&mut mutation)?;
    Ok(serde_json::to_value(e.mutate(mutation).await?)?)
}
fn normalize_mutation_properties(mutation: &mut DeckMutation) -> anyhow::Result<()> {
    let properties = match mutation {
        DeckMutation::Add { properties, .. } | DeckMutation::Set { properties, .. } => properties,
        _ => return Ok(()),
    };
    if let Some(text) = properties.get_mut("text") {
        *text = text.replace("\\r\\n", "\n").replace("\\n", "\n");
    }
    for key in [
        "fill",
        "fillColor",
        "bg",
        "bgColor",
        "background",
        "color",
        "fontColor",
        "font.color",
        "lineColor",
        "borderColor",
    ] {
        let Some(color) = properties.get_mut(key) else {
            continue;
        };
        let normalized = color.strip_prefix('#').unwrap_or(color);
        if normalized.len() != 6 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            anyhow::bail!("invalid `{key}` color `{color}`: expected six hexadecimal digits");
        }
        *color = normalized.to_ascii_uppercase();
    }
    Ok(())
}

fn finite_number(value: &Value, field: &str) -> anyhow::Result<f64> {
    let number = value
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("invalid field `{field}`: expected number"))?;
    if !number.is_finite() {
        anyhow::bail!("invalid field `{field}`: expected finite number");
    }
    Ok(number)
}

fn normalize_update_properties(properties: &mut HashMap<String, String>) -> anyhow::Result<()> {
    for key in ["x", "left", "y", "top", "width", "w", "height", "h"] {
        if let Some(value) = properties.get_mut(key) {
            // Semantic-tool geometry is always inches. Preserve explicit units for
            // advanced callers, while preventing numeric strings from becoming EMU.
            if let Ok(number) = value.parse::<f64>() {
                if !number.is_finite() {
                    anyhow::bail!("geometry must be finite");
                }
                *value = format!("{number}in");
            }
        }
    }
    if let Some(value) = properties.remove("font_size") {
        properties.insert("fontSize".into(), value);
    }
    Ok(())
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
    #[tokio::test]
    async fn text_add_roundtrips_inches_font_geometry_and_stable_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("text.pptx");
        let engine = DeckEngine::create(&path, None).await.unwrap();

        let result = execute(
            "text_add",
            &engine,
            json!({
                "slide": 1,
                "text": "roundtrip text",
                "x": 1.0,
                "y": 0.5,
                "width": 4.0,
                "height": 1.25,
                "font_size": 24.0
            }),
        )
        .await
        .unwrap();
        let stable_id = result["affected"][0]
            .as_str()
            .unwrap_or_else(|| panic!("missing stable affected ID: {result}"))
            .to_owned();
        assert!(stable_id.starts_with("slide:"));
        assert!(stable_id.contains("/shape:"));

        // Read from a newly opened handler through DeckEngine::raw, proving the
        // transaction's saved package retained semantic inch and point values.
        let xml = engine.raw("ppt/slides/slide1.xml".into()).await.unwrap();
        assert!(xml.contains(r#"<a:off x="914400" y="457200"/>"#), "{xml}");
        assert!(
            xml.contains(r#"<a:ext cx="3657600" cy="1143000"/>"#),
            "{xml}"
        );
        assert!(xml.contains(r#"sz="2400""#), "{xml}");

        execute(
            "element_update",
            &engine,
            json!({
                "id": stable_id,
                "properties": {
                    "text": "updated",
                    "x": "2",
                    "color": "#003057",
                    "fill": "#F3F7FA"
                }
            }),
        )
        .await
        .unwrap();
        let xml = engine.raw("ppt/slides/slide1.xml".into()).await.unwrap();
        assert!(xml.contains("updated"), "{xml}");
        assert!(xml.contains(r#"<a:off x="1828800" y="457200"/>"#), "{xml}");
    }

    #[tokio::test]
    async fn slide_operations_keep_ids_stable_and_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("slides.pptx");
        let engine = DeckEngine::create(&path, None).await.unwrap();
        for _ in 0..2 {
            execute("slide_create", &engine, json!({})).await.unwrap();
        }
        let shape = execute(
            "text_add",
            &engine,
            json!({"slide":1,"text":"before move","x":1,"y":1,"width":3,"height":1}),
        )
        .await
        .unwrap();
        let shape_id = shape["affected"][0].as_str().unwrap().to_owned();
        let before = engine.inspect(None).await.unwrap();
        let ids = before["slides"]
            .as_array()
            .unwrap()
            .iter()
            .map(|slide| slide["slide_id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();

        execute("slide_reorder", &engine, json!({"from": 1, "to": 3}))
            .await
            .unwrap();
        let reordered = engine.inspect(None).await.unwrap();
        let reordered_ids = reordered["slides"]
            .as_array()
            .unwrap()
            .iter()
            .map(|slide| slide["slide_id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            reordered_ids,
            vec![ids[1].clone(), ids[2].clone(), ids[0].clone()]
        );
        execute(
            "text_add",
            &engine,
            json!({"slide":1,"text":"added after reorder","x":1,"y":2,"width":3,"height":1}),
        )
        .await
        .unwrap();
        assert_eq!(
            engine
                .inspect(Some("/slide[1]/shape[1]".into()))
                .await
                .unwrap()["text"],
            "added after reorder"
        );
        execute(
            "element_update",
            &engine,
            json!({"id": shape_id, "properties":{"text":"after move"}}),
        )
        .await
        .unwrap();
        assert_eq!(
            engine
                .inspect(Some("/slide[3]/shape[1]".into()))
                .await
                .unwrap()["text"],
            "after move"
        );

        let duplicate = execute("slide_duplicate", &engine, json!({"index": 2}))
            .await
            .unwrap();
        assert!(duplicate["affected"][0]
            .as_str()
            .unwrap()
            .starts_with("slide:"));
        execute("slide_delete", &engine, json!({"index": 2}))
            .await
            .unwrap();

        // Reopen-derived inspection must see three valid slides, with the deleted
        // stable ID absent and the moved slide IDs otherwise unchanged.
        let after = DeckEngine::new(&path).unwrap().inspect(None).await.unwrap();
        let after_ids = after["slides"]
            .as_array()
            .unwrap()
            .iter()
            .map(|slide| slide["slide_id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(after_ids.len(), 3);
        assert!(!after_ids.contains(&ids[2].as_str()));
        assert!(after_ids.contains(&ids[0].as_str()));
        assert!(after_ids.contains(&ids[1].as_str()));
    }

    #[tokio::test]
    async fn image_add_embeds_visible_picture_and_returns_stable_id() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("pixel.png");
        image::RgbImage::from_pixel(2, 2, image::Rgb([0, 48, 87]))
            .save(&image_path)
            .unwrap();
        let deck_path = dir.path().join("image.pptx");
        let engine = DeckEngine::create(&deck_path, None).await.unwrap();

        let result = execute(
            "image_add",
            &engine,
            json!({
                "slide": 1,
                "path": image_path,
                "x": 0,
                "y": 0,
                "width": 13.333,
                "height": 7.5
            }),
        )
        .await
        .unwrap();

        let stable_id = result["affected"][0].as_str().unwrap();
        assert!(stable_id.contains("/picture:"), "{result}");
        assert!(engine
            .resolve_element(stable_id.to_owned())
            .await
            .unwrap()
            .starts_with("/slide[1]/picture["));
        let xml = engine.raw("ppt/slides/slide1.xml".into()).await.unwrap();
        assert!(xml.contains("xmlns:r="), "{xml}");
        assert!(xml.contains("<p:pic>"), "{xml}");
        let snapshot = engine.snapshot().await.unwrap();
        assert!(
            snapshot.html.contains("data-path=\"/slide[1]/picture["),
            "{}",
            snapshot.html
        );
        let inspected = engine.inspect(None).await.unwrap();
        let picture = inspected["slides"][0]["shapes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|shape| shape["type"] == "image")
            .unwrap_or_else(|| panic!("missing image from inspection: {inspected}"));
        assert!((picture["geometry"]["width"].as_f64().unwrap() - 13.333).abs() < 0.0002);
        assert_eq!(picture["geometry"]["height"], 7.5);
    }

    #[tokio::test]
    async fn semantic_tools_normalize_colors_newlines_and_hexagons() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("normalized.pptx");
        let engine = DeckEngine::create(&path, None).await.unwrap();
        execute(
            "text_add",
            &engine,
            json!({
                "slide": 1,
                "text": "first\\nsecond",
                "x": 1,
                "y": 1,
                "width": 3,
                "height": 1
            }),
        )
        .await
        .unwrap();
        execute(
            "shape_add",
            &engine,
            json!({
                "slide": 1,
                "kind": "hexagon",
                "x": 5,
                "y": 1,
                "width": 2,
                "height": 2,
                "fill": "#003057"
            }),
        )
        .await
        .unwrap();
        let xml = engine.raw("ppt/slides/slide1.xml".into()).await.unwrap();
        assert!(xml.contains("first\nsecond"), "{xml}");
        assert!(!xml.contains(r"first\nsecond"), "{xml}");
        assert!(xml.contains(r#"prst="hexagon""#), "{xml}");
        assert!(xml.contains(r#"val="003057""#), "{xml}");
    }

    #[test]
    fn normalizes_hash_prefixed_colors() {
        let mut mutation = DeckMutation::Set {
            path: "/slide[1]/shape[1]".into(),
            properties: HashMap::from([
                ("fill".into(), "#f3f7fa".into()),
                ("fontColor".into(), "003057".into()),
            ]),
        };
        normalize_mutation_properties(&mut mutation).unwrap();
        let DeckMutation::Set { properties, .. } = mutation else {
            unreachable!();
        };
        assert_eq!(properties["fill"], "F3F7FA");
        assert_eq!(properties["fontColor"], "003057");
    }

    #[test]
    fn rejects_invalid_colors_before_mutation() {
        let mut mutation = DeckMutation::Set {
            path: "/slide[1]/shape[1]".into(),
            properties: HashMap::from([("fill".into(), "#xyz".into())]),
        };
        assert!(normalize_mutation_properties(&mut mutation).is_err());
    }

    #[test]
    fn geometry() {
        assert!(validate_geometry(0., 0., 1., 1.).is_ok());
        assert!(validate_geometry(13., 0., 1., 1.).is_err());
    }
}

#[cfg(test)]
#[path = "deck_tool_schema_tests.rs"]
mod schema_tests;
