use std::time::{Duration, Instant};

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::*;
use crate::tui::{AppEvent, Message, Role, SlideItem, TranscriptItem};

fn mouse(kind: MouseEventKind, x: u16, y: u16) -> AppEvent {
    AppEvent::Input(crossterm::event::Event::Mouse(MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }))
}

fn app_at(width: u16, height: u16) -> App {
    App {
        mouse: MouseState {
            viewport: Rect::new(0, 0, width, height),
            ..MouseState::default()
        },
        ..App::default()
    }
}

fn draw(app: &App) {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(
        app.mouse.viewport.width,
        app.mouse.viewport.height,
    ))
    .unwrap();
    terminal
        .draw(|frame| crate::tui::render(frame, app))
        .unwrap();
}

#[test]
fn clicking_code_copy_uses_unwrapped_source_after_scroll_and_resize() {
    for width in [140, 70] {
        let source =
            "let result = a_very_long_function_name_that_wraps_in_a_narrow_conversation();";
        let mut app = App {
            transcript: vec![TranscriptItem::Message(Message {
                role: Role::Assistant,
                text: format!("{}\n```rust\n{source}\n```", "earlier\n".repeat(30)),
                complete: true,
            })],
            ..app_at(width, 40)
        };
        let chat = layout::regions(app.mouse.viewport, &app).chat;
        draw(&app);
        let rows = chat::painted_text_rows(&app);
        let (row, column) = rows
            .iter()
            .enumerate()
            .find_map(|(row, text)| {
                text.to_lowercase()
                    .find("copy")
                    .map(|column| (row, text[..column].width()))
            })
            .expect("copy header visible above final code block");
        let x = chat.x + column as u16;
        let y = chat.y + 1 + row as u16;
        app.apply(mouse(MouseEventKind::Down(MouseButton::Left), x, y));
        let actions = app.apply(mouse(MouseEventKind::Up(MouseButton::Left), x, y));
        assert_eq!(actions, vec![AppAction::CopyText(source.into())]);
        assert!(app.mouse.selection.is_none());
    }
}

#[test]
fn copy_uses_painted_source_until_streaming_and_resize_are_drawn() {
    let mut app = App {
        transcript: vec![TranscriptItem::Message(Message {
            role: Role::Assistant,
            text: "```rust\nlet first = 1;\n".into(),
            complete: false,
        })],
        ..app_at(140, 40)
    };
    draw(&app);
    let body = chat::painted_area(&app).unwrap();
    let (row, column) = chat::painted_text_rows(&app)
        .iter()
        .enumerate()
        .find_map(|(row, text)| {
            text.find("COPY")
                .map(|column| (row, text[..column].width()))
        })
        .unwrap();
    let x = body.x + column as u16;
    let y = body.y + row as u16;
    app.apply(mouse(MouseEventKind::Down(MouseButton::Left), x, y));
    let TranscriptItem::Message(message) = &mut app.transcript[0] else {
        unreachable!()
    };
    message.text.push_str("let second = 2;\n```");
    message.complete = true;
    app.apply(AppEvent::Input(crossterm::event::Event::Resize(70, 30)));
    assert_eq!(
        app.apply(mouse(MouseEventKind::Up(MouseButton::Left), x, y)),
        vec![AppAction::CopyText("let first = 1;".into())]
    );
    app.mouse.toast = None;
    draw(&app);
    let body = chat::painted_area(&app).unwrap();
    let (row, column) = chat::painted_text_rows(&app)
        .iter()
        .enumerate()
        .find_map(|(row, text)| {
            text.find("COPY")
                .map(|column| (row, text[..column].width()))
        })
        .unwrap();
    let x = body.x + column as u16;
    let y = body.y + row as u16;
    app.apply(mouse(MouseEventKind::Down(MouseButton::Left), x, y));
    assert_eq!(
        app.apply(mouse(MouseEventKind::Up(MouseButton::Left), x, y)),
        vec![AppAction::CopyText(
            "let first = 1;\nlet second = 2;".into()
        )]
    );
}

#[test]
fn clicking_a_visible_slide_selects_it() {
    let mut app = app_at(140, 40);
    app.preview.slides = (0..4)
        .map(|index| SlideItem {
            title: format!("Slide {}", index + 1),
            image_path: None,
        })
        .collect();
    let outline = layout::regions(app.mouse.viewport, &app).outline;

    let actions = app.apply(mouse(
        MouseEventKind::Down(MouseButton::Left),
        outline.x + 2,
        outline.y + 1 + 2,
    ));

    assert_eq!(app.preview.active, 2);
    assert_eq!(actions, vec![AppAction::SetActiveSlide(2)]);
}

#[test]
fn dragging_visible_conversation_text_copies_it_and_shows_feedback() {
    let mut app = App {
        transcript: vec![TranscriptItem::Message(Message {
            role: Role::System,
            text: "hello world".into(),
            complete: true,
        })],
        ..app_at(140, 40)
    };
    let chat = layout::regions(app.mouse.viewport, &app).chat;
    let text_y = chat.y + 2;
    draw(&app);

    assert!(app
        .apply(mouse(
            MouseEventKind::Down(MouseButton::Left),
            chat.x + 1,
            text_y,
        ))
        .is_empty());
    app.apply(mouse(
        MouseEventKind::Drag(MouseButton::Left),
        chat.x + 5,
        text_y,
    ));
    let actions = app.apply(mouse(
        MouseEventKind::Up(MouseButton::Left),
        chat.x + 5,
        text_y,
    ));

    assert_eq!(actions, vec![AppAction::CopyText("hello".into())]);
    assert_eq!(
        app.mouse.toast.as_ref().map(|toast| toast.message.as_str()),
        Some("Copied 5 chars")
    );
}

#[test]
fn dragging_from_a_wide_grapheme_uses_terminal_columns() {
    let mut app = App {
        transcript: vec![TranscriptItem::Message(Message {
            role: Role::System,
            text: "界x".into(),
            complete: true,
        })],
        ..app_at(140, 40)
    };
    let chat = layout::regions(app.mouse.viewport, &app).chat;
    let text_y = chat.y + 2;
    draw(&app);

    app.apply(mouse(
        MouseEventKind::Down(MouseButton::Left),
        chat.x + 2,
        text_y,
    ));
    app.apply(mouse(
        MouseEventKind::Drag(MouseButton::Left),
        chat.x + 3,
        text_y,
    ));
    let actions = app.apply(mouse(
        MouseEventKind::Up(MouseButton::Left),
        chat.x + 3,
        text_y,
    ));

    assert_eq!(actions, vec![AppAction::CopyText("界x".into())]);
}

#[test]
fn scrolling_over_the_conversation_moves_through_history() {
    let mut app = App {
        transcript: vec![TranscriptItem::Message(Message {
            role: Role::Assistant,
            text: (0..40)
                .map(|line| format!("conversation line {line}"))
                .collect::<Vec<_>>()
                .join("\n"),
            complete: true,
        })],
        ..app_at(80, 24)
    };
    let chat = layout::regions(app.mouse.viewport, &app).chat;
    let x = chat.x + chat.width / 2;
    let y = chat.y + chat.height / 2;

    draw(&app);
    let latest_rows = chat::painted_text_rows(&app);

    app.apply(mouse(MouseEventKind::ScrollUp, x, y));
    assert_eq!(app.conversation_scroll_offset, 3);
    draw(&app);
    assert_ne!(chat::painted_text_rows(&app), latest_rows);

    app.apply(mouse(MouseEventKind::ScrollDown, x, y));
    assert_eq!(app.conversation_scroll_offset, 0);
    draw(&app);
    assert_eq!(chat::painted_text_rows(&app), latest_rows);
}

#[test]
fn scrolling_outside_the_conversation_does_not_move_it() {
    let mut app = app_at(140, 40);
    let outline = layout::regions(app.mouse.viewport, &app).outline;

    app.apply(mouse(
        MouseEventKind::ScrollUp,
        outline.x + 1,
        outline.y + 1,
    ));

    assert_eq!(app.conversation_scroll_offset, 0);
}

#[test]
fn copy_feedback_expires_on_tick() {
    let mut app = app_at(80, 24);
    let now = Instant::now();
    app.mouse.toast = Some(CopyToast {
        message: "Copied 4 chars".into(),
        expires_at: now + Duration::from_secs(1),
        location: ScreenPoint::default(),
    });

    app.apply(AppEvent::Tick(now + Duration::from_secs(2)));

    assert!(app.mouse.toast.is_none());
}
