use super::*;
use pretty_assertions::assert_eq;
use ratatui::style::Style;

fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn line_styles(line: &Line<'_>) -> Vec<Style> {
    line.spans.iter().map(|span| span.style).collect()
}

#[test]
fn keeps_list_markers_with_a_long_path_in_narrow_output() {
    for marker in ["-", "1.", "2)"] {
        let markdown =
            format!("{marker} fixtures/downstream/no-default-features/Cargo.toml: package 0.0.0");
        let lines = markdown_lines(&markdown, 39);

        let first_line_suffix_len = 39 - marker.len() - 1;
        let path = "fixtures/downstream/no-default-features/Cargo.toml: package 0.0.0";
        assert_eq!(
            lines.iter().map(line_text).collect::<Vec<_>>(),
            vec![
                format!("{marker} {}", &path[..first_line_suffix_len]),
                path[first_line_suffix_len..].to_string(),
            ]
        );
    }
}

#[test]
fn preserves_underscores_inside_identifiers() {
    let lines = markdown_lines("keep foo_bar_baz literal but style _this_", 120);

    assert_eq!(
        line_text(&lines[0]),
        "keep foo_bar_baz literal but style this"
    );
    assert!(line_styles(&lines[0]).contains(&Theme::markdown_italic()));
}

#[test]
fn wraps_long_unicode_styled_lines_without_losing_text_or_styles() {
    let plain_prefix = "éλ".repeat(256);
    let bold = "你🙂".repeat(256);
    let plain_suffix = "界ß".repeat(256);
    let markdown = format!("{plain_prefix} **{bold}** {plain_suffix}");
    let expected = format!("{plain_prefix} {bold} {plain_suffix}");
    let lines = markdown_lines(&markdown, 17);
    let rendered = lines.iter().map(line_text).collect::<String>();
    let rendered_bold = lines
        .iter()
        .flat_map(|line| &line.spans)
        .filter(|span| span.style == Theme::markdown_bold())
        .map(|span| span.content.as_ref())
        .collect::<String>();

    assert_eq!(rendered, expected);
    assert_eq!(rendered_bold, bold);
    assert!(lines
        .iter()
        .all(|line| display_width(&line_text(line)) <= 17));
}

#[test]
fn code_block_rows_use_the_full_pane_width_without_borders() {
    let lines = markdown_lines("```\n你好你好\n```", 6);

    // Header row plus content rows, no bottom border.
    assert_eq!(lines.len(), 3);
    assert_eq!(line_text(&lines[1]), "你好你");
    assert_eq!(line_text(&lines[2]), "好");
    assert!(lines
        .iter()
        .all(|line| !line_text(line).contains(['╭', '╮', '╰', '╯', '│'])));
}

#[test]
fn code_blocks_preserve_markdown_markers_as_literal_text() {
    let lines = markdown_lines("```\nfn __init__() { println!(\"*ok*\"); }\n```", 80);

    assert!(line_text(&lines[1]).contains("fn __init__() { println!(\"*ok*\"); }"));
    assert_eq!(line_styles(&lines[1]), vec![Theme::code_text()]);
}

#[test]
fn code_block_header_shows_language_label_and_copy_button() {
    let rendered = render_markdown("```rust\nlet x = 1;\n```", 40);

    let header = &rendered.lines[0];
    // COPY keeps one blank column of inset from the right pane edge.
    assert_eq!(display_width(&line_text(header)), 39);
    assert!(line_text(header).starts_with("RUST"));
    assert!(line_text(header).ends_with(" COPY "));
    assert!(line_styles(header).contains(&Theme::dim()));
}

#[test]
fn highlighted_code_blocks_style_tokens_and_keep_literal_text() {
    crate::tui::syntax::warm_syntax_set();
    let lines = markdown_lines("```rust\nlet answer = 42; // note\n```", 80);

    assert_eq!(line_text(&lines[1]), "let answer = 42; // note");
    let styles = line_styles(&lines[1]);
    assert!(styles.len() > 1, "expected highlighted spans: {styles:?}");
    assert!(styles.iter().any(|style| *style != Theme::code_text()));
}

#[test]
fn unknown_language_code_blocks_fall_back_to_plain_styling() {
    let lines = markdown_lines("```no-such-language\nplain text body\n```", 80);

    assert!(line_text(&lines[0]).starts_with("NO-SUCH-LANGUAGE"));
    assert_eq!(line_text(&lines[1]), "plain text body");
    assert_eq!(line_styles(&lines[1]), vec![Theme::code_text()]);
}

#[test]
fn code_fence_closers_match_marker_length_and_allow_only_whitespace() {
    let opening = parse_opening_fence("   ````mermaid").expect("valid opening fence");
    assert_eq!(opening.marker, '`');
    assert_eq!(opening.length, 4);
    assert!(!is_closing_fence("```", opening));
    assert!(!is_closing_fence("~~~~", opening));
    assert!(!is_closing_fence("````not-a-close", opening));
    assert!(is_closing_fence("  `````   ", opening));
    assert!(parse_opening_fence("    ```rust").is_none());
    assert!(parse_opening_fence("```rust`edition").is_none());
}

#[test]
fn open_code_block_preserves_multiline_string_highlighting_and_copy_source() {
    crate::tui::syntax::warm_syntax_set();
    let source = "let text = \"open\nstill inside\nmore";
    let rendered = render_markdown(&format!("```rust\n{source}\n"), 80);
    let string_style = Theme::syntax(crate::tui::markdown_theme::SyntaxRole::String);
    assert_eq!(
        rendered.lines[2],
        Line::from(Span::styled("still inside", string_style))
    );
    assert_eq!(
        rendered.lines[3],
        Line::from(Span::styled("more", string_style))
    );
    assert_eq!(rendered.code_blocks[0].text, source);
}

#[test]
fn mermaid_scanner_keeps_an_invalid_closer_inside_the_raw_block() {
    let rendered = render_markdown(
        "````mermaid\nflowchart TD\nA[one]\n```not-a-close\nA --> B[two]\n````",
        80,
    );
    let text = rendered
        .lines
        .iter()
        .map(line_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("MERMAID"), "{text}");
    assert!(text.contains("one"), "{text}");
    assert!(text.contains("two"), "{text}");
}

#[test]
fn open_mermaid_fence_paints_complete_line_prefix() {
    let open = render_markdown("```mermaid\nflowchart LR\nA --> B\n", 60);
    let open_text = open.lines.iter().map(line_text).collect::<Vec<_>>();

    assert!(open_text[0].starts_with("MERMAID"));
    assert!(
        !open_text.iter().any(|line| line.contains("flowchart LR")),
        "open complete-line prefix should already be art: {open_text:?}"
    );

    let sticky = render_markdown("```mermaid\npie\n\"Dogs\" : 5\n", 60);
    let sticky_text = sticky.lines.iter().map(line_text).collect::<Vec<_>>();
    assert!(sticky_text.iter().any(|line| line.contains("pie")));

    let closed = render_markdown("```mermaid\nflowchart LR\nA --> B\n```", 60);
    assert!(line_text(&closed.lines[0]).contains("MERMAID"));
    assert!(!closed
        .lines
        .iter()
        .map(line_text)
        .any(|line| line.contains("flowchart LR")));
}

#[test]
fn mermaid_render_reflows_to_the_requested_transcript_width() {
    let markdown = "```mermaid\nflowchart LR\nA[Parse] --> B[Render]\n```";
    let wide = markdown_lines(markdown, 80);
    let narrow = markdown_lines(markdown, 36);

    assert!(wide
        .iter()
        .all(|line| display_width(&line_text(line)) <= 80));
    assert!(narrow
        .iter()
        .all(|line| display_width(&line_text(line)) <= 36));
    assert_ne!(
        wide.iter().map(line_text).collect::<Vec<_>>(),
        narrow.iter().map(line_text).collect::<Vec<_>>()
    );
}

#[test]
fn image_syntax_inside_code_fence_stays_literal() {
    let lines = markdown_lines("```\n![diagram](arch.png)\n```", 120);
    let text: Vec<String> = lines.iter().map(line_text).collect();

    assert!(text
        .iter()
        .any(|line| line.contains("![diagram](arch.png)")));
}

#[test]
fn preview_keeps_complete_lines_and_only_the_stable_open_line_prefix() {
    for (source, visible) in [
        ("", ""),
        ("done\n", "done\n"),
        ("done\nbefore **bold", "done\nbefore "),
        ("done\n**bold", "done\n"),
        ("done\nbefore **bold** tail", "done\nbefore **bold** tail"),
        ("## heading *pending", "## heading "),
        ("see [label](https://ex", "see "),
        ("math $x^2$ then $y", "math $x^2$ then "),
        ("done\n`", "done\n"),
        ("done\n  ``", "done\n"),
        ("done\n```rust", "done\n"),
        ("done\n\u{200b}", "done\n"),
        ("done\n**\u{200b}**", "done\n"),
        ("done\n  ", "done\n  "),
        ("```rust\n**literal", "```rust\n**literal"),
        ("~~~rust\n**literal", "~~~rust\n**literal"),
        ("````rust\n```\n**literal", "````rust\n```\n**literal"),
        ("```rust\n~~~\n**literal", "```rust\n~~~\n**literal"),
        ("```rust\n\u{200b}", "```rust\n"),
        ("```rust\n```\n**pending", "```rust\n```\n"),
    ] {
        assert_eq!(
            &source[..markdown_preview_end(source)],
            visible,
            "{source:?}"
        );
    }
}

#[test]
fn stable_prefix_stops_at_earliest_open_marker() {
    assert_eq!(
        inline_markdown_stable_prefix_len("before **bold"),
        "before ".len()
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("before *it **also"),
        "before ".len(),
        "earlier italic must win over later bold"
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("before **bold `code"),
        "before ".len(),
        "earlier bold must win over later code"
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("before `code **bold"),
        "before ".len()
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("a _x **y"),
        "a ".len(),
        "earlier underscore italic must win over later bold"
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("done **x** tail"),
        "done **x** tail".len()
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("text *a* **b"),
        "text *a* ".len()
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("see [x](http"),
        "see ".len()
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("see [x]"),
        "see ".len(),
        "trailing ] may still become a link target"
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("see [x] plain"),
        "see [x] plain".len(),
        "] followed by non-( stays plain text"
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("go https://ex"),
        "go ".len()
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("before [link](https://x) **b"),
        "before [link](https://x) ".len()
    );
}

// Covers: open $ math must hold the preview while currency dollars stay plain
// Owner: pure unit (markdown inline math streaming bounds)
#[test]
fn stable_prefix_holds_open_inline_math_but_not_currency() {
    assert_eq!(
        inline_markdown_stable_prefix_len("before $x^2"),
        "before ".len()
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("pay $"),
        "pay ".len(),
        "a trailing $ may still open math once more input arrives"
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("done $x^2$ tail"),
        "done $x^2$ tail".len()
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("cost $5 and more"),
        "cost $5 and more".len(),
        "a $ before a digit is currency, not math"
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("a `$x` b"),
        "a `$x` b".len(),
        "$ inside a code span never opens math"
    );
    assert_eq!(
        inline_markdown_stable_prefix_len("closed $a_i$ then *em"),
        "closed $a_i$ then ".len(),
        "markers after closed math must still hold"
    );
}

// Covers: closed $...$ renders single-row art while currency and tall math stay literal
// Owner: pure unit (markdown inline math integration)
#[test]
fn renders_single_row_inline_math_in_prose() {
    let lines = markdown_lines("energy $E = mc^2$ done", 80);
    let text = lines.iter().map(line_text).collect::<Vec<_>>();
    assert_eq!(text, vec!["energy E = mc² done"]);

    let currency = markdown_lines("that costs $5 and $10 total", 80);
    let currency_text = currency.iter().map(line_text).collect::<Vec<_>>();
    assert_eq!(currency_text, vec!["that costs $5 and $10 total"]);

    let tall = markdown_lines(r"half is $\frac{1}{2}$ here", 80);
    let tall_text = tall.iter().map(line_text).collect::<Vec<_>>();
    assert_eq!(
        tall_text,
        vec![r"half is $\frac{1}{2}$ here"],
        "multi-row inline math must keep its literal source"
    );

    let code = markdown_lines("run `echo $x^2$` now", 80);
    let code_text = code.iter().map(line_text).collect::<Vec<_>>();
    assert!(
        code_text.iter().any(|line| line.contains("echo $x^2$")),
        "code spans keep dollars literal: {code_text:?}"
    );
}

// Covers: closed $$ blocks must render through the markdown panel path
// Owner: pure unit (markdown display math integration)
#[test]
fn renders_closed_display_math_blocks_in_markdown() {
    let multi = markdown_lines("before\n$$\n\\frac{a}{b}\n$$\nafter", 40);
    let multi_text = multi.iter().map(line_text).collect::<Vec<_>>();
    assert!(
        multi_text.iter().any(|line| line.contains("MATH")),
        "{multi_text:?}"
    );
    assert!(
        multi_text
            .iter()
            .any(|line| line.contains('a') && !line.contains("\\frac")),
        "{multi_text:?}"
    );
    assert!(
        multi_text.iter().any(|line| line == "after"),
        "{multi_text:?}"
    );
    let single = markdown_lines("$$x^2 + y^2$$", 40);
    let single_text = single.iter().map(line_text).collect::<Vec<_>>();
    assert!(
        single_text.iter().any(|line| line.contains("MATH")),
        "{single_text:?}"
    );
    assert!(
        !single_text.iter().any(|line| line.contains("$$")),
        "{single_text:?}"
    );
}

// Covers: $$ inside fences and open $$ tails must not false-commit
// Owner: pure unit (markdown display math streaming bounds)
#[test]
fn keeps_fenced_and_open_display_math_literal() {
    let fenced = markdown_lines("```text\n$$x^2$$\n```", 40);
    let fenced_text = fenced.iter().map(line_text).collect::<Vec<_>>();
    assert!(
        fenced_text.iter().any(|line| line.contains("$$x^2$$")),
        "{fenced_text:?}"
    );
    assert!(
        !fenced_text.iter().any(|line| line.contains("MATH")),
        "{fenced_text:?}"
    );

    let open = "intro\n$$\n\\frac{a}{b}";
    let open_render = render_markdown("$$\na_i * b_j\n", 40);
    assert!(open_render
        .lines
        .iter()
        .map(line_text)
        .any(|line| line.contains("a_i * b_j")));
    assert_eq!(open_render.code_blocks[0].text, "a_i * b_j");
    assert_eq!(
        incremental_markdown_tail_start(open),
        "intro\n".len(),
        "open multi-line math must stay in the mutable tail"
    );
    let closed_then_prose = "intro\n$$\n\\frac{a}{b}\n$$\nafter";
    assert_eq!(
        incremental_markdown_tail_start(closed_then_prose),
        "intro\n$$\n\\frac{a}{b}\n$$\n".len(),
        "prose after a closed math block becomes the trailing block"
    );
}
