//! Nested session management. Storage results return through the app event loop.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionAction {
    Resume,
    Rename,
    Delete,
}

impl SessionAction {
    const ALL: [Self; 3] = [Self::Resume, Self::Rename, Self::Delete];

    fn label(self) -> &'static str {
        match self {
            Self::Resume => "Resume",
            Self::Rename => "Rename",
            Self::Delete => "Delete",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionView {
    List,
    Actions {
        entry: SessionPickerEntry,
        selected: SessionAction,
    },
    Rename {
        entry: SessionPickerEntry,
        name: String,
    },
    ConfirmDelete {
        entry: SessionPickerEntry,
        delete_selected: bool,
    },
}

impl SessionPickerState {
    pub fn manager(entries: Vec<SessionPickerEntry>) -> Self {
        Self {
            mode: SessionPickerMode::Manage,
            ..Self::new(entries)
        }
    }

    pub fn management_succeeded(&mut self, entries: Vec<SessionPickerEntry>) {
        self.entries = entries;
        self.selected = self.selected.min(self.visible().len().saturating_sub(1));
        self.view = SessionView::List;
        self.error = None;
    }

    pub(super) fn handle_manager_key(&mut self, key: KeyEvent) -> SessionPickerEvent {
        match &mut self.view {
            SessionView::List => {}
            SessionView::Actions { entry, selected } => {
                match key.code {
                    KeyCode::Esc => {
                        self.view = SessionView::List;
                        self.error = None;
                    }
                    KeyCode::Up => {
                        *selected = match selected {
                            SessionAction::Resume | SessionAction::Rename => SessionAction::Resume,
                            SessionAction::Delete => SessionAction::Rename,
                        }
                    }
                    KeyCode::Down => {
                        *selected = match selected {
                            SessionAction::Resume => SessionAction::Rename,
                            SessionAction::Rename | SessionAction::Delete => SessionAction::Delete,
                        }
                    }
                    KeyCode::Home => *selected = SessionAction::Resume,
                    KeyCode::End => *selected = SessionAction::Delete,
                    KeyCode::Enter => {
                        self.error = None;
                        match *selected {
                            SessionAction::Resume => {
                                return SessionPickerEvent::Selected(entry.id.clone())
                            }
                            SessionAction::Rename => {
                                self.view = SessionView::Rename {
                                    name: entry.name.clone(),
                                    entry: entry.clone(),
                                }
                            }
                            SessionAction::Delete if entry.current => self.error = Some(
                                "Switch to another session before deleting the current session."
                                    .into(),
                            ),
                            SessionAction::Delete => {
                                self.view = SessionView::ConfirmDelete {
                                    entry: entry.clone(),
                                    delete_selected: false,
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            SessionView::Rename { entry, name } => match key.code {
                KeyCode::Esc => {
                    self.view = SessionView::Actions {
                        entry: entry.clone(),
                        selected: SessionAction::Rename,
                    };
                    self.error = None;
                }
                KeyCode::Enter => {
                    if name.trim().is_empty() {
                        self.error = Some("Enter a session name.".into());
                    } else {
                        return SessionPickerEvent::Rename {
                            id: entry.id.clone(),
                            name: name.trim().to_owned(),
                        };
                    }
                }
                KeyCode::Backspace => {
                    name.pop();
                    self.error = None;
                }
                KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
                    name.clear();
                    self.error = None;
                }
                KeyCode::Char(c)
                    if !c.is_control()
                        && !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    name.push(c);
                    self.error = None;
                }
                _ => {}
            },
            SessionView::ConfirmDelete {
                entry,
                delete_selected,
            } => match key.code {
                KeyCode::Esc => {
                    self.view = SessionView::Actions {
                        entry: entry.clone(),
                        selected: SessionAction::Delete,
                    };
                    self.error = None;
                }
                KeyCode::Up | KeyCode::Home => *delete_selected = false,
                KeyCode::Down | KeyCode::End => *delete_selected = true,
                KeyCode::Enter if *delete_selected => {
                    // Reset before dispatch so retrying after an error requires another deliberate choice.
                    *delete_selected = false;
                    return SessionPickerEvent::Delete(entry.id.clone());
                }
                KeyCode::Enter => {
                    self.view = SessionView::Actions {
                        entry: entry.clone(),
                        selected: SessionAction::Delete,
                    };
                    self.error = None;
                }
                _ => {}
            },
        }
        SessionPickerEvent::None
    }
}

pub(super) fn render(frame: &mut Frame<'_>, state: &SessionPickerState) {
    let area = popup(frame, 76, 20);
    let (title, entry, help) = match &state.view {
        SessionView::List => return,
        SessionView::Actions { entry, .. } => {
            (" Manage session ", entry, "↑↓ choose  Enter  Esc back")
        }
        SessionView::Rename { entry, .. } => (
            " Rename session ",
            entry,
            "Enter save  Esc back\nCtrl+U clear",
        ),
        SessionView::ConfirmDelete { entry, .. } => {
            (" Delete session? ", entry, "↑↓ choose  Enter  Esc back")
        }
    };
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = vec![Line::styled(
        detail_suffix(&entry.name, usize::from(inner.width)),
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD),
    )];
    match &state.view {
        SessionView::Actions { selected, .. } => {
            for action in SessionAction::ALL {
                lines.push(choice(action.label(), *selected == action));
            }
            if entry.current {
                lines.extend(crate::tui::conversation_entry::plain_rows(
                    "Current session: switch before deleting.",
                    usize::from(inner.width),
                    Style::default().fg(theme::MUTED),
                ));
            }
        }
        SessionView::Rename { name, .. } => {
            lines.push(Line::styled("Name:", Style::default().fg(theme::MUTED)));
            lines.push(Line::styled(
                format!(
                    "{}▏",
                    detail_suffix(name, usize::from(inner.width.saturating_sub(1)))
                ),
                theme::accent_block(),
            ));
        }
        SessionView::ConfirmDelete {
            delete_selected, ..
        } => {
            lines.extend(crate::tui::conversation_entry::plain_rows(
                "Permanently delete this conversation? Deck files are kept.",
                usize::from(inner.width),
                Style::default().fg(theme::WARNING),
            ));
            lines.push(choice("Cancel", !delete_selected));
            lines.push(choice("Delete session", *delete_selected));
        }
        SessionView::List => {}
    }
    let help_rows = crate::tui::conversation_entry::plain_rows(
        help,
        usize::from(inner.width),
        Style::default().fg(theme::MUTED),
    );
    // Controls and keyboard hints take priority over long storage errors.
    if let Some(error) = &state.error {
        let available = usize::from(inner.height)
            .saturating_sub(help_rows.len())
            .saturating_sub(lines.len());
        lines.extend(
            crate::tui::conversation_entry::plain_rows(
                &format!("Notice: {error}"),
                usize::from(inner.width),
                Style::default().fg(theme::WARNING),
            )
            .into_iter()
            .take(available),
        );
    }
    let rows = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(help_rows.len() as u16),
    ])
    .split(inner);
    frame.render_widget(Paragraph::new(lines), rows[0]);
    frame.render_widget(Paragraph::new(help_rows), rows[1]);
}

fn choice(label: &str, selected: bool) -> Line<'static> {
    Line::styled(
        format!("{} {label}", if selected { "›" } else { " " }),
        if selected {
            theme::accent_block()
        } else {
            Style::default().fg(theme::TEXT)
        },
    )
}
