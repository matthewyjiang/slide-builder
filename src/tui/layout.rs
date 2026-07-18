use super::{
    app::{App, ImportDesignStatus},
    chat,
    event::ImportDesignStage,
    modal, outline, preview, slideshow, statusline, theme,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub fn render(frame: &mut Frame<'_>, app: &App) {
    render_with_preview(frame, app, None);
}

pub fn render_with_preview(
    frame: &mut Frame<'_>,
    app: &App,
    preview_image: Option<&mut super::preview_image::PreviewImage>,
) {
    let area = frame.area();
    if app.fullscreen {
        slideshow::render(frame, area, app, preview_image);
        return;
    }

    let regions = regions(area, app);
    statusline::render_header(frame, regions.header, app);
    chat::render(frame, regions.chat, app);
    preview::render(frame, regions.preview, app, preview_image);
    outline::render(frame, regions.outline, app);
    if regions.compact {
        render_horizontal_separator(frame, regions.chat_separator);
        render_horizontal_separator(frame, regions.preview_separator);
    } else {
        render_vertical_separator(frame, regions.chat_separator);
        render_horizontal_separator(frame, regions.preview_separator);
    }
    render_input(frame, regions.input, app);
    render_slash_commands(frame, regions.input, app);
    statusline::render_actions(frame, regions.actions, app);
    super::mouse::render_feedback(frame, app);
    modal::render(frame, &app.modal);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UiRegions {
    pub header: Rect,
    pub chat: Rect,
    pub chat_separator: Rect,
    pub preview: Rect,
    pub preview_separator: Rect,
    pub outline: Rect,
    pub input: Rect,
    pub actions: Rect,
    pub compact: bool,
}

pub(crate) fn regions(area: Rect, app: &App) -> UiRegions {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(input_height(app, area)),
            Constraint::Length(1),
        ])
        .split(area);
    let workspace = rows[1];
    if workspace.width < 88 {
        let panels = Layout::vertical([
            Constraint::Percentage(50),
            Constraint::Length(1),
            Constraint::Percentage(30),
            Constraint::Length(1),
            Constraint::Percentage(20),
        ])
        .split(workspace);
        return UiRegions {
            header: rows[0],
            chat: panels[0],
            chat_separator: panels[1],
            preview: panels[2],
            preview_separator: panels[3],
            outline: panels[4],
            input: rows[2],
            actions: rows[3],
            compact: true,
        };
    }

    let body = Layout::horizontal([
        Constraint::Percentage(56),
        Constraint::Length(1),
        Constraint::Percentage(44),
    ])
    .split(workspace);
    let right = Layout::vertical([
        Constraint::Percentage(68),
        Constraint::Length(1),
        Constraint::Percentage(32),
    ])
    .split(body[2]);
    UiRegions {
        header: rows[0],
        chat: body[0],
        chat_separator: body[1],
        preview: right[0],
        preview_separator: right[1],
        outline: right[2],
        input: rows[2],
        actions: rows[3],
        compact: false,
    }
}

fn render_vertical_separator(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(theme::SUBTLE)),
        area,
    );
}

fn render_horizontal_separator(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(theme::SUBTLE)),
        area,
    );
}

fn input_height(app: &App, area: Rect) -> u16 {
    visual_line_count(&app.input.text, area.width)
        .saturating_add(2)
        .clamp(3, area.height.saturating_div(3).max(3))
}

fn visual_line_count(text: &str, width: u16) -> u16 {
    let width = width.max(1) as usize;
    text.split('\n')
        .map(|line| Line::from(line).width().max(1).div_ceil(width) as u16)
        .sum()
}

fn input_cursor_position(text: &str, width: u16) -> (u16, u16) {
    let width = width.max(1) as usize;
    let mut lines = text.split('\n').peekable();
    let mut row = 0usize;
    let mut col = 0usize;
    while let Some(line) = lines.next() {
        let line_width = Line::from(line).width();
        if lines.peek().is_some() {
            row += line_width.max(1).div_ceil(width);
        } else {
            row += line_width / width;
            col = line_width % width;
        }
    }
    (row as u16, col as u16)
}

fn render_input(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let attachment = if app.input.attach_active_slide {
        Span::styled(
            "  ● active slide attached ",
            Style::default().fg(theme::WARNING),
        )
    } else {
        Span::raw("")
    };
    let mut title_spans = vec![Span::styled(" Prompt ", theme::panel_title()), attachment];
    title_spans.extend(import_status_spans(app));
    let title = Line::from(title_spans);
    let display = if app.input.text.is_empty() {
        Text::from(Line::styled(
            "Describe what to create, revise, or explore...",
            Style::default().fg(theme::MUTED),
        ))
    } else {
        Text::from(app.input.text.as_str())
    };
    frame.render_widget(
        Paragraph::new(display).wrap(Wrap { trim: false }).block(
            Block::default()
                .title(title)
                .borders(Borders::TOP | Borders::BOTTOM)
                .border_style(Style::default().fg(theme::SUBTLE)),
        ),
        area,
    );

    if !app.run_active && area.width > 0 && area.height > 1 {
        let before = &app.input.text[..app.input.cursor];
        let (row, col) = input_cursor_position(before, area.width);
        frame.set_cursor_position((
            area.x + col.min(area.width - 1),
            area.y + 1 + row.min(area.height - 2),
        ));
    }
}

fn import_status_spans(app: &App) -> Vec<Span<'static>> {
    const INDICATORS: [&str; 6] = [
        "[━─────]",
        "[─━────]",
        "[──━───]",
        "[───━──]",
        "[────━─]",
        "[─────━]",
    ];

    let Some(status) = &app.import_design_status else {
        return vec![];
    };
    let (text, style) = match status {
        ImportDesignStatus::Running(progress) => {
            let stage = match progress.stage {
                ImportDesignStage::Reading => "Reading",
                ImportDesignStage::Analyzing => "Analyzing",
                ImportDesignStage::Building => "Building",
                ImportDesignStage::Installing => "Installing",
            };
            let indicator = match progress.percent {
                Some(percent) => format!("[{percent:>3}%]"),
                None => INDICATORS[progress.animation_frame % INDICATORS.len()].into(),
            };
            (
                format!("  {indicator} {stage} {} ", progress.source_name),
                Style::default().fg(theme::ACCENT),
            )
        }
        ImportDesignStatus::Completed { design_name, .. } => (
            format!("  ✓ Imported {design_name} "),
            Style::default().fg(theme::SUCCESS),
        ),
        ImportDesignStatus::Failed { error } => (
            format!("  ✗ Import failed: {error} · retry /import-design "),
            Style::default().fg(theme::DANGER),
        ),
        ImportDesignStatus::Cancelled { .. } => (
            "  Import cancelled ".into(),
            Style::default().fg(theme::MUTED),
        ),
    };
    vec![Span::styled(text, style)]
}

fn render_slash_commands(frame: &mut Frame<'_>, input_area: Rect, app: &App) {
    if app.run_active || !matches!(app.modal, modal::ModalState::None) {
        return;
    }
    let suggestions = app.input.slash_suggestions();
    if suggestions.is_empty() || input_area.y < 3 {
        return;
    }

    let visible_rows = suggestions
        .len()
        .min(6)
        .min(input_area.y.saturating_sub(2) as usize);
    if visible_rows == 0 {
        return;
    }
    let selected = app.input.slash_selection.min(suggestions.len() - 1);
    let start = selected
        .saturating_add(1)
        .saturating_sub(visible_rows)
        .min(suggestions.len() - visible_rows);
    let width = input_area.width.clamp(1, 72);
    let height = visible_rows as u16 + 2;
    let area = Rect::new(input_area.x, input_area.y - height, width, height);
    let items = suggestions[start..start + visible_rows]
        .iter()
        .enumerate()
        .map(|(offset, suggestion)| {
            let is_selected = start + offset == selected;
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:<12}", suggestion.name),
                    Style::default()
                        .fg(theme::TEXT)
                        .add_modifier(if is_selected {
                            ratatui::style::Modifier::BOLD
                        } else {
                            ratatui::style::Modifier::empty()
                        }),
                ),
                Span::styled(suggestion.detail, Style::default().fg(theme::MUTED)),
            ]))
            .style(if is_selected {
                theme::accent_block()
            } else {
                Style::default()
            })
        });

    frame.render_widget(Clear, area);
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .title(" Slash commands ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::ACCENT)),
        ),
        area,
    );
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
