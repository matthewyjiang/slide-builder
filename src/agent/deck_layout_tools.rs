//! Tool schemas for shared deck layout settings and deterministic composition.
use serde_json::{json, Value};

pub(super) fn schema(name: &str) -> Option<Value> {
    Some(match name {
        "deck_layout_inspect" | "deck_layout_audit" => {
            json!({"type":"object","properties":{},"additionalProperties":false})
        }
        "deck_layout_set" => json!({
            "type":"object",
            "required":["margins","gutter","regions","text_styles"],
            "properties":{
                "margins":{
                    "type":"object","required":["left","right","top","bottom"],
                    "properties":{
                        "left":{"type":"number","minimum":0},
                        "right":{"type":"number","minimum":0},
                        "top":{"type":"number","minimum":0},
                        "bottom":{"type":"number","minimum":0}
                    },"additionalProperties":false
                },
                "gutter":{"type":"number","minimum":0},
                "regions":{"type":"object","additionalProperties":{
                    "type":"object","required":["x","y","width","height"],
                    "properties":{
                        "x":{"type":"number","minimum":0},
                        "y":{"type":"number","minimum":0},
                        "width":{"type":"number","exclusiveMinimum":0},
                        "height":{"type":"number","exclusiveMinimum":0}
                    },"additionalProperties":false
                }},
                "text_styles":{"type":"object","additionalProperties":{
                    "type":"object","required":["font_size","font_family","color"],
                    "properties":{
                        "font_size":{"type":"number","minimum":1,"maximum":4000},
                        "font_family":{"type":"string","minLength":1},
                        "color":{"type":"string","pattern":"^#?[0-9a-fA-F]{6}$"},
                        "bold":{"type":"boolean"},"italic":{"type":"boolean"},
                        "alignment":{"type":"string","enum":["left","center","right","justify"]}
                    },"additionalProperties":false
                }}
            },"additionalProperties":false
        }),
        "elements_layout" => layout_schema(),
        _ => return None,
    })
}

fn layout_schema() -> Value {
    let ids = json!({
        "type":"array","minItems":1,"uniqueItems":true,
        "items":{"type":"string","minLength":1},
        "description":"Stable element IDs from mutation results. Geometry operations require elements on one slide."
    });
    let axis = json!({"type":"string","enum":["horizontal","vertical"]});
    json!({"oneOf":[
        {
            "type":"object","required":["operation","ids","edge"],
            "properties":{
                "operation":{"const":"align"},"ids":ids,
                "edge":{"type":"string","enum":["left","right","top","bottom","center_x","center_y"]},
                "reference":{"type":"string","description":"Optional anchor ID on the same slide; otherwise use selection bounds."}
            },"additionalProperties":false
        },
        {
            "type":"object","required":["operation","ids","axis"],
            "properties":{
                "operation":{"const":"distribute"},"ids":ids,"axis":axis,
                "gap":{"type":"number","minimum":0,"description":"Inches. Omit to preserve outer endpoints and equalize gaps in spatial order."}
            },"additionalProperties":false
        },
        {
            "type":"object","required":["operation","ids","reference","dimension"],
            "properties":{
                "operation":{"const":"match_size"},"ids":ids,
                "reference":{"type":"string"},
                "dimension":{"type":"string","enum":["width","height","both"]}
            },"additionalProperties":false
        },
        {
            "type":"object","required":["operation","ids","region","axis"],
            "properties":{
                "operation":{"const":"place"},"ids":ids,"axis":axis,
                "region":{"type":"string","description":"Named region from deck_layout_inspect. IDs fill equal slots in supplied order; placement resizes elements."},
                "gap":{"type":"number","minimum":0,"description":"Inches. Defaults to the saved deck gutter."}
            },"additionalProperties":false
        },
        {
            "type":"object","required":["operation","ids","style"],
            "properties":{
                "operation":{"const":"text_style"},"ids":ids,
                "style":{"type":"string","description":"Named text style from deck_layout_inspect; applies to all text runs in each selected element."}
            },"additionalProperties":false
        },
        {
            "type":"object","required":["operation","ids"],
            "properties":{
                "operation":{"const":"release"},
                "ids":ids
            },"additionalProperties":false,
            "description":"Forget saved geometry relationships touching these IDs and their text-style assignments without moving or restyling content. IDs from deleted elements are accepted for cleanup."
        }
    ]})
}

pub(super) fn description(name: &str) -> Option<&'static str> {
    Some(match name {
        "deck_layout_inspect" => "Read the deck's saved margins, gutter, named regions, and text styles before composition. Missing settings are not inferred or silently invented.",
        "deck_layout_set" => "Save a complete shared layout contract inside the active PowerPoint. Geometry is inches; font sizes are points. Derive values from the design package or existing deck. Replaces settings, not slide content; use elements_layout to apply them.",
        "elements_layout" => "Align, distribute, match sizes, place elements into a named region, or apply a named text style using stable IDs. Relationships are saved for later audit. Use release to forget checks without changing content. Coordinates are calculated deterministically and changes commit atomically. Placement resizes elements; inspect rendered images afterward for text fit and image proportions.",
        "deck_layout_audit" => "Check slide geometry and saved layout/style assignments without modifying the deck. Findings are mechanical evidence, not a visual-quality score. Render afterward to review hierarchy, wrapping, and intentional exceptions.",
        _ => return None,
    })
}
