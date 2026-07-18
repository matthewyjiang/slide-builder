use super::{
    app::{App, Role, ToolStatus, TranscriptItem},
    theme,
};
use ratatui::buffer::Buffer;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Paragraph, Widget, Wrap},
    Frame,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
    frame.render_widget(
        Paragraph::new(Line::styled(" Conversation ", theme::panel_title())),
        regions[0],
    );

    let lines = conversation_lines(app);
    let scroll = scroll_for(&lines, regions[1]);
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        regions[1],
    );
}

fn conversation_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for item in &app.transcript {
        match item {
            TranscriptItem::Message(message) => {
                let (label, color) = match message.role {
                    Role::User => ("You", theme::ACCENT),
                    Role::Assistant => ("Slide-builder", theme::SUCCESS),
                    Role::System => ("Notice", theme::WARNING),
                };
                lines.push(Line::from(Span::styled(
                    label,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                )));
                lines.extend(
                    message.text.lines().map(|line| {
                        Line::styled(line.to_owned(), Style::default().fg(theme::TEXT))
                    }),
                );
                if !message.complete {
                    lines.push(Line::styled("▌", Style::default().fg(theme::SUCCESS)));
                }
                lines.push(Line::from(""));
            }
            TranscriptItem::Tool(card) => {
                let (glyph, color) = match card.status {
                    ToolStatus::Proposed => ("○", theme::MUTED),
                    ToolStatus::Running => ("◌", theme::WARNING),
                    ToolStatus::Succeeded => ("✓", theme::SUCCESS),
                    ToolStatus::Failed => ("✗", theme::DANGER),
                };
                lines.push(Line::from(Span::styled(
                    format!("{glyph} {}", tool_status_text(card)),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                )));
                if !card.detail.is_empty() {
                    lines.push(Line::styled(
                        format!("  {}", card.detail),
                        Style::default().fg(theme::MUTED),
                    ));
                }
                lines.push(Line::from(""));
            }
        }
    }
    if lines.is_empty() {
        lines.extend([
            Line::from(""),
            Line::styled(
                "Build your deck by describing the outcome.",
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::from(""),
            Line::styled(
                "Try “Create a 6-slide launch narrative” or “Make the active slide more visual.”",
                Style::default().fg(theme::MUTED),
            ),
            Line::from(""),
            Line::styled(
                "Use Ctrl+V before sending to include the active slide.",
                Style::default().fg(theme::MUTED),
            ),
        ]);
    }
    lines
}

fn scroll_for(lines: &[Line<'_>], area: Rect) -> u16 {
    let width = area.width.max(1) as usize;
    let rendered_lines = lines
        .iter()
        .map(|line| line.width().max(1).div_ceil(width))
        .sum::<usize>();
    rendered_lines.saturating_sub(area.height as usize) as u16
}

pub(crate) fn visible_text_rows(area: Rect, app: &App) -> Vec<String> {
    if area.width == 0 || area.height <= 1 {
        return vec![];
    }
    let body = Rect::new(0, 0, area.width, area.height - 1);
    let lines = conversation_lines(app);
    let scroll = scroll_for(&lines, body);
    let mut buffer = Buffer::empty(body);
    Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .render(body, &mut buffer);
    (0..body.height)
        .map(|y| {
            let mut row = String::new();
            for x in 0..body.width {
                row.push_str(buffer[(x, y)].symbol());
            }
            row.trim_end().to_owned()
        })
        .collect()
}

struct ToolVerbs {
    proposed: &'static str,
    running: &'static str,
    succeeded: &'static str,
}

fn tool_status_text(card: &super::app::ToolCard) -> String {
    let verbs = tool_verbs(&card.name);
    let verb = match card.status {
        ToolStatus::Proposed => verbs.proposed,
        ToolStatus::Running => verbs.running,
        ToolStatus::Succeeded => verbs.succeeded,
        ToolStatus::Failed => {
            return format!(
                "Could not {} {}",
                verbs.proposed.to_lowercase(),
                card.summary
            )
        }
    };
    format!("{verb} {}", card.summary)
}

fn tool_verbs(name: &str) -> ToolVerbs {
    let (proposed, running, succeeded) = match name {
        "slide_create" | "text_add" | "image_add" | "shape_add" => ("Add", "Adding", "Added"),
        "slide_duplicate" => ("Duplicate", "Duplicating", "Duplicated"),
        "slide_delete" => ("Delete", "Deleting", "Deleted"),
        "slide_reorder" => ("Move", "Moving", "Moved"),
        "element_update" | "deck_advanced" => ("Update", "Updating", "Updated"),
        "deck_inspect" => ("Inspect", "Inspecting", "Inspected"),
        "deck_validate" => ("Validate", "Validating", "Validated"),
        "render_deck" => ("Request", "Requesting", "Requested"),
        "set_active_slide" => ("Select", "Selecting", "Selected"),
        "list_dir" => ("List", "Listing", "Listed"),
        "read_file" | "get_search_content" => ("Read", "Reading", "Read"),
        "write_file" => ("Write", "Writing", "Wrote"),
        "edit_file" => ("Edit", "Editing", "Edited"),
        "load_skill" | "discover_instructions" => ("Load", "Loading", "Loaded"),
        "web_search" => ("Search", "Searching", "Searched"),
        _ => ("Run", "Running", "Ran"),
    };
    ToolVerbs {
        proposed,
        running,
        succeeded,
    }
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;
