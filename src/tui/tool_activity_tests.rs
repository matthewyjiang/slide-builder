use super::*;
use crate::tui::{chat, AgentEvent, App, AppEvent};
use crossterm::event::{KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, layout::Rect, Terminal};

fn event(app: &mut App, event: AgentEvent) {
    app.apply(AppEvent::Run(event));
}

fn propose(app: &mut App, id: &str) {
    event(
        app,
        AgentEvent::ToolProposed {
            id: id.into(),
            name: "shape_add".into(),
            summary: "rectangle to slide 3".into(),
            arguments: "{\n  \"slide\": 3\n}".into(),
        },
    );
    event(app, AgentEvent::ToolStarted { id: id.into() });
}

fn key(app: &mut App, code: KeyCode) {
    assert!(app
        .handle_key(KeyEvent::new(code, KeyModifiers::NONE))
        .is_empty());
}

fn inspect(app: &mut App) {
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL));
    key(app, KeyCode::Char('t'));
}

fn screen(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| chat::render(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn live_group_stays_open_between_calls_then_collapses_after_run() {
    let mut app = App {
        run_active: true,
        ..App::default()
    };
    propose(&mut app, "1");
    event(
        &mut app,
        AgentEvent::ToolFinished {
            id: "1".into(),
            result: Ok("shape:42".into()),
        },
    );
    assert!(screen(&app, 70, 12).contains("Added rectangle"));
    propose(&mut app, "2");
    assert_eq!(app.tool_activity.groups(&app.transcript), vec![0..2]);
    event(
        &mut app,
        AgentEvent::ToolFinished {
            id: "2".into(),
            result: Ok("shape:43".into()),
        },
    );
    event(&mut app, AgentEvent::RunFinished);
    let rendered = screen(&app, 70, 12);
    assert!(rendered.contains("▸ ✓ Completed tool activity · 2 operations"));
    assert!(!rendered.contains("Added rectangle"));
}

#[test]
fn keyboard_inspection_preserves_prompt_and_manual_expansion() {
    let mut app = App {
        run_active: true,
        ..App::default()
    };
    app.input.text = "keep this draft".into();
    propose(&mut app, "1");
    inspect(&mut app);
    key(&mut app, KeyCode::Right);
    key(&mut app, KeyCode::Enter);
    event(
        &mut app,
        AgentEvent::ToolFinished {
            id: "1".into(),
            result: Ok("shape:42".into()),
        },
    );
    event(&mut app, AgentEvent::RunFinished);
    let rendered = screen(&app, 70, 16);
    assert!(rendered.contains("Tool: shape_add"));
    assert!(rendered.contains("\"slide\": 3"));
    assert!(rendered.contains("shape:42"));
    key(&mut app, KeyCode::Esc);
    assert_eq!(
        app.tool_activity.focus,
        Some(ActivityFocus::Call { group: 0, index: 0 })
    );
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.tool_activity.focus, Some(ActivityFocus::Group(0)));
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.tool_activity.focus, None);
    assert_eq!(app.input.text, "keep this draft");
    assert!(app.tool_activity.is_expanded(&(0..1), &app.transcript));
}

#[test]
fn failures_override_collapse_and_escape_does_not_cancel_from_inspection() {
    let mut app = App {
        run_active: true,
        ..App::default()
    };
    propose(&mut app, "1");
    inspect(&mut app);
    key(&mut app, KeyCode::Enter);
    event(
        &mut app,
        AgentEvent::ToolFinished {
            id: "1".into(),
            result: Err("Image file not found".into()),
        },
    );
    let rendered = screen(&app, 70, 12);
    assert!(rendered.contains("▾ ! Tool activity has errors"));
    assert!(rendered.contains("Image file not found"));
    key(&mut app, KeyCode::Esc);
    assert!(app.run_active);
}

#[test]
fn messages_and_run_boundaries_separate_groups() {
    let mut app = App::default();
    propose(&mut app, "1");
    event(
        &mut app,
        AgentEvent::TextDelta("Now updating the deck.".into()),
    );
    propose(&mut app, "2");
    event(&mut app, AgentEvent::RunCancelled);
    propose(&mut app, "3");
    assert_eq!(
        app.tool_activity.groups(&app.transcript),
        vec![0..1, 2..3, 3..4]
    );
    let TranscriptItem::Tool(card) = &app.transcript[2] else {
        panic!()
    };
    assert_eq!(card.status, ToolStatus::Failed);
    assert!(card.detail.contains("Run cancelled"));
}

#[test]
fn narrow_inspection_wraps_and_pages_through_full_output() {
    let mut app = App::default();
    propose(&mut app, "1");
    let detail = (0..30)
        .map(|n| format!("output line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    event(
        &mut app,
        AgentEvent::ToolFinished {
            id: "1".into(),
            result: Ok(detail),
        },
    );
    inspect(&mut app);
    key(&mut app, KeyCode::Right);
    key(&mut app, KeyCode::Enter);
    for width in [1, 12, 32, 70] {
        let rendered = super::super::tool_activity_render::render(&app, 0..1, width);
        assert!(rendered.lines.iter().all(|line| line.width() <= width));
    }
    let before = chat::visible_text_rows(Rect::new(0, 0, 32, 10), &app);
    for _ in 0..5 {
        key(&mut app, KeyCode::PageDown);
    }
    let after = chat::visible_text_rows(Rect::new(0, 0, 32, 10), &app);
    assert_ne!(before, after);
    assert!(after.iter().any(|row| row.contains("output line 29")));

    // A real workspace viewport bounds paging, so repeated PageDown presses do
    // not create an invisible overscroll that must be unwound with PageUp.
    app.mouse.viewport = Rect::new(0, 0, 80, 24);
    for _ in 0..30 {
        key(&mut app, KeyCode::PageDown);
    }
    let last_page = app.tool_activity.detail_scroll;
    key(&mut app, KeyCode::PageDown);
    assert_eq!(app.tool_activity.detail_scroll, last_page);
    key(&mut app, KeyCode::PageUp);
    assert_eq!(app.tool_activity.detail_scroll, last_page.saturating_sub(1));
}

#[test]
fn raw_output_is_rendered_as_text_not_terminal_commands() {
    let mut app = App::default();
    propose(&mut app, "1");
    event(
        &mut app,
        AgentEvent::ToolFinished {
            id: "1".into(),
            result: Err("bad\u{1b}[2J\routput".into()),
        },
    );
    let rendered = screen(&app, 70, 12);
    assert!(rendered.contains("bad\\u{1b}[2J\\routput"));
}
