use super::*;
use crate::tui::{App, Message, Role, ToolCard, ToolStatus, TranscriptItem};

#[test]
fn composer_growth_does_not_change_image_dimensions() {
    for viewport in [Rect::new(0, 0, 140, 40), Rect::new(0, 0, 70, 30)] {
        let mut app = App::default();
        app.mouse.viewport = viewport;
        let first_chat = super::super::layout::regions(viewport, &app).chat;
        let initial = image_size(&app, usize::from(first_chat.width));
        app.input.text = "one\ntwo\nthree\nfour".into();
        let next_chat = super::super::layout::regions(viewport, &app).chat;
        assert!(next_chat.height < first_chat.height);
        assert_eq!(image_size(&app, usize::from(next_chat.width)), initial);
    }
}

#[test]
fn unchanged_messages_share_paint_across_frames_and_scrolling() {
    super::super::syntax::warm_syntax_set();
    let app = App {
        transcript: vec![TranscriptItem::Message(Message {
            role: Role::Assistant,
            text: "unchanged\n".repeat(300),
            complete: true,
        })],
        ..App::default()
    };
    let first = conversation_content(&app, 40);
    let second = conversation_content(&app, 40);
    assert!(Rc::ptr_eq(&first.chunks[0], &second.chunks[0]));
    let area = Rect::new(0, 0, 40, 5);
    let mut buffer = Buffer::empty(area);
    render_text(&second.chunks, area, &mut buffer, 100);
    assert!(buffer_row_text(&buffer, 0, area.width).contains("unchanged"));
}

#[test]
fn image_markers_share_retained_paint_including_offscreen_messages() {
    super::super::syntax::warm_syntax_set();
    let app = App {
        transcript: (0..10)
            .map(|_| {
                TranscriptItem::Message(Message {
                    role: Role::Assistant,
                    text: format!("![plot](image.png)\n{}", "unchanged\n".repeat(1000)),
                    complete: true,
                })
            })
            .collect(),
        ..App::default()
    };
    let first = conversation_content(&app, 40);
    let second = conversation_content(&app, 40);
    for (first, second) in first.chunks.iter().zip(&second.chunks) {
        assert!(
            Rc::ptr_eq(first, second),
            "unchanged image layout must be retained"
        );
    }
}

#[test]
fn latest_rows_remain_visible_after_wrapped_tool_output() {
    let app = App {
        transcript: vec![
            TranscriptItem::Tool(ToolCard {
                id: "call-1".into(),
                name: "write_file".into(),
                summary: "a very long generated artifact name that wraps repeatedly".into(),
                arguments: String::new(),
                detail: "a detailed tool result that also wraps on a narrow panel".into(),
                status: ToolStatus::Succeeded,
            }),
            TranscriptItem::Message(Message {
                role: Role::Assistant,
                text: "latest answer".into(),
                complete: true,
            }),
        ],
        ..App::default()
    };

    let rows = visible_text_rows(Rect::new(0, 0, 12, 6), &app);

    assert!(rows.iter().any(|row| row.contains("latest")));
    assert!(rows.iter().all(|row| row.chars().count() <= 12));
}
