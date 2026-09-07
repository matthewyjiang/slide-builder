use super::Action;
use rho_sdk::model::ToolSpec;
use serde_json::json;

pub(super) fn spec(action: Action) -> ToolSpec {
    let (description, input_schema) = match action {
        Action::Create => (
            "Create an immutable, deck-local SVG illustration candidate from authored SVG and a brief. Validates a strict static subset and attaches a PNG preview; returns an asset ID. Declare xmlns=\"http://www.w3.org/2000/svg\". Use shapes/groups/paths with an explicit positive viewBox, solid fills/strokes, no text, CSS, external resources, embedded images, or scripts. Either provide both root width/height or omit both. Keep labels/charts/evidence native or sourced. Mechanical validation is not visual approval. Set revises to preserve revision lineage without overwriting a candidate.",
            json!({"type":"object","required":["name","brief","svg"],"properties":{
                "name":{"type":"string","minLength":1},
                "brief":{"type":"object","required":["purpose","style","alt_text"],"properties":{
                    "purpose":{"type":"string","minLength":1,"description":"What this conceptual illustration should communicate."},
                    "style":{"type":"string","minLength":1,"description":"Deck palette, stroke weight, composition, background treatment, and approved references."},
                    "alt_text":{"type":"string","minLength":1,"description":"Meaningful accessible description; identify conceptual illustrations where appropriate."}
                },"additionalProperties":false},
                "svg":{"type":"string","minLength":1},
                "revises":{"type":["string","null"],"format":"uuid"}
            },"additionalProperties":false}),
        ),
        Action::List => (
            "List the active deck's saved asset candidates and briefs so existing assets can be reused. Reports skipped invalid records as warnings without hiding usable assets. Does not claim that candidates have passed visual review.",
            json!({"type":"object","properties":{},"additionalProperties":false}),
        ),
        Action::Inspect => (
            "Load a saved asset by ID, revalidate its SVG, and attach a PNG preview. Returns source and brief for review or revision with asset_create_svg. This reviews the asset in isolation, not its slide placement.",
            json!({"type":"object","required":["id"],"properties":{"id":{"type":"string","format":"uuid"}},"additionalProperties":false}),
        ),
        Action::Place => (
            "Place a saved asset by ID in the active deck. Centers the full artwork inside an inch-based box, preserving aspect ratio without cropping or stretching. Embeds SVG with a PNG fallback and alt text. Revalidates before a transactional insertion. Always call render_deck afterward and inspect the composed slide for quality; approval is not inherited from another use.",
            json!({"type":"object","required":["id","slide","x","y","width","height"],"properties":{
                "id":{"type":"string","format":"uuid"},"slide":{"type":"integer","minimum":1},
                "x":{"type":"number","minimum":0},"y":{"type":"number","minimum":0},
                "width":{"type":"number","exclusiveMinimum":0},"height":{"type":"number","exclusiveMinimum":0}
            },"additionalProperties":false}),
        ),
    };
    ToolSpec {
        name: action.name().into(),
        description: description.into(),
        input_schema,
    }
}
