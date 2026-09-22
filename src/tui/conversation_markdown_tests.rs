use super::*;
use crate::tui::{App, Role, TranscriptItem};
use pretty_assertions::assert_eq;
use ratatui::{backend::TestBackend, style::Modifier, Terminal};

fn assistant(text: &str, complete: bool) -> Message {
    Message {
        role: Role::Assistant,
        text: text.into(),
        complete,
    }
}

fn text(rendered: &RenderedMarkdown) -> String {
    rendered
        .lines
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn assistant_markup_is_styled_but_user_input_remains_literal() {
    let source = "# Heading\n\n**bold** and `code`";
    let rendered = conversation_entry::render_message_content(&assistant(source, true), 40);
    assert!(text(&rendered).contains("bold and code"));
    assert!(!text(&rendered).contains("**"));
    assert!(rendered
        .lines
        .iter()
        .flat_map(|line| &line.spans)
        .any(|span| span.content.contains("bold")
            && span.style.add_modifier.contains(Modifier::BOLD)));
    let user = Message {
        role: Role::User,
        ..assistant(source, true)
    };
    assert!(text(&conversation_entry::render_message_content(&user, 40)).contains("**bold**"));
}

#[test]
fn streaming_defers_unclosed_inline_markup_until_it_is_complete() {
    let pending = conversation_entry::render_message_content(&assistant("Hello **wor", false), 40);
    assert!(text(&pending).contains("Hello"));
    assert!(!text(&pending).contains("wor"));
    assert!(text(&pending).contains('▌'));
    let complete =
        conversation_entry::render_message_content(&assistant("Hello **world**", true), 40);
    assert!(text(&complete).contains("Hello world"));
    assert!(!text(&complete).contains('▌'));
}

#[test]
fn code_copy_coordinates_include_message_padding() {
    let source = "```rust\nlet x = 1;\n```";
    let rendered = conversation_entry::render_message_content(&assistant(source, true), 40);
    let block = &rendered.code_blocks[0];
    assert_eq!(block.text, "let x = 1;");
    assert_eq!(block.top_line, 1);
    let header = rendered.lines[block.top_line].to_string();
    assert!(header
        .chars()
        .skip(block.copy_columns.start)
        .take(block.copy_columns.len())
        .collect::<String>()
        .to_lowercase()
        .contains("copy"));
}

#[test]
fn cache_tracks_replacement_resize_and_stream_completion() {
    let mut cache = MessageCache::default();
    cache.prepare(40, 1);
    let first = assistant("**hello**", false);
    assert!(text(&cache.message(0, &first)).contains('▌'));
    let completed = assistant("**hello**", true);
    assert!(!text(&cache.message(0, &completed)).contains('▌'));
    let replacement = assistant("replacement", true);
    assert!(text(&cache.message(0, &replacement)).contains("replacement"));
    cache.prepare(8, 1);
    assert!(cache
        .message(0, &replacement)
        .lines
        .iter()
        .all(|line| line.width() <= 8));
    cache.prepare(8, 0);
    assert!(cache.entries.is_empty());
}

#[test]
fn rich_assistant_content_renders_in_the_chat_at_wide_and_narrow_widths() {
    let source = "# Summary\n\n**Done** with $x^2$.\n\n| Name | Value |\n| --- | ---: |\n| result | 42 |\n\n```rust\nfn main() {}\n```\n\n$$\\frac{1}{2}$$\n\n```mermaid\nflowchart TD\n A[Start] --> B[Done]\n```\n\nfinal answer";
    for width in [100, 36, 12] {
        let app = App {
            transcript: vec![TranscriptItem::Message(assistant(source, true))],
            ..App::default()
        };
        let mut terminal = Terminal::new(TestBackend::new(width, 80)).unwrap();
        terminal
            .draw(|frame| crate::tui::chat::render(frame, frame.area(), &app))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let visible = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(!visible.contains("**Done**"));
        assert!(visible.contains("final"));
        assert!(visible.contains("answer"));
    }
}

fn assert_same_render(actual: &RenderedMarkdown, expected: &RenderedMarkdown) {
    assert_eq!(
        (
            &actual.lines,
            &actual.code_blocks,
            &actual.image_sources,
            &actual.image_rows
        ),
        (
            &expected.lines,
            &expected.code_blocks,
            &expected.image_sources,
            &expected.image_rows
        )
    );
}

#[test]
fn incremental_render_matches_fresh_at_every_character_and_completion() {
    syntax::warm_syntax_set();
    let source = "# Résumé\n\n**Done** with $x^2$.\n\n| Name | Value |\n| --- | ---: |\nresult | 42\nnext | 1234567\n\n```rust\nlet x = \"你好\";\n```\n\n$$\n\\frac{1}{2}\n$$\n\n```mermaid\nflowchart TD\n A[Start] --> B[Done]\n```\n\n![plot](plot.png)\n\nfinal **answer**";
    for width in [1_usize, 12, 40, 100] {
        let mut cache = MessageCache::default();
        let images = ConversationImages::default();
        let available = ratatui::layout::Size::new(width.saturating_sub(2).max(1) as u16, 30);
        cache.prepare(width, 1);
        for end in source
            .char_indices()
            .map(|(index, _)| index)
            .chain([source.len()])
        {
            let message = assistant(&source[..end], false);
            let actual = cache.message_with_images(0, &message, &images, available);
            let expected = conversation_entry::render_message_content(&message, width);
            assert_same_render(&actual, &expected);
        }
        let completed = assistant(source, true);
        assert_same_render(
            &cache.message_with_images(0, &completed, &images, available),
            &conversation_entry::render_message_content(&completed, width),
        );
    }
}

#[test]
fn stable_code_math_and_mermaid_are_not_rendered_again_on_append() {
    let prefix = "```rust\nfn main() {}\n```\n\n$$\\frac{1}{2}$$\n\n```mermaid\nflowchart TD\n A --> B\n```\n\n";
    let mut message = assistant(prefix, false);
    let mut cache = MessageCache::default();
    cache.prepare(80, 1);
    cache.message(0, &message);
    let initial = cache.entries[0].as_ref().unwrap().stream.as_ref().unwrap();
    let initial_rendered = initial.rendered_bytes;
    let frozen = initial.bytes;
    assert_eq!(&prefix[frozen..], "\n");
    let suffix = "the final answer keeps arriving";
    let mut expected_bytes = 0;
    for ch in suffix.chars() {
        message.text.push(ch);
        cache.message(0, &message);
        expected_bytes += message.text.len() - frozen;
    }
    let stream = cache.entries[0].as_ref().unwrap().stream.as_ref().unwrap();
    assert_eq!(stream.rendered_bytes - initial_rendered, expected_bytes);
    let cached = cache.message(0, &message);
    assert!(Rc::ptr_eq(&cached, &cache.message(0, &message)));
}

#[test]
fn assistant_control_and_bidi_text_is_safe_in_paint_copy_and_streaming() {
    let unsafe_text = "hello\tworld\r\u{1b}[31m\u{061c}\u{200e}\u{200f}\u{202a}\u{202e}\u{2066}\u{2069}\u{7f}\n```text\ncopy\tthis\u{202e}\n```\n";
    let expected_copy = "copy\\tthis\\u{202e}";
    let mut cache = MessageCache::default();
    cache.prepare(100, 1);
    for complete in [false, true] {
        let message = assistant(unsafe_text, complete);
        let rendered = cache.message(0, &message);
        assert_eq!(rendered.code_blocks[0].text, expected_copy);
        assert!(text(&rendered).contains("hello\\tworld\\r\\u{1b}[31m"));
        assert!(!text(&rendered)
            .chars()
            .any(|ch| ch.is_control() && ch != '\n'));
        assert_same_render(
            &rendered,
            &conversation_entry::render_message_content(&message, 100),
        );
    }
}

#[test]
fn cache_invalidates_role_readiness_and_replacement_without_mutating_shared_paint() {
    syntax::warm_syntax_set();
    let mut cache = MessageCache::default();
    cache.prepare(40, 1);
    let mut message = assistant("```rust\nlet x = 1;\n```\n\none", false);
    let snapshot = cache.message(0, &message);
    let expected_snapshot = conversation_entry::render_message_content(&message, 40);
    message.text.push_str(" more\n");
    assert_same_render(
        &cache.message(0, &message),
        &conversation_entry::render_message_content(&message, 40),
    );
    assert_same_render(&snapshot, &expected_snapshot);
    // Exercise the readiness mismatch without changing global syntax state.
    cache.syntax_ready = !syntax::syntax_set_ready();
    cache.prepare(40, 1);
    assert!(cache.entries[0].is_none());
    for replacement in [
        assistant("different **pending**", false),
        Message {
            role: Role::User,
            ..message.clone()
        },
        assistant("short", true),
    ] {
        assert_same_render(
            &cache.message(0, &replacement),
            &conversation_entry::render_message_content(&replacement, 40),
        );
    }
}
