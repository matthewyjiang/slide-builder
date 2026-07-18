use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::{app::App, chat, event::AppAction, layout, outline, theme};

const TOAST_DURATION: Duration = Duration::from_secs(2);
const WHEEL_SCROLL_LINES: u16 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScreenPoint {
    pub x: u16,
    pub y: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: ScreenPoint,
    pub cursor: ScreenPoint,
    pub dragged: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyToast {
    pub message: String,
    pub expires_at: Instant,
    pub location: ScreenPoint,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MouseState {
    pub viewport: Rect,
    pub selection: Option<TextSelection>,
    pub hovered_slide: Option<usize>,
    pub toast: Option<CopyToast>,
}

impl MouseState {
    pub fn expire_toast(&mut self, now: Instant) {
        if self
            .toast
            .as_ref()
            .is_some_and(|toast| now >= toast.expires_at)
        {
            self.toast = None;
        }
    }
}

pub(crate) fn handle(app: &mut App, event: MouseEvent) -> Vec<AppAction> {
    if app.fullscreen || !matches!(app.modal, super::modal::ModalState::None) {
        return vec![];
    }
    if app.mouse.viewport.width == 0 || app.mouse.viewport.height == 0 {
        return vec![];
    }
    let point = ScreenPoint {
        x: event.column,
        y: event.row,
    };
    let regions = layout::regions(app.mouse.viewport, app);
    let chat_body = Rect::new(
        regions.chat.x,
        regions.chat.y.saturating_add(1),
        regions.chat.width,
        regions.chat.height.saturating_sub(1),
    );

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            app.mouse.toast = None;
            if contains(chat_body, point) {
                app.mouse.selection = Some(TextSelection {
                    anchor: point,
                    cursor: point,
                    dragged: false,
                });
                return vec![];
            }
            app.mouse.selection = None;
            if let Some(index) = slide_at(app, regions.outline, point) {
                if app.preview.select(index) {
                    return vec![AppAction::SetActiveSlide(index)];
                }
            }
            vec![]
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(selection) = &mut app.mouse.selection {
                selection.cursor = clamp_to(chat_body, point);
                selection.dragged |= selection.cursor != selection.anchor;
            }
            vec![]
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let Some(mut selection) = app.mouse.selection else {
                return vec![];
            };
            selection.cursor = clamp_to(chat_body, point);
            selection.dragged |= selection.cursor != selection.anchor;
            app.mouse.selection = Some(selection);
            if !selection.dragged {
                return vec![];
            }
            let text = selected_text(app, chat_body, selection);
            if text.is_empty() {
                return vec![];
            }
            let count = text.chars().count();
            app.mouse.toast = Some(CopyToast {
                message: format!("Copied {count} chars"),
                expires_at: Instant::now() + TOAST_DURATION,
                location: point,
            });
            vec![AppAction::CopyText(text)]
        }
        MouseEventKind::ScrollUp if contains(regions.chat, point) => {
            app.mouse.selection = None;
            chat::scroll_up(app, chat_body, WHEEL_SCROLL_LINES);
            vec![]
        }
        MouseEventKind::ScrollDown if contains(regions.chat, point) => {
            app.mouse.selection = None;
            chat::scroll_down(app, WHEEL_SCROLL_LINES);
            vec![]
        }
        MouseEventKind::Moved => {
            app.mouse.hovered_slide = slide_at(app, regions.outline, point);
            vec![]
        }
        _ => vec![],
    }
}

pub(crate) fn render_feedback(frame: &mut Frame<'_>, app: &App) {
    if let Some(selection) = app.mouse.selection {
        render_selection(frame, app, selection);
    }
    let Some(toast) = &app.mouse.toast else {
        return;
    };
    let viewport = frame.area();
    let width = (toast.message.chars().count() as u16 + 4).min(viewport.width);
    let height = 3.min(viewport.height);
    if width == 0 || height == 0 {
        return;
    }
    let max_x = viewport.right().saturating_sub(width);
    let max_y = viewport.bottom().saturating_sub(height);
    let area = Rect::new(
        toast.location.x.saturating_add(1).min(max_x),
        toast.location.y.saturating_sub(height).min(max_y),
        width,
        height,
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(Line::styled(
            toast.message.as_str(),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::SUCCESS)),
        ),
        area,
    );
}

fn render_selection(frame: &mut Frame<'_>, app: &App, selection: TextSelection) {
    let regions = layout::regions(frame.area(), app);
    let body = Rect::new(
        regions.chat.x,
        regions.chat.y.saturating_add(1),
        regions.chat.width,
        regions.chat.height.saturating_sub(1),
    );
    let (start, end) = ordered(selection.anchor, selection.cursor);
    let rows = chat::visible_text_rows(regions.chat, app);
    for y in start.y..=end.y {
        if y < body.y || y >= body.bottom() {
            continue;
        }
        let row = &rows[(y - body.y) as usize];
        let row_width = row.chars().count() as u16;
        let start_x = if y == start.y { start.x } else { body.x };
        let end_x = if y == end.y {
            end.x
        } else {
            body.x.saturating_add(row_width).saturating_sub(1)
        };
        let start_x = start_x.max(body.x);
        let end_x = end_x.min(body.x.saturating_add(row_width).saturating_sub(1));
        if start_x <= end_x && row_width > 0 {
            frame.buffer_mut().set_style(
                Rect::new(start_x, y, end_x - start_x + 1, 1),
                Style::default().fg(theme::TEXT).bg(theme::ACCENT_SOFT),
            );
        }
    }
}

fn selected_text(app: &App, body: Rect, selection: TextSelection) -> String {
    let regions = layout::regions(app.mouse.viewport, app);
    let rows = chat::visible_text_rows(regions.chat, app);
    let (start, end) = ordered(selection.anchor, selection.cursor);
    let mut selected = Vec::new();
    for y in start.y..=end.y {
        if y < body.y || y >= body.bottom() {
            continue;
        }
        let row = &rows[(y - body.y) as usize];
        let from = if y == start.y {
            start.x.saturating_sub(body.x) as usize
        } else {
            0
        };
        let through = if y == end.y {
            end.x.saturating_sub(body.x) as usize + 1
        } else {
            row.chars().count()
        };
        selected.push(
            row.chars()
                .skip(from)
                .take(through.saturating_sub(from))
                .collect::<String>(),
        );
    }
    selected.join("\n")
}

fn slide_at(app: &App, area: Rect, point: ScreenPoint) -> Option<usize> {
    let body = Rect::new(
        area.x,
        area.y.saturating_add(1),
        area.width,
        area.height.saturating_sub(1),
    );
    if !contains(body, point) {
        return None;
    }
    let start = outline::visible_start(app, body.height as usize);
    let index = start + point.y.saturating_sub(body.y) as usize;
    (index < app.preview.slides.len()).then_some(index)
}

fn ordered(a: ScreenPoint, b: ScreenPoint) -> (ScreenPoint, ScreenPoint) {
    if (a.y, a.x) <= (b.y, b.x) {
        (a, b)
    } else {
        (b, a)
    }
}

fn contains(area: Rect, point: ScreenPoint) -> bool {
    point.x >= area.x && point.x < area.right() && point.y >= area.y && point.y < area.bottom()
}

fn clamp_to(area: Rect, point: ScreenPoint) -> ScreenPoint {
    ScreenPoint {
        x: point.x.clamp(area.x, area.right().saturating_sub(1)),
        y: point.y.clamp(area.y, area.bottom().saturating_sub(1)),
    }
}

#[cfg(test)]
#[path = "mouse_tests.rs"]
mod tests;
