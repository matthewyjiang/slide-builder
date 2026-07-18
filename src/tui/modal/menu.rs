//! Reusable grouped popup-menu state and renderer.
//!
//! Menus are data: callers provide groups of typed items, while this module
//! owns navigation, text editing, scrolling-by-selection, and responsive
//! presentation. This keeps configuration-specific logic out of the widget.

use crate::tui::theme;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuValue {
    Text(String),
    Choice {
        options: Vec<String>,
        selected: usize,
    },
    Toggle(bool),
}

impl MenuValue {
    pub fn display(&self) -> String {
        match self {
            Self::Text(value) => value.clone(),
            Self::Choice { options, selected } => {
                options.get(*selected).cloned().unwrap_or_default()
            }
            Self::Toggle(value) => if *value { "on" } else { "off" }.into(),
        }
    }

    pub fn visual_display(&self) -> String {
        match self {
            Self::Text(value) => value.clone(),
            Self::Choice { .. } => format!("‹ {} ›", self.display()),
            Self::Toggle(true) => "● on".into(),
            Self::Toggle(false) => "○ off".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub id: String,
    pub label: String,
    pub help: String,
    pub value: MenuValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuGroup {
    pub title: String,
    pub items: Vec<MenuItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuState {
    pub title: String,
    pub groups: Vec<MenuGroup>,
    pub selected: usize,
    pub editing: bool,
    pub status: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEvent {
    None,
    Changed,
    Save,
    Cancel,
}

impl MenuState {
    pub fn item_count(&self) -> usize {
        self.groups.iter().map(|group| group.items.len()).sum()
    }

    pub fn selected_item(&self) -> Option<&MenuItem> {
        self.groups.iter().flat_map(|g| &g.items).nth(self.selected)
    }

    pub fn selected_item_mut(&mut self) -> Option<&mut MenuItem> {
        self.groups
            .iter_mut()
            .flat_map(|g| &mut g.items)
            .nth(self.selected)
    }

    pub fn item(&self, id: &str) -> Option<&MenuItem> {
        self.groups
            .iter()
            .flat_map(|g| &g.items)
            .find(|item| item.id == id)
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> MenuEvent {
        self.status = None;
        if key.code == KeyCode::Char('s')
            && key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
        {
            self.editing = false;
            return MenuEvent::Save;
        }
        if self.editing {
            let Some(item) = self.selected_item_mut() else {
                return MenuEvent::None;
            };
            let MenuValue::Text(value) = &mut item.value else {
                self.editing = false;
                return MenuEvent::None;
            };
            return match key.code {
                KeyCode::Enter => {
                    self.editing = false;
                    MenuEvent::Changed
                }
                KeyCode::Esc => {
                    self.editing = false;
                    MenuEvent::None
                }
                KeyCode::Backspace => {
                    value.pop();
                    MenuEvent::Changed
                }
                KeyCode::Char(c) if !c.is_control() => {
                    value.push(c);
                    MenuEvent::Changed
                }
                _ => MenuEvent::None,
            };
        }

        match key.code {
            KeyCode::Esc => MenuEvent::Cancel,
            KeyCode::Char('s')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                MenuEvent::Save
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                MenuEvent::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.item_count().saturating_sub(1));
                MenuEvent::None
            }
            KeyCode::Home => {
                self.selected = 0;
                MenuEvent::None
            }
            KeyCode::End => {
                self.selected = self.item_count().saturating_sub(1);
                MenuEvent::None
            }
            KeyCode::Left => self.adjust(-1),
            KeyCode::Right | KeyCode::Char(' ') => self.adjust(1),
            KeyCode::Enter => match self.selected_item().map(|item| &item.value) {
                Some(MenuValue::Text(_)) => {
                    self.editing = true;
                    MenuEvent::None
                }
                Some(_) => self.adjust(1),
                None => MenuEvent::None,
            },
            _ => MenuEvent::None,
        }
    }

    fn adjust(&mut self, delta: isize) -> MenuEvent {
        let Some(item) = self.selected_item_mut() else {
            return MenuEvent::None;
        };
        match &mut item.value {
            MenuValue::Toggle(value) => *value = !*value,
            MenuValue::Choice { options, selected } if !options.is_empty() => {
                *selected =
                    ((*selected as isize + delta).rem_euclid(options.len() as isize)) as usize;
            }
            _ => return MenuEvent::None,
        }
        MenuEvent::Changed
    }
}

/// A centered popup that uses most of small terminals and adds margins on large ones.
pub fn responsive_popup(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).clamp(1, 100);
    let height = area.height.saturating_sub(2).clamp(1, 36);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

pub fn render_menu(frame: &mut Frame<'_>, menu: &MenuState) {
    let rect = responsive_popup(frame.area());
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .title(format!(" {} ", menu.title))
        .title_bottom(
            Line::from(" ↑↓ navigate · ←→ change · Enter edit · Ctrl+S save · Esc close ")
                .right_aligned(),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(2.min(inner.height))])
        .split(inner);
    let mut all = Vec::new();
    let mut flat_index = 0usize;
    let mut selected_line = 0usize;
    for group in &menu.groups {
        all.push(Line::styled(
            format!(" {}", group.title),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ));
        for item in &group.items {
            if flat_index == menu.selected {
                selected_line = all.len();
            }
            let selected = flat_index == menu.selected;
            let marker = if selected { "›" } else { " " };
            let edit = if selected && menu.editing { "▏" } else { "" };
            all.push(Line::from(vec![
                Span::styled(
                    format!("{marker} {:<22}", item.label),
                    if selected {
                        Style::default().fg(theme::ACCENT)
                    } else {
                        Style::default().fg(theme::TEXT)
                    },
                ),
                Span::styled(
                    format!("{}{}", item.value.visual_display(), edit),
                    if selected {
                        theme::accent_block().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme::MUTED)
                    },
                ),
            ]));
            flat_index += 1;
        }
        all.push(Line::raw(""));
    }
    let visible = rows[0].height as usize;
    let start = selected_line
        .saturating_sub(visible.saturating_sub(1) / 2)
        .min(all.len().saturating_sub(visible));
    frame.render_widget(Paragraph::new(all).scroll((start as u16, 0)), rows[0]);

    let help = menu
        .status
        .as_deref()
        .or_else(|| menu.selected_item().map(|i| i.help.as_str()))
        .unwrap_or("");
    frame.render_widget(
        Paragraph::new(help).style(Style::default().fg(if menu.status.is_some() {
            theme::DANGER
        } else {
            theme::MUTED
        })),
        rows[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn menu() -> MenuState {
        MenuState {
            title: "Test".into(),
            groups: vec![MenuGroup {
                title: "Group".into(),
                items: vec![
                    MenuItem {
                        id: "name".into(),
                        label: "Name".into(),
                        help: String::new(),
                        value: MenuValue::Text("abc".into()),
                    },
                    MenuItem {
                        id: "enabled".into(),
                        label: "Enabled".into(),
                        help: String::new(),
                        value: MenuValue::Toggle(false),
                    },
                ],
            }],
            selected: 0,
            editing: false,
            status: None,
        }
    }

    #[test]
    fn menu_navigation_and_typed_values_are_generic() {
        let mut menu = menu();
        menu.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        menu.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(menu.item("enabled").unwrap().value, MenuValue::Toggle(true));
    }

    #[test]
    fn save_shortcut_works_during_text_editing() {
        let mut menu = menu();
        menu.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(menu.editing);
        assert_eq!(
            menu.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
            MenuEvent::Save
        );
        assert_eq!(menu.item("name").unwrap().value.display(), "abc");
    }

    #[test]
    fn popup_stays_inside_different_terminal_sizes() {
        for area in [Rect::new(0, 0, 20, 5), Rect::new(2, 3, 240, 80)] {
            let popup = responsive_popup(area);
            assert!(popup.x >= area.x && popup.y >= area.y);
            assert!(popup.right() <= area.right() && popup.bottom() <= area.bottom());
        }
    }
}
