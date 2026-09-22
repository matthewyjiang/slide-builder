use ratatui::{
    style::Style,
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use super::{
    app::{Message, Role, ToolCard, ToolStatus},
    theme,
};

#[cfg(test)]
pub(crate) fn render_message(message: &Message, width: usize) -> Vec<Line<'static>> {
    render_message_content(message, width).lines
}

pub(crate) fn render_message_content(
    message: &Message,
    width: usize,
) -> super::markdown::RenderedMarkdown {
    let width = width.max(1);
    let inner_width = padded_inner_width(width);
    if message.role == Role::Assistant {
        return super::conversation_markdown::render(message, width, inner_width);
    }
    let style = match message.role {
        Role::User => theme::user_message(),
        Role::Assistant => theme::assistant_message(),
        Role::System => theme::system_message(),
    };
    let mut rows = wrapped_rows(&message.text, inner_width, style);
    if !message.complete {
        rows.push(("▌".into(), style.fg(theme::SUCCESS)));
    }
    super::markdown::RenderedMarkdown {
        lines: render_block(rows, width, style),
        code_blocks: vec![],
        image_sources: vec![],
        image_rows: vec![],
    }
}

pub(crate) fn render_tool(card: &ToolCard, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let (glyph, color) = match card.status {
        ToolStatus::Proposed => ("○", theme::MUTED),
        ToolStatus::Running => ("◌", theme::WARNING),
        ToolStatus::Succeeded => ("✓", theme::SUCCESS),
        ToolStatus::Failed => ("✗", theme::DANGER),
    };
    let mut rows = plain_rows(
        &format!("{glyph} {}", tool_status_text(card)),
        width,
        Style::default(),
    );
    if let Some(first) = rows.first_mut() {
        let text = first.to_string();
        *first = Line::from(vec![
            Span::styled(glyph, Style::default().fg(color)),
            Span::raw(text[glyph.len()..].to_owned()),
        ]);
    }
    rows
}

pub(crate) fn tool_status_text(card: &ToolCard) -> String {
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

fn render_block(
    rows: Vec<(String, Style)>,
    width: usize,
    block_style: Style,
) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(rows.len() + 2);
    lines.push(blank_line(width, block_style));
    lines.extend(
        rows.into_iter()
            .map(|(content, style)| padded_line(content, width, style)),
    );
    lines.push(blank_line(width, block_style));
    lines
}

fn wrapped_rows(text: &str, width: usize, style: Style) -> Vec<(String, Style)> {
    let mut rows = Vec::new();
    for line in text.lines() {
        rows.extend(wrap_line(line, width).into_iter().map(|line| (line, style)));
    }
    if rows.is_empty() {
        rows.push((String::new(), style));
    }
    rows
}

pub(crate) fn plain_rows(text: &str, width: usize, style: Style) -> Vec<Line<'static>> {
    wrapped_rows(text, width, style)
        .into_iter()
        .map(|(text, style)| Line::styled(text, style))
        .collect()
}

fn wrap_line(line: &str, width: usize) -> Vec<String> {
    super::render::wrap_line_at_whitespace_ranges_with_protected_prefix(
        line, width, /*protected_prefix_end*/ 0,
    )
    .into_iter()
    .map(|range| line[range].to_owned())
    .collect()
}

fn padded_line(content: String, width: usize, style: Style) -> Line<'static> {
    let inner_width = padded_inner_width(width);
    let trailing = inner_width.saturating_sub(content.width());
    let text = if width < 3 {
        format!("{content}{}", " ".repeat(trailing))
    } else {
        format!(" {content}{} ", " ".repeat(trailing))
    };
    Line::from(Span::styled(text, style))
}

fn padded_inner_width(width: usize) -> usize {
    if width < 3 {
        width.max(1)
    } else {
        width - 2
    }
}

fn blank_line(width: usize, style: Style) -> Line<'static> {
    Line::from(Span::styled(" ".repeat(width), style))
}

struct ToolVerbs {
    proposed: &'static str,
    running: &'static str,
    succeeded: &'static str,
}

fn tool_verbs(name: &str) -> ToolVerbs {
    let (proposed, running, succeeded) = match name {
        "slide_create" | "text_add" | "image_add" | "shape_add" => ("Add", "Adding", "Added"),
        "slide_duplicate" => ("Duplicate", "Duplicating", "Duplicated"),
        "slide_delete" => ("Delete", "Deleting", "Deleted"),
        "slide_reorder" => ("Move", "Moving", "Moved"),
        "element_update" | "deck_advanced" | "deck_layout_set" => ("Update", "Updating", "Updated"),
        "elements_layout" => ("Apply", "Applying", "Applied"),
        "deck_inspect" | "deck_layout_inspect" => ("Inspect", "Inspecting", "Inspected"),
        "deck_layout_audit" => ("Check", "Checking", "Checked"),
        "deck_validate" => ("Validate", "Validating", "Validated"),
        "render_deck" => ("Render", "Rendering", "Rendered"),
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
#[path = "conversation_entry_tests.rs"]
mod tests;
