use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::tui::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    OpenDeck,
    ChangeDesign,
    ImportDesign,
    RenderPreview,
    Configure,
    ToggleAttachment,
    Present,
    ShowHelp,
    Quit,
}

#[derive(Clone, Copy)]
struct CommandItem {
    command: Command,
    slash_name: &'static str,
    label: &'static str,
    detail: &'static str,
    shortcut: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlashCommandAction {
    OpenPalette,
    Run(Command),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlashCommand {
    pub action: SlashCommandAction,
    pub name: &'static str,
    pub detail: &'static str,
}

const COMMANDS: [CommandItem; 9] = [
    CommandItem {
        command: Command::OpenDeck,
        slash_name: "/open",
        label: "Open deck",
        detail: "Choose a different PowerPoint file",
        shortcut: "Ctrl+O",
    },
    CommandItem {
        command: Command::ChangeDesign,
        slash_name: "/design",
        label: "Change design",
        detail: "Choose the deck's visual system",
        shortcut: "Ctrl+P",
    },
    CommandItem {
        command: Command::ImportDesign,
        slash_name: "/import-design",
        label: "Import design",
        detail: "Create a design package from a PowerPoint file",
        shortcut: "",
    },
    CommandItem {
        command: Command::RenderPreview,
        slash_name: "/render",
        label: "Refresh preview",
        detail: "Render the latest deck changes",
        shortcut: "Ctrl+R",
    },
    CommandItem {
        command: Command::Configure,
        slash_name: "/config",
        label: "Settings",
        detail: "Provider, permissions, preview, and renderer",
        shortcut: "Ctrl+,",
    },
    CommandItem {
        command: Command::ToggleAttachment,
        slash_name: "/attach",
        label: "Attach active slide",
        detail: "Include the current slide with your next prompt",
        shortcut: "Ctrl+V",
    },
    CommandItem {
        command: Command::Present,
        slash_name: "/present",
        label: "Present active slide",
        detail: "Open a distraction-free slide view",
        shortcut: "Enter",
    },
    CommandItem {
        command: Command::ShowHelp,
        slash_name: "/help",
        label: "Keyboard help",
        detail: "See navigation and editing controls",
        shortcut: "F1",
    },
    CommandItem {
        command: Command::Quit,
        slash_name: "/quit",
        label: "Quit slide-builder",
        detail: "Close the current workspace",
        shortcut: "Ctrl+C",
    },
];

pub fn matching_slash_commands(input: &str) -> Vec<SlashCommand> {
    let query = input.to_ascii_lowercase();
    let mut matches = Vec::new();
    if "/actions".starts_with(&query) {
        matches.push(SlashCommand {
            action: SlashCommandAction::OpenPalette,
            name: "/actions",
            detail: "Browse every available action",
        });
    }
    matches.extend(
        COMMANDS
            .iter()
            .filter(|item| item.slash_name.starts_with(&query))
            .map(|item| SlashCommand {
                action: SlashCommandAction::Run(item.command),
                name: item.slash_name,
                detail: item.detail,
            }),
    );
    matches
}

pub fn exact_slash_command(input: &str) -> Option<SlashCommandAction> {
    if input.eq_ignore_ascii_case("/actions") || input.eq_ignore_ascii_case("/menu") {
        return Some(SlashCommandAction::OpenPalette);
    }
    COMMANDS
        .iter()
        .find(|item| item.slash_name.eq_ignore_ascii_case(input))
        .map(|item| SlashCommandAction::Run(item.command))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandPaletteState {
    pub selected: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandPaletteEvent {
    None,
    Cancel,
    Run(Command),
}

impl CommandPaletteState {
    pub fn handle_key(&mut self, key: KeyEvent) -> CommandPaletteEvent {
        match key.code {
            KeyCode::Esc => CommandPaletteEvent::Cancel,
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                CommandPaletteEvent::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(COMMANDS.len() - 1);
                CommandPaletteEvent::None
            }
            KeyCode::Home => {
                self.selected = 0;
                CommandPaletteEvent::None
            }
            KeyCode::End => {
                self.selected = COMMANDS.len() - 1;
                CommandPaletteEvent::None
            }
            KeyCode::Enter => CommandPaletteEvent::Run(COMMANDS[self.selected].command),
            _ => CommandPaletteEvent::None,
        }
    }
}

pub fn render(frame: &mut Frame<'_>, state: &CommandPaletteState) {
    let area = centered(frame.area(), 76, 22);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(" Actions ")
        .title_style(
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .title_bottom(Line::from(" ↑↓ choose  Enter run  Esc close ").right_aligned())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(inner);
    frame.render_widget(
        Paragraph::new("Everything you can do, in one place.")
            .style(Style::default().fg(theme::MUTED)),
        rows[0],
    );
    let items = COMMANDS.iter().enumerate().map(|(index, item)| {
        let selected = index == state.selected;
        ListItem::new(Line::from(vec![
            Span::styled(
                format!("  {:<20}", item.label),
                if selected {
                    Style::default()
                        .fg(theme::TEXT)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::TEXT)
                },
            ),
            Span::styled(
                format!("{:<34}", item.detail),
                Style::default().fg(theme::MUTED),
            ),
            Span::styled(format!(" {:>8} ", item.shortcut), theme::keycap()),
        ]))
        .style(if selected {
            theme::accent_block()
        } else {
            Style::default()
        })
    });
    frame.render_widget(List::new(items).highlight_symbol("›"), rows[1]);
}

fn centered(area: Rect, max_width: u16, max_height: u16) -> Rect {
    let width = area.width.saturating_sub(2).min(max_width).max(1);
    let height = area.height.saturating_sub(2).min(max_height).max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    #[test]
    fn palette_navigation_runs_selected_command() {
        let mut state = CommandPaletteState::default();
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            CommandPaletteEvent::Run(Command::ChangeDesign)
        );
    }

    #[test]
    fn import_design_is_available_as_an_exact_slash_command() {
        assert_eq!(
            exact_slash_command("/import-design"),
            Some(SlashCommandAction::Run(Command::ImportDesign))
        );
    }
}
