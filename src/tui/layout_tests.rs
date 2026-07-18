use super::*;
use crate::tui::App;
use ratatui::{backend::TestBackend, Terminal};

fn render_at(width: u16, height: u16, app: &App) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

#[test]
fn slash_command_suggestions_render_above_the_prompt() {
    let mut app = App::default();
    app.input.text = "/con".into();
    app.input.cursor = app.input.text.len();

    let content = render_at(100, 30, &app);

    assert!(content.contains("Slash commands"));
    assert!(content.contains("/config"));
    assert!(content.contains("Tab"));
    assert!(content.contains("complete"));
}

#[test]
fn wide_workspace_exposes_all_primary_surfaces_and_controls() {
    let content = render_at(140, 40, &App::default());
    for expected in [
        "SLIDE BUILDER",
        "Conversation",
        "Preview",
        "Slides",
        "Prompt",
        "Ctrl+K",
        "F1",
    ] {
        assert!(content.contains(expected), "missing {expected:?}");
    }
}

#[test]
fn composer_height_tracks_newlines_and_wrapped_lines() {
    let area = Rect::new(0, 0, 40, 30);
    let mut app = App::default();
    assert_eq!(input_height(&app, area), 3);

    app.input.text = "one\ntwo".into();
    assert_eq!(input_height(&app, area), 4);

    app.input.text = "one\n".into();
    assert_eq!(input_height(&app, area), 4);

    app.input.text = "x".repeat(81);
    assert_eq!(input_height(&app, area), 5);
}

#[test]
fn composer_has_separators_above_and_below() {
    let backend = TestBackend::new(40, 3);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_input(frame, frame.area(), &App::default()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let top = (0..40).map(|x| buffer[(x, 0)].symbol()).collect::<String>();
    let bottom = (0..40).map(|x| buffer[(x, 2)].symbol()).collect::<String>();

    assert!(top.contains("Prompt"));
    assert!(bottom.chars().all(|symbol| symbol == '─'));
}

#[test]
fn compact_workspace_keeps_all_status_surfaces_visible() {
    let content = render_at(72, 18, &App::default());
    assert!(content.contains("Conversation"));
    assert!(content.contains("Preview"));
    assert!(content.contains("Slides"));
    assert!(content.contains("Alt+S"));
}

#[test]
fn workspace_uses_separators_without_numbered_panel_labels() {
    let content = render_at(140, 40, &App::default());
    assert!(content.contains('│'));
    assert!(content.contains('─'));
    for removed in ["1  Conversation", "2  Preview", "3  Slides", "4  Prompt"] {
        assert!(
            !content.contains(removed),
            "found removed label {removed:?}"
        );
    }
}
