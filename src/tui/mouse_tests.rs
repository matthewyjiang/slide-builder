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

    assert!(app
        .apply(mouse(
            MouseEventKind::Down(MouseButton::Left),
            chat.x,
            text_y,
        ))
        .is_empty());
    app.apply(mouse(
        MouseEventKind::Drag(MouseButton::Left),
        chat.x + 4,
        text_y,
    ));
    let actions = app.apply(mouse(
        MouseEventKind::Up(MouseButton::Left),
        chat.x + 4,
        text_y,
    ));

    assert_eq!(actions, vec![AppAction::CopyText("hello".into())]);
    assert_eq!(
        app.mouse.toast.as_ref().map(|toast| toast.message.as_str()),
        Some("Copied 5 chars")
    );
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

    let latest_rows = chat::visible_text_rows(chat, &app);

    app.apply(mouse(MouseEventKind::ScrollUp, x, y));
    assert_eq!(app.conversation_scroll_offset, 3);
    assert_ne!(chat::visible_text_rows(chat, &app), latest_rows);

    app.apply(mouse(MouseEventKind::ScrollDown, x, y));
    assert_eq!(app.conversation_scroll_offset, 0);
    assert_eq!(chat::visible_text_rows(chat, &app), latest_rows);
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
