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
    assert!(content.contains("Tab complete"));
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
fn compact_workspace_prioritizes_the_active_task() {
    let app = App {
        focus: Focus::Preview,
        ..App::default()
    };
    let content = render_at(72, 18, &app);
    assert!(content.contains("Preview"));
    assert!(!content.contains("Conversation"));
    assert!(content.contains("Tab"));
}
