use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::{
    app::{Message, Role, ToolCard, ToolStatus},
    theme,
};

pub(crate) fn render_message(message: &Message, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let inner_width = padded_inner_width(width);
    let style = match message.role {
        Role::User => theme::user_message(),
        Role::Assistant => theme::assistant_message(),
        Role::System => theme::system_message(),
    };
    let mut rows = wrapped_rows(&message.text, inner_width, style);
    if !message.complete {
        rows.push(("▌".into(), style.fg(theme::SUCCESS)));
    }
    render_block(rows, width, style)
}

pub(crate) fn render_tool(card: &ToolCard, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let inner_width = padded_inner_width(width);
    let (glyph, heading_style) = match card.status {
        ToolStatus::Proposed => ("○", theme::tool_proposed()),
        ToolStatus::Running => ("◌", theme::tool_running()),
        ToolStatus::Succeeded => ("✓", theme::tool_succeeded()),
        ToolStatus::Failed => ("✗", theme::tool_failed()),
    };
    let block_style = heading_style
        .fg(theme::TEXT)
        .remove_modifier(Modifier::BOLD);
    let mut rows = wrapped_rows(
        &format!("{glyph} {}", tool_status_text(card)),
        inner_width,
        heading_style,
    );
    if !card.detail.is_empty() {
        rows.extend(wrapped_rows(
            &format!("  {}", card.detail),
            inner_width,
            block_style.fg(theme::MUTED),
        ));
    }
    render_block(rows, width, block_style)
}

pub(crate) fn render_empty_state(width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut lines = vec![Line::from("")];
    lines.extend(plain_rows(
        "Build your deck by describing the outcome.",
        width,
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::from(""));
    lines.extend(plain_rows(
        "Try “Create a 6-slide launch narrative” or “Make the active slide more visual.”",
        width,
        Style::default().fg(theme::MUTED),
    ));
    lines.push(Line::from(""));
    lines.extend(plain_rows(
        "Use Ctrl+V before sending to include the active slide.",
        width,
        Style::default().fg(theme::MUTED),
    ));
    lines
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

fn plain_rows(text: &str, width: usize, style: Style) -> Vec<Line<'static>> {
    wrapped_rows(text, width, style)
        .into_iter()
        .map(|(text, style)| Line::styled(text, style))
        .collect()
}

fn wrap_line(line: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    if line.is_empty() {
        return vec![String::new()];
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    while start < line.len() {
        let mut used_width = 0;
        let mut last_fitting_split = None;
        let mut whitespace_break = None;
        let mut saw_non_whitespace = false;
        let mut overflow = false;
        let mut prefer_width_split = false;

        for (relative_index, grapheme) in line[start..].grapheme_indices(true) {
            let grapheme_width = grapheme.width();
            let next = start + relative_index + grapheme.len();
            if used_width == 0 && grapheme_width > width {
                last_fitting_split = Some(next);
                overflow = true;
                break;
            }
            if used_width > 0 && used_width + grapheme_width > width {
                overflow = true;
                prefer_width_split = grapheme.chars().all(char::is_whitespace);
                break;
            }

            used_width += grapheme_width;
            last_fitting_split = Some(next);
            if grapheme.chars().all(char::is_whitespace) {
                if saw_non_whitespace {
                    whitespace_break = Some(next);
                }
            } else {
                saw_non_whitespace = true;
            }
        }

        if !overflow {
            chunks.push(line[start..].to_owned());
            break;
        }

        let split = if prefer_width_split {
            last_fitting_split.expect("overflow requires a fitting split")
        } else {
            whitespace_break
                .filter(|split| *split > start)
                .unwrap_or_else(|| last_fitting_split.expect("overflow requires a fitting split"))
        };
        chunks.push(line[start..split].to_owned());
        start = split;
    }
    chunks
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
        "element_update" | "deck_advanced" => ("Update", "Updating", "Updated"),
        "deck_inspect" => ("Inspect", "Inspecting", "Inspected"),
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
