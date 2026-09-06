//! Compact group summaries and progressively disclosed call details.
use std::ops::Range;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::{
    app::{App, ToolStatus, TranscriptItem},
    conversation_entry::{plain_rows, render_tool},
    theme,
    tool_activity::ActivityFocus,
};

pub(crate) struct ActivityRows {
    pub lines: Vec<Line<'static>>,
    pub focused_row: Option<usize>,
}

pub(crate) fn render(app: &App, group: Range<usize>, width: usize) -> ActivityRows {
    let state = &app.tool_activity;
    let expanded = state.is_expanded(&group, &app.transcript);
    let mut pending = false;
    let mut failed = false;
    for item in &app.transcript[group.clone()] {
        if let TranscriptItem::Tool(card) = item {
            match card.status {
                ToolStatus::Proposed | ToolStatus::Running => pending = true,
                ToolStatus::Failed => failed = true,
                ToolStatus::Succeeded => {}
            }
        }
    }
    let (glyph, label, color) = if failed {
        ("!", "Tool activity has errors", theme::DANGER)
    } else if pending || (app.run_active && group.end == app.transcript.len()) {
        ("◌", "Tool activity", theme::WARNING)
    } else {
        ("✓", "Completed tool activity", theme::SUCCESS)
    };
    let count = group.len();
    let noun = if count == 1 {
        "operation"
    } else {
        "operations"
    };
    let disclosure = if expanded { "▾" } else { "▸" };
    let header = format!("{disclosure} {glyph} {label} · {count} {noun}");
    let focused = state.focus == Some(ActivityFocus::Group(group.start));
    let mut lines = indented(
        &header,
        width,
        1,
        Style::default().add_modifier(Modifier::BOLD),
    );
    if let Some(first) = lines.first_mut() {
        // Only the status glyph is semantic color; focus has its own cyan surface.
        let text = first.to_string();
        let prefix = format!(" {disclosure} ");
        if let Some(rest) = text.strip_prefix(&format!("{prefix}{glyph}")) {
            *first = Line::from(vec![
                Span::raw(prefix),
                Span::styled(glyph, Style::default().fg(color)),
                Span::raw(rest.to_owned()),
            ])
            .style(Style::default().add_modifier(Modifier::BOLD));
        }
    }
    let mut focused_row = focused.then_some(0);
    if focused {
        highlight(&mut lines);
    }
    if expanded {
        for index in group.clone() {
            let TranscriptItem::Tool(card) = &app.transcript[index] else {
                continue;
            };
            let selected = state.focus
                == Some(ActivityFocus::Call {
                    group: group.start,
                    index,
                });
            let indent = 3.min(width.saturating_sub(1));
            let mut heading = render_tool(card, width.saturating_sub(indent).max(1));
            for row in &mut heading {
                row.spans.insert(0, Span::raw(" ".repeat(indent)));
            }
            if selected {
                focused_row = Some(lines.len());
                highlight(&mut heading);
            }
            lines.extend(heading);
            if state.details_open(index) {
                lines.extend(indented(
                    &format!("Tool: {}", card.name),
                    width,
                    5,
                    Style::default().fg(theme::MUTED),
                ));
                if !card.arguments.is_empty() {
                    lines.extend(indented(
                        "Arguments",
                        width,
                        5,
                        Style::default().add_modifier(Modifier::BOLD),
                    ));
                    lines.extend(indented(&card.arguments, width, 5, Style::default()));
                }
                let label = match card.status {
                    ToolStatus::Failed => "Error",
                    ToolStatus::Proposed | ToolStatus::Running => "Progress",
                    ToolStatus::Succeeded => "Output",
                };
                lines.extend(indented(
                    label,
                    width,
                    5,
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                let detail = if card.detail.is_empty() {
                    "No output recorded."
                } else {
                    &card.detail
                };
                lines.extend(indented(detail, width, 5, Style::default()));
            } else if card.status == ToolStatus::Failed && !card.detail.is_empty() {
                lines.extend(indented(&card.detail, width, 5, Style::default()));
            }
        }
    }
    ActivityRows { lines, focused_row }
}

fn indented(text: &str, width: usize, indent: usize, style: Style) -> Vec<Line<'static>> {
    let indent = indent.min(width.saturating_sub(1));
    // Raw tool output is data, not terminal control sequences.
    let text: String = text
        .chars()
        .map(|character| {
            if character.is_control() && character != '\n' {
                character.escape_default().to_string()
            } else {
                character.to_string()
            }
        })
        .collect();
    let mut rows = plain_rows(&text, width.saturating_sub(indent).max(1), style);
    for row in &mut rows {
        row.spans.insert(0, Span::raw(" ".repeat(indent)));
    }
    rows
}

fn highlight(rows: &mut [Line<'static>]) {
    for row in rows {
        row.style = theme::accent_block().add_modifier(Modifier::BOLD);
        for span in &mut row.spans {
            span.style = Style::default();
        }
    }
}
