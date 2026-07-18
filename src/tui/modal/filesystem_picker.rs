use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::{paths::expand_tilde, tui::theme};

use super::popup;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSystemEntry {
    pub path: PathBuf,
    pub is_directory: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSystemPickerState {
    pub current_directory: PathBuf,
    pub entries: Vec<FileSystemEntry>,
    pub selected: usize,
    pub path_input: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileSystemPickerEvent {
    None,
    Cancel,
    Selected(PathBuf),
}

impl FileSystemPickerState {
    pub fn new(start_directory: PathBuf) -> Self {
        let mut state = Self {
            current_directory: start_directory,
            entries: Vec::new(),
            selected: 0,
            path_input: String::new(),
            error: None,
        };
        state.open_directory(state.current_directory.clone());
        state
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> FileSystemPickerEvent {
        match key.code {
            KeyCode::Esc => FileSystemPickerEvent::Cancel,
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                FileSystemPickerEvent::None
            }
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.entries.len().saturating_sub(1));
                FileSystemPickerEvent::None
            }
            KeyCode::Home => {
                self.selected = 0;
                FileSystemPickerEvent::None
            }
            KeyCode::End => {
                self.selected = self.entries.len().saturating_sub(1);
                FileSystemPickerEvent::None
            }
            KeyCode::Backspace => {
                self.path_input.pop();
                self.error = None;
                FileSystemPickerEvent::None
            }
            KeyCode::Char(_)
                if key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                FileSystemPickerEvent::None
            }
            KeyCode::Char(character) => {
                self.path_input.push(character);
                self.error = None;
                FileSystemPickerEvent::None
            }
            KeyCode::Enter if !self.path_input.trim().is_empty() => {
                let input = self.path_input.trim();
                let path = PathBuf::from(input);
                let path = match expand_tilde(&path) {
                    Ok(path) if path.is_absolute() => path,
                    Ok(path) => self.current_directory.join(path),
                    Err(error) => {
                        self.error = Some(error.to_string());
                        return FileSystemPickerEvent::None;
                    }
                };
                self.activate(path)
            }
            KeyCode::Enter => {
                let Some(entry) = self.entries.get(self.selected) else {
                    return FileSystemPickerEvent::None;
                };
                self.activate(entry.path.clone())
            }
            _ => FileSystemPickerEvent::None,
        }
    }

    pub fn paste(&mut self, text: &str) {
        self.path_input.push_str(text.trim());
        self.error = None;
    }

    fn activate(&mut self, path: PathBuf) -> FileSystemPickerEvent {
        if path.is_dir() {
            self.open_directory(path);
            return FileSystemPickerEvent::None;
        }
        if path.is_file() && is_powerpoint(&path) {
            return FileSystemPickerEvent::Selected(path);
        }
        self.error = Some("Choose a directory or a .pptx file".into());
        FileSystemPickerEvent::None
    }

    fn open_directory(&mut self, path: PathBuf) {
        match read_entries(&path) {
            Ok(entries) => {
                self.current_directory = path;
                self.entries = entries;
                self.selected = 0;
                self.path_input.clear();
                self.error = None;
            }
            Err(error) => {
                self.error = Some(format!("Cannot open {}: {error}", path.display()));
            }
        }
    }
}

fn read_entries(directory: &Path) -> std::io::Result<Vec<FileSystemEntry>> {
    let mut entries = Vec::new();
    if let Some(parent) = directory.parent() {
        entries.push(FileSystemEntry {
            path: parent.to_path_buf(),
            is_directory: true,
        });
    }

    let mut children = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let is_directory = path.is_dir();
            (is_directory || is_powerpoint(&path)).then_some(FileSystemEntry { path, is_directory })
        })
        .collect::<Vec<_>>();
    children.sort_by(|left, right| {
        right.is_directory.cmp(&left.is_directory).then_with(|| {
            file_name(&left.path)
                .to_lowercase()
                .cmp(&file_name(&right.path).to_lowercase())
        })
    });
    entries.extend(children);
    Ok(entries)
}

fn is_powerpoint(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pptx"))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn render(frame: &mut Frame<'_>, state: &FileSystemPickerState) {
    let area = popup(frame, 82, 24);
    let block = Block::default()
        .title(" Import design from PowerPoint ")
        .title_style(
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .title_bottom(Line::from(" ↑↓ choose  Enter open/select  Esc cancel ").right_aligned())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(state.current_directory.display().to_string())
            .style(Style::default().fg(theme::MUTED)),
        rows[0],
    );

    let visible_height = rows[1].height as usize;
    let start = state
        .selected
        .saturating_sub(visible_height.saturating_sub(1));
    let items = state
        .entries
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_height)
        .map(|(index, entry)| {
            let name = if entry.path == state.current_directory.parent().unwrap_or(Path::new("")) {
                "../".into()
            } else if entry.is_directory {
                format!("{}/", file_name(&entry.path))
            } else {
                file_name(&entry.path)
            };
            ListItem::new(Span::raw(format!("  {name}"))).style(if index == state.selected {
                Style::default().bg(theme::ACCENT_SOFT).fg(theme::TEXT)
            } else {
                Style::default().fg(theme::TEXT)
            })
        });
    frame.render_widget(List::new(items).highlight_symbol("›"), rows[1]);

    let input = if state.path_input.is_empty() {
        "Type or paste a path...".to_owned()
    } else {
        state.path_input.clone()
    };
    frame.render_widget(
        Paragraph::new(input)
            .style(Style::default().fg(if state.path_input.is_empty() {
                theme::MUTED
            } else {
                theme::TEXT
            }))
            .block(Block::default().borders(Borders::ALL).title(" Path ")),
        rows[2],
    );
    if let Some(error) = &state.error {
        frame.render_widget(
            Paragraph::new(error.as_str()).style(Style::default().fg(theme::DANGER)),
            rows[3],
        );
    }
}

#[cfg(test)]
#[path = "filesystem_picker_tests.rs"]
mod tests;
