use serde_json::Value;
use std::path::Path;

/// Reduces tool arguments to the small amount of context useful in the transcript.
/// Tool inputs still flow to the SDK unchanged.
pub fn target(name: &str, arguments: &Value) -> String {
    match name {
        "slide_create" => "slide".into(),
        "slide_duplicate" | "slide_delete" => numbered("slide", arguments, "index"),
        "slide_reorder" => match (number(arguments, "from"), number(arguments, "to")) {
            (Some(from), Some(to)) => format!("slide {from} to position {to}"),
            _ => "slide".into(),
        },
        "text_add" => on_slide("text", arguments),
        "image_add" => on_slide("image", arguments),
        "shape_add" => {
            let kind = arguments
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("shape");
            on_slide(kind, arguments)
        }
        "element_update" => "element".into(),
        "deck_inspect" | "deck_validate" | "deck_advanced" => "deck".into(),
        "render_deck" => "preview render".into(),
        "set_active_slide" => numbered("slide", arguments, "index"),
        "list_dir" => path_target(arguments, "folder"),
        "read_file" | "write_file" | "edit_file" => path_target(arguments, "file"),
        "bash" | "powershell" => "command".into(),
        "web_search" => "web".into(),
        "get_search_content" => "search results".into(),
        "load_skill" => arguments
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("skill")
            .to_owned(),
        "discover_instructions" => "instructions".into(),
        _ => name.replace('_', " "),
    }
}

fn on_slide(item: &str, arguments: &Value) -> String {
    number(arguments, "slide")
        .map(|slide| format!("{item} to slide {slide}"))
        .unwrap_or_else(|| item.to_owned())
}

fn numbered(item: &str, arguments: &Value, field: &str) -> String {
    number(arguments, field)
        .map(|index| format!("{item} {index}"))
        .unwrap_or_else(|| item.to_owned())
}

fn number(arguments: &Value, field: &str) -> Option<u64> {
    arguments.get(field).and_then(Value::as_u64)
}

fn path_target(arguments: &Value, fallback: &str) -> String {
    let Some(path) = arguments.get("path").and_then(Value::as_str) else {
        return fallback.into();
    };
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_owned()
}

#[cfg(test)]
#[path = "tool_summary_tests.rs"]
mod tests;
