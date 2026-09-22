//! Syntect scope highlighting for fenced Markdown source.
//!
//! The build script merges two-face's grammars and bundled PowerShell syntax.
//! Call `warm_syntax_set` off the UI thread, then invalidate plain-text caches
//! when `syntax_set_ready` becomes true. Roles use the terminal palette, not a
//! fixed syntect theme. See markdown/PROVENANCE.md for upstream provenance.

use std::sync::{LazyLock, OnceLock};

use ratatui::style::Style;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

use super::markdown_theme::{SyntaxRole, Theme};

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();

pub(super) fn syntax_set_ready() -> bool {
    SYNTAX_SET.get().is_some()
}

/// Inflate the grammar dump and role selectors. Safe to call more than once.
pub(crate) fn warm_syntax_set() {
    SYNTAX_SET.get_or_init(|| {
        syntect::dumps::from_uncompressed_data(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/syntaxes-newlines.bin"
        )))
        .expect("bundled syntax dump must be valid")
    });
    LazyLock::force(&ROLE_SELECTORS);
}

/// Scope prefixes in match order: specific selectors before general ones.
static ROLE_SELECTORS: LazyLock<Vec<(Scope, SyntaxRole)>> = LazyLock::new(|| {
    [
        ("entity.name.function", SyntaxRole::Function),
        ("support.function", SyntaxRole::Function),
        ("entity.name.type", SyntaxRole::Type),
        ("support.type", SyntaxRole::Type),
        ("comment", SyntaxRole::Comment),
        ("string", SyntaxRole::String),
        ("constant", SyntaxRole::Constant),
        ("keyword", SyntaxRole::Keyword),
        ("storage", SyntaxRole::Keyword),
    ]
    .into_iter()
    .map(|(selector, role)| (Scope::new(selector).expect("valid scope selector"), role))
    .collect()
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HighlightSegment {
    pub(super) text: String,
    pub(super) role: Option<SyntaxRole>,
}

impl HighlightSegment {
    pub(super) fn style(&self, plain: Style) -> Style {
        match self.role {
            Some(role) => plain.patch(Theme::syntax(role)),
            None => plain,
        }
    }
}

/// Stateful highlighter for one source stream. Feed lines in order.
#[derive(Clone)]
pub(super) struct BlockHighlighter {
    parse: ParseState,
    stack: ScopeStack,
}

impl BlockHighlighter {
    /// Unknown languages or an unready grammar dump fall back to plain text.
    pub(super) fn for_language(token: &str) -> Option<Self> {
        let syntax = SYNTAX_SET
            .get()?
            .find_syntax_by_token(canonical_language_token(token))?;
        Some(Self::from_syntax(syntax))
    }

    fn from_syntax(syntax: &SyntaxReference) -> Self {
        Self {
            parse: ParseState::new(syntax),
            stack: ScopeStack::new(),
        }
    }

    /// Role segments for one source line, without a trailing newline.
    pub(super) fn highlight_line(&mut self, line: &str) -> Vec<HighlightSegment> {
        let mut text = String::with_capacity(line.len() + 1);
        text.push_str(line);
        text.push('\n');
        let syntax_set = SYNTAX_SET
            .get()
            .expect("highlighter requires ready syntax set");
        let Ok(ops) = self.parse.parse_line(&text, syntax_set) else {
            return vec![HighlightSegment {
                text: line.to_string(),
                role: None,
            }];
        };
        let mut segments = Vec::new();
        let mut cursor = 0;
        for (offset, op) in ops {
            let offset = offset.min(line.len());
            if offset > cursor {
                push_merged(&mut segments, &line[cursor..offset], self.scope_role());
                cursor = offset;
            }
            let _ = self.stack.apply(&op);
        }
        if cursor < line.len() {
            push_merged(&mut segments, &line[cursor..], self.scope_role());
        }
        if segments.is_empty() {
            segments.push(HighlightSegment {
                text: String::new(),
                role: None,
            });
        }
        segments
    }

    /// Advance a committed streaming row without allocating styled segments.
    #[cfg(test)]
    pub(super) fn advance_line(&mut self, line: &str) {
        let mut text = String::with_capacity(line.len() + 1);
        text.push_str(line);
        text.push('\n');
        let syntax_set = SYNTAX_SET
            .get()
            .expect("highlighter requires ready syntax set");
        if let Ok(ops) = self.parse.parse_line(&text, syntax_set) {
            for (_, op) in ops {
                let _ = self.stack.apply(&op);
            }
        }
    }

    fn scope_role(&self) -> Option<SyntaxRole> {
        for scope in self.stack.as_slice().iter().rev() {
            for (selector, role) in ROLE_SELECTORS.iter() {
                if selector.is_prefix_of(*scope) {
                    return Some(*role);
                }
            }
        }
        None
    }
}

fn push_merged(segments: &mut Vec<HighlightSegment>, text: &str, role: Option<SyntaxRole>) {
    if let Some(last) = segments.last_mut().filter(|last| last.role == role) {
        last.text.push_str(text);
        return;
    }
    segments.push(HighlightSegment {
        text: text.to_string(),
        role,
    });
}

fn canonical_language_token(token: &str) -> &str {
    match token {
        "jsx" => "javascript",
        "ps1" => "powershell",
        "shell" | "console" => "bash",
        other => other,
    }
}

#[cfg(test)]
#[path = "syntax_tests.rs"]
mod tests;
