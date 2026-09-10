use super::*;

#[test]
fn layout_keeps_text_and_cursor_in_the_same_cell_coordinates() {
    for (text, cursor, width, expected_lines, expected_cursor) in [
        ("hello world", 11, 8, vec!["hello wo", "rld"], (1, 3)),
        ("hello world", 8, 8, vec!["hello wo", "rld"], (1, 0)),
        ("one\n\ntwo\n", 9, 8, vec!["one", "", "two", ""], (3, 0)),
        ("12345678", 8, 8, vec!["12345678", ""], (1, 0)),
        ("12345678\nx", 10, 8, vec!["12345678", "x"], (1, 1)),
        ("abc界x", 6, 4, vec!["abc", "界x"], (1, 2)),
        ("e\u{301}界x", 7, 3, vec!["e\u{301}界", "x"], (1, 1)),
        ("a  b c", 6, 4, vec!["a  b", " c"], (1, 2)),
    ] {
        let layout = ComposerLayout::new(text, cursor, width);
        let lines: Vec<String> = layout.lines.iter().map(ToString::to_string).collect();
        assert_eq!(
            (lines, layout.cursor),
            (
                expected_lines.into_iter().map(String::from).collect(),
                expected_cursor
            ),
            "{text:?} at {cursor}"
        );
    }
}
