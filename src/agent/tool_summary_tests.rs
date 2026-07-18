use super::target;
use serde_json::json;

#[test]
fn deck_mutations_keep_only_actionable_context() {
    assert_eq!(
        target(
            "shape_add",
            &json!({
                "slide": 3,
                "kind": "rectangle",
                "x": 0.5,
                "y": 1.0,
                "width": 4.0,
                "height": 2.0,
                "fill": "#ffffff"
            })
        ),
        "rectangle to slide 3"
    );
    assert_eq!(
        target("slide_reorder", &json!({"from": 4, "to": 2})),
        "slide 4 to position 2"
    );
    assert_eq!(
        target(
            "shape_add",
            &json!({"edits": [
                {"slide": 1, "kind": "rectangle"},
                {"slide": 2, "kind": "ellipse"}
            ]})
        ),
        "2 deck edits"
    );
}

#[test]
fn file_tools_show_only_the_file_name() {
    assert_eq!(
        target(
            "write_file",
            &json!({"path": "/tmp/generated/assets/chart.svg", "content": "lots of data"})
        ),
        "chart.svg"
    );
}

#[test]
fn shell_commands_do_not_echo_command_content() {
    assert_eq!(
        target("bash", &json!({"command": "a very long command"})),
        "command"
    );
}
