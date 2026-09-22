use super::*;
use pretty_assertions::assert_eq;

#[test]
fn wrapping_and_truncation_keep_graphemes_whole_at_cell_boundaries() {
    let source = "e\u{301}界👩‍💻";
    for width in [1, 2] {
        let hard = hard_wrap_ranges(source, width)
            .into_iter()
            .map(|range| &source[range])
            .collect::<Vec<_>>();
        let soft = wrap_line_at_whitespace_ranges_with_protected_prefix(
            source, width, /*protected_prefix_end*/ 0,
        )
        .into_iter()
        .map(|range| &source[range])
        .collect::<Vec<_>>();
        assert_eq!(
            (hard, soft),
            (vec!["e\u{301}", "界", "👩‍💻"], vec!["e\u{301}", "界", "👩‍💻"])
        );
    }
    for (width, expected) in [
        (0, ""),
        (1, "e\u{301}"),
        (2, "e\u{301}"),
        (3, "e\u{301}界"),
        (5, source),
    ] {
        assert_eq!(truncate_to_display_width(source, width), expected);
    }
    assert_eq!(truncate_to_display_width("👩‍💻x", 2), "👩‍💻");
}

#[test]
fn styled_code_wrap_keeps_combining_marks_and_emoji_with_their_text() {
    let style = Style::default().fg(ratatui::style::Color::Blue);
    let source = "e\u{301}👩‍💻";
    assert_eq!(
        hard_wrap_styled_spans(source, &[Span::styled(source, style)], 2, Style::default()),
        vec![
            vec![Span::styled("e\u{301}", style)],
            vec![Span::styled("👩‍💻", style)]
        ]
    );
}
