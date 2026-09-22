use super::*;
use crate::tui::markdown_theme::{SyntaxRole, Theme};
use ratatui::style::{Color, Style};

fn highlighter(token: &str) -> BlockHighlighter {
    warm_syntax_set();
    BlockHighlighter::for_language(token).expect("bundled syntax")
}

fn segment_texts(segments: &[HighlightSegment]) -> Vec<&str> {
    segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect()
}

fn role_of(segments: &[HighlightSegment], text: &str) -> Option<SyntaxRole> {
    segments
        .iter()
        .find(|segment| segment.text.trim() == text)
        .unwrap_or_else(|| panic!("no segment for {text:?} in {segments:?}"))
        .role
}

// Covers: rust fence tokens map onto distinct syntax roles
// Owner: pure unit (syntax highlight)
#[test]
fn rust_tokens_map_to_distinct_roles() {
    let mut highlighter = highlighter("rust");
    let segments = highlighter.highlight_line("let answer = 42; // note");

    assert_eq!(
        segment_texts(&segments).concat(),
        "let answer = 42; // note"
    );
    assert_eq!(role_of(&segments, "let"), Some(SyntaxRole::Keyword));
    assert_eq!(role_of(&segments, "42"), Some(SyntaxRole::Constant));
    assert_eq!(role_of(&segments, "// note"), Some(SyntaxRole::Comment));
}

// Covers: multi-line string grammar state survives across highlight_line calls
// Owner: pure unit (syntax highlight)
#[test]
fn string_state_carries_across_lines() {
    let mut highlighter = highlighter("rust");
    highlighter.highlight_line("let text = \"open");
    let segments = highlighter.highlight_line("still inside");

    assert!(segments
        .iter()
        .all(|segment| segment.role == Some(SyntaxRole::String)));
}

// Covers: unknown fence language falls back to no highlighter
// Owner: pure unit (syntax language lookup)
#[test]
fn unknown_language_has_no_highlighter() {
    warm_syntax_set();
    assert!(BlockHighlighter::for_language("no-such-language").is_none());
}

// Covers: TypeScript fence tags resolve after two-face syntax dump
// Owner: pure unit (syntax language lookup)
#[test]
fn typescript_fence_tokens_resolve() {
    warm_syntax_set();
    for token in ["ts", "tsx", "typescript"] {
        assert!(
            BlockHighlighter::for_language(token).is_some(),
            "expected highlighter for fence token {token}"
        );
    }
}

// Covers: common alias tags map onto dump-native grammars
// Owner: pure unit (syntax language lookup)
#[test]
fn common_fence_aliases_resolve() {
    warm_syntax_set();
    for token in ["jsx", "shell", "console", "toml", "ps1", "powershell"] {
        assert!(
            BlockHighlighter::for_language(token).is_some(),
            "expected highlighter for fence token {token}"
        );
    }
}

// Covers: TypeScript keywords and constants get role segments
// Owner: pure unit (syntax highlight)
#[test]
fn typescript_tokens_map_to_roles() {
    let mut highlighter = highlighter("ts");
    let segments = highlighter.highlight_line("const answer: number = 42; // note");

    assert_eq!(
        segment_texts(&segments).concat(),
        "const answer: number = 42; // note"
    );
    assert_eq!(role_of(&segments, "const"), Some(SyntaxRole::Keyword));
    assert_eq!(role_of(&segments, "42"), Some(SyntaxRole::Constant));
    assert_eq!(role_of(&segments, "// note"), Some(SyntaxRole::Comment));
}

// Covers: empty line still yields one plain segment so layout keeps the row
// Owner: pure unit (syntax highlight)
#[test]
fn empty_line_yields_one_empty_plain_segment() {
    let mut highlighter = highlighter("rust");
    let segments = highlighter.highlight_line("");

    assert_eq!(segment_texts(&segments), vec![""]);
    assert_eq!(segments[0].role, None);
}

// Covers: PowerShell cmdlets highlight, double-dash flags do not
// Owner: pure unit (bundled PowerShell grammar)
#[test]
fn powershell_cmdlets_do_not_match_double_dash_flags() {
    let mut highlighter = highlighter("powershell");
    let segments = highlighter.highlight_line("git commit --no-verify; Write-Output 1");
    assert_eq!(
        role_of(&segments, "Write-Output"),
        Some(SyntaxRole::Function)
    );
    assert!(
        segments
            .iter()
            .filter(|segment| segment.text.contains("no-verify"))
            .all(|segment| segment.role != Some(SyntaxRole::Function)),
        "double-dash flags must not paint as cmdlets: {segments:?}"
    );
}

// Covers: callers map plain roles onto their own base style at paint time
// Owner: pure unit (highlight segment style)
#[test]
fn highlight_segment_style_uses_caller_plain() {
    let plain = Theme::text();
    let keyword = HighlightSegment {
        text: "let".into(),
        role: Some(SyntaxRole::Keyword),
    };
    let body = HighlightSegment {
        text: " x".into(),
        role: None,
    };
    assert_eq!(keyword.style(plain), Theme::syntax(SyntaxRole::Keyword));
    assert_eq!(body.style(plain), plain);
    let washed = Style::default().fg(Color::White).bg(Color::Rgb(12, 40, 24));
    assert_eq!(keyword.style(washed).bg, washed.bg);
    assert_eq!(
        keyword.style(washed).fg,
        Theme::syntax(SyntaxRole::Keyword).fg
    );
    assert_eq!(body.style(washed), washed);
}
