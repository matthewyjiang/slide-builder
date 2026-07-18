use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::event::{AgentEvent, AppAction, AppEvent, ApprovalDecision, RenderManifest};
use super::modal::{
    exact_slash_command, matching_slash_commands, Command, CommandPaletteEvent,
    CommandPaletteState, ConfigurationEvent, ConfigurationState, ModalState, SlashCommand,
    SlashCommandAction,
};
use crate::config::Config;

/// How long slide-prefix mode stays armed without a follow-up key.
const PREFIX_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolStatus {
    Proposed,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub text: String,
    pub complete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCard {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub detail: String,
    pub status: ToolStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptItem {
    Message(Message),
    Tool(ToolCard),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlideItem {
    pub title: String,
    pub image_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewStatus {
    Empty,
    Rendering { generation: u64 },
    Ready { generation: u64 },
    Stale { generation: u64 },
    Failed { generation: u64, error: String },
    Unavailable { reason: String },
}

impl Default for PreviewStatus {
    fn default() -> Self {
        Self::Empty
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreviewState {
    pub status: PreviewStatus,
    pub slides: Vec<SlideItem>,
    pub active: usize,
}

impl PreviewState {
    pub fn slide_count(&self) -> usize {
        self.slides.len()
    }
    pub fn generation(&self) -> u64 {
        match &self.status {
            PreviewStatus::Rendering { generation }
            | PreviewStatus::Ready { generation }
            | PreviewStatus::Stale { generation }
            | PreviewStatus::Failed { generation, .. } => *generation,
            PreviewStatus::Empty | PreviewStatus::Unavailable { .. } => 0,
        }
    }
    pub fn active_image_path(&self) -> Option<&std::path::Path> {
        self.slides
            .get(self.active)
            .and_then(|slide| slide.image_path.as_deref())
    }
    pub fn select(&mut self, index: usize) -> bool {
        if self.slides.is_empty() {
            self.active = 0;
            return false;
        }
        let next = index.min(self.slides.len() - 1);
        let changed = next != self.active;
        self.active = next;
        changed
    }
    pub fn next(&mut self) -> bool {
        self.select(self.active.saturating_add(1))
    }
    pub fn previous(&mut self) -> bool {
        self.select(self.active.saturating_sub(1))
    }
    pub fn first(&mut self) -> bool {
        self.select(0)
    }
    pub fn last(&mut self) -> bool {
        self.select(self.slides.len().saturating_sub(1))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputState {
    pub text: String,
    /// Byte offset, always kept on a UTF-8 boundary.
    pub cursor: usize,
    pub attach_active_slide: bool,
    pub slash_selection: usize,
    pub slash_menu_hidden: bool,
}

impl InputState {
    pub fn slash_suggestions(&self) -> Vec<SlashCommand> {
        if self.slash_menu_hidden
            || self.cursor != self.text.len()
            || !self.text.starts_with('/')
            || self.text.chars().any(char::is_whitespace)
        {
            return vec![];
        }
        matching_slash_commands(&self.text)
    }

    fn reset_slash_menu(&mut self) {
        self.slash_selection = 0;
        self.slash_menu_hidden = false;
    }

    fn set_text(&mut self, text: &str) {
        self.text.clear();
        self.text.push_str(text);
        self.cursor = self.text.len();
        self.reset_slash_menu();
    }

    fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.reset_slash_menu();
    }

    pub fn insert(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.reset_slash_menu();
    }
    pub fn newline(&mut self) {
        self.insert('\n');
    }
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let previous = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.text.replace_range(previous..self.cursor, "");
        self.cursor = previous;
        self.reset_slash_menu();
    }
    pub fn delete(&mut self) {
        if self.cursor == self.text.len() {
            return;
        }
        let next = self.text[self.cursor..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(0)
            + self.cursor;
        self.text.replace_range(self.cursor..next, "");
        self.reset_slash_menu();
    }
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }
    pub fn move_right(&mut self) {
        if let Some(c) = self.text[self.cursor..].chars().next() {
            self.cursor += c.len_utf8();
        }
    }
    pub fn take(&mut self) -> String {
        self.cursor = 0;
        self.slash_selection = 0;
        self.slash_menu_hidden = false;
        std::mem::take(&mut self.text)
    }
}

#[derive(Clone, Debug)]
pub struct App {
    pub transcript: Vec<TranscriptItem>,
    pub tool_cards: HashMap<String, usize>,
    pub preview: PreviewState,
    pub input: InputState,
    pub modal: ModalState,
    pub fullscreen: bool,
    pub prefix_active: bool,
    /// When set, prefix mode expires at this instant so a forgotten prefix cannot trap input.
    pub prefix_deadline: Option<Instant>,
    pub run_active: bool,
    pub should_quit: bool,
    pub deck_name: String,
    pub design_name: String,
    pub mode: String,
    pub model: String,
    pub token_usage: Option<(u64, u64)>,
    pub config: Config,
}

impl Default for App {
    fn default() -> Self {
        Self {
            transcript: vec![],
            tool_cards: HashMap::new(),
            preview: PreviewState::default(),
            input: InputState::default(),
            modal: ModalState::None,
            fullscreen: false,
            prefix_active: false,
            prefix_deadline: None,
            run_active: false,
            should_quit: false,
            deck_name: "No deck".into(),
            design_name: "Default".into(),
            mode: "supervised".into(),
            model: "-".into(),
            token_usage: None,
            config: Config::default(),
        }
    }
}

impl App {
    pub fn apply(&mut self, event: AppEvent) -> Vec<AppAction> {
        match event {
            AppEvent::Input(crossterm::event::Event::Key(key)) => self.handle_key(key),
            AppEvent::Run(event) => {
                self.apply_agent_event(event);
                vec![]
            }
            AppEvent::RunHandleReady { .. } => {
                self.run_active = true;
                vec![]
            }
            AppEvent::Approval(request) => {
                self.modal = ModalState::Approval(request);
                vec![]
            }
            AppEvent::RenderStarted { generation } => {
                if generation >= self.preview.generation() {
                    self.preview.status = PreviewStatus::Rendering { generation };
                }
                vec![]
            }
            AppEvent::RenderDone {
                generation,
                manifest,
            } => {
                if generation >= self.preview.generation() {
                    self.apply_manifest(generation, manifest);
                }
                vec![]
            }
            AppEvent::RenderFailed { generation, error } => {
                if generation >= self.preview.generation() {
                    self.preview.status = PreviewStatus::Failed { generation, error };
                }
                vec![]
            }
            AppEvent::RendererUnavailable(reason) => {
                self.preview.status = PreviewStatus::Unavailable { reason };
                vec![]
            }
            AppEvent::DeckFileChanged => {
                self.mark_preview_stale();
                vec![AppAction::RequestRender]
            }
            AppEvent::Tick(now) => {
                self.expire_prefix(now);
                vec![]
            }
            AppEvent::Input(_) => vec![],
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Vec<AppAction> {
        // Only key-down counts. Releases must not clear or activate prefix mode.
        if key.kind != KeyEventKind::Press {
            return vec![];
        }
        if !matches!(self.modal, ModalState::None) {
            return self.handle_modal_key(key);
        }
        if self.fullscreen {
            return self.handle_fullscreen_key(key);
        }
        if self.prefix_active {
            return self.handle_prefix_key(key);
        }
        if matches!(key.code, KeyCode::F(1) | KeyCode::F(2)) {
            self.modal = if key.code == KeyCode::F(1) {
                ModalState::Help
            } else {
                ModalState::CommandPalette(CommandPaletteState::default())
            };
            return vec![];
        }
        // Alt+S opens the slide prefix. Avoid Ctrl+B: herdr/tmux claim that chord, so the
        // follow-up key never reaches this app and prefix mode appears stuck.
        if key.modifiers == KeyModifiers::ALT && matches!(key.code, KeyCode::Char('s' | 'S')) {
            self.enter_prefix();
            return vec![];
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return match key.code {
                KeyCode::Char('k' | 'K') => {
                    self.modal = ModalState::CommandPalette(CommandPaletteState::default());
                    vec![]
                }
                KeyCode::Char(',') => {
                    self.modal =
                        ModalState::Configuration(Box::new(ConfigurationState::new(&self.config)));
                    vec![]
                }
                KeyCode::Char('p') => {
                    self.modal = ModalState::DesignPicker(Default::default());
                    vec![AppAction::OpenDesignPicker]
                }
                KeyCode::Char('o') => {
                    self.modal = ModalState::DeckPicker(Default::default());
                    vec![AppAction::OpenDeckPicker]
                }
                KeyCode::Char('r') => vec![AppAction::RequestRender],
                KeyCode::Char('v') => {
                    self.input.attach_active_slide = !self.input.attach_active_slide;
                    vec![]
                }
                KeyCode::Char('c' | 'C') if !self.input.text.is_empty() => {
                    self.input.clear();
                    vec![]
                }
                KeyCode::Char('c' | 'C') => {
                    self.should_quit = true;
                    vec![AppAction::Quit]
                }
                _ => vec![],
            };
        }
        match key.code {
            KeyCode::Esc if self.run_active => vec![AppAction::CancelRun],
            _ => self.handle_input_key(key),
        }
    }

    fn enter_prefix(&mut self) {
        self.prefix_active = true;
        self.prefix_deadline = Some(Instant::now() + PREFIX_TIMEOUT);
    }

    fn clear_prefix(&mut self) {
        self.prefix_active = false;
        self.prefix_deadline = None;
    }

    fn expire_prefix(&mut self, now: Instant) {
        if self.prefix_active && self.prefix_deadline.is_some_and(|deadline| now >= deadline) {
            self.clear_prefix();
        }
    }

    fn handle_prefix_key(&mut self, key: KeyEvent) -> Vec<AppAction> {
        // Toggle / cancel without running a slide action.
        if key.code == KeyCode::Esc
            || (key.modifiers == KeyModifiers::ALT && matches!(key.code, KeyCode::Char('s' | 'S')))
        {
            self.clear_prefix();
            return vec![];
        }

        self.clear_prefix();
        match key.code {
            KeyCode::Char('h' | 'k') | KeyCode::Left => self.navigate_previous(),
            KeyCode::Char('j' | 'l') | KeyCode::Right => self.navigate_next(),
            KeyCode::Home | KeyCode::Char('g') => self.navigate_first(),
            KeyCode::End | KeyCode::Char('G') => self.navigate_last(),
            KeyCode::Char('r') => vec![AppAction::RequestRender],
            KeyCode::Enter | KeyCode::Char('f') => {
                if self.preview.slide_count() > 0 {
                    self.fullscreen = true;
                }
                vec![]
            }
            // Unknown keys cancel prefix and are intentionally not typed into the prompt.
            _ => vec![],
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) -> Vec<AppAction> {
        let suggestions = self.input.slash_suggestions();
        match key.code {
            KeyCode::Esc if !suggestions.is_empty() => {
                self.input.slash_menu_hidden = true;
                vec![]
            }
            KeyCode::Up if !suggestions.is_empty() => {
                self.input.slash_selection = self.input.slash_selection.saturating_sub(1);
                vec![]
            }
            KeyCode::Down if !suggestions.is_empty() => {
                self.input.slash_selection =
                    (self.input.slash_selection + 1).min(suggestions.len() - 1);
                vec![]
            }
            KeyCode::Tab if !suggestions.is_empty() => {
                let selected = self.input.slash_selection.min(suggestions.len() - 1);
                self.input.set_text(suggestions[selected].name);
                vec![]
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.input.newline();
                vec![]
            }
            KeyCode::Enter => {
                let slash_action = exact_slash_command(self.input.text.trim()).or_else(|| {
                    suggestions
                        .get(
                            self.input
                                .slash_selection
                                .min(suggestions.len().saturating_sub(1)),
                        )
                        .map(|suggestion| suggestion.action)
                });
                if let Some(action) = slash_action {
                    self.input.take();
                    return self.run_slash_command(action);
                }
                if self.input.text.trim().is_empty() || self.run_active {
                    return vec![];
                }
                let text = self.input.take();
                let attach = std::mem::take(&mut self.input.attach_active_slide);
                self.run_active = true;
                self.transcript.push(TranscriptItem::Message(Message {
                    role: Role::User,
                    text: text.clone(),
                    complete: true,
                }));
                vec![AppAction::SendMessage {
                    text,
                    attach_active_slide: attach,
                }]
            }
            KeyCode::Char(c) => {
                self.input.insert(c);
                vec![]
            }
            KeyCode::Backspace => {
                self.input.backspace();
                vec![]
            }
            KeyCode::Delete => {
                self.input.delete();
                vec![]
            }
            KeyCode::Left => {
                self.input.move_left();
                vec![]
            }
            KeyCode::Right => {
                self.input.move_right();
                vec![]
            }
            KeyCode::Home => {
                self.input.cursor = 0;
                vec![]
            }
            KeyCode::End => {
                self.input.cursor = self.input.text.len();
                vec![]
            }
            _ => vec![],
        }
    }

    fn handle_fullscreen_key(&mut self, key: KeyEvent) -> Vec<AppAction> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('f') => {
                self.fullscreen = false;
                vec![]
            }
            KeyCode::Left | KeyCode::Char('k') => self.navigate_previous(),
            KeyCode::Right | KeyCode::Char('j') | KeyCode::Char(' ') => self.navigate_next(),
            KeyCode::Char('g') => self.navigate_first(),
            KeyCode::Char('G') => self.navigate_last(),
            _ => vec![],
        }
    }

    fn handle_modal_key(&mut self, key: KeyEvent) -> Vec<AppAction> {
        if let ModalState::CommandPalette(state) = &mut self.modal {
            let event = state.handle_key(key);
            return match event {
                CommandPaletteEvent::None => vec![],
                CommandPaletteEvent::Cancel => {
                    self.modal = ModalState::None;
                    vec![]
                }
                CommandPaletteEvent::Run(command) => {
                    self.modal = ModalState::None;
                    self.run_command(command)
                }
            };
        }
        if let ModalState::Configuration(state) = &mut self.modal {
            return match state.handle_key(key) {
                ConfigurationEvent::None => vec![],
                ConfigurationEvent::Cancel => {
                    self.modal = ModalState::None;
                    vec![]
                }
                ConfigurationEvent::Save(config) => {
                    self.modal = ModalState::None;
                    vec![AppAction::SaveConfiguration(config)]
                }
            };
        }
        if let ModalState::Approval(request) = &self.modal {
            let decision = match key.code {
                KeyCode::Char('a') | KeyCode::Enter => Some(ApprovalDecision::AllowOnce),
                KeyCode::Char('s') if request.allow_for_session => {
                    Some(ApprovalDecision::AllowForSession)
                }
                KeyCode::Char('d') | KeyCode::Char('n') | KeyCode::Esc => {
                    Some(ApprovalDecision::Deny)
                }
                _ => None,
            };
            if let Some(decision) = decision {
                let id = request.id.clone();
                self.modal = ModalState::None;
                return vec![AppAction::RespondApproval { id, decision }];
            }
            return vec![];
        }
        if key.code == KeyCode::Esc {
            self.modal = ModalState::None;
        }
        vec![]
    }

    fn run_slash_command(&mut self, action: SlashCommandAction) -> Vec<AppAction> {
        match action {
            SlashCommandAction::OpenPalette => {
                self.modal = ModalState::CommandPalette(CommandPaletteState::default());
                vec![]
            }
            SlashCommandAction::Run(command) => self.run_command(command),
        }
    }

    fn run_command(&mut self, command: Command) -> Vec<AppAction> {
        match command {
            Command::OpenDeck => {
                self.modal = ModalState::DeckPicker(Default::default());
                vec![AppAction::OpenDeckPicker]
            }
            Command::ChangeDesign => {
                self.modal = ModalState::DesignPicker(Default::default());
                vec![AppAction::OpenDesignPicker]
            }
            Command::RenderPreview => vec![AppAction::RequestRender],
            Command::Configure => {
                self.modal =
                    ModalState::Configuration(Box::new(ConfigurationState::new(&self.config)));
                vec![]
            }
            Command::ToggleAttachment => {
                self.input.attach_active_slide = !self.input.attach_active_slide;
                vec![]
            }
            Command::Present => {
                if self.preview.slide_count() > 0 {
                    self.fullscreen = true;
                }
                vec![]
            }
            Command::ShowHelp => {
                self.modal = ModalState::Help;
                vec![]
            }
            Command::Quit => {
                self.should_quit = true;
                vec![AppAction::Quit]
            }
        }
    }

    fn navigate_previous(&mut self) -> Vec<AppAction> {
        if self.preview.previous() {
            vec![AppAction::SetActiveSlide(self.preview.active)]
        } else {
            vec![]
        }
    }
    fn navigate_next(&mut self) -> Vec<AppAction> {
        if self.preview.next() {
            vec![AppAction::SetActiveSlide(self.preview.active)]
        } else {
            vec![]
        }
    }
    fn navigate_first(&mut self) -> Vec<AppAction> {
        if self.preview.first() {
            vec![AppAction::SetActiveSlide(self.preview.active)]
        } else {
            vec![]
        }
    }
    fn navigate_last(&mut self) -> Vec<AppAction> {
        if self.preview.last() {
            vec![AppAction::SetActiveSlide(self.preview.active)]
        } else {
            vec![]
        }
    }

    fn apply_manifest(&mut self, generation: u64, manifest: RenderManifest) {
        let old_active = self.preview.active;
        self.preview.slides = manifest
            .slides
            .into_iter()
            .map(|s| SlideItem {
                title: format!("Slide {}", s.index + 1),
                image_path: Some(s.image_path),
            })
            .collect();
        self.preview.active = old_active.min(self.preview.slides.len().saturating_sub(1));
        self.preview.status = PreviewStatus::Ready { generation };
    }
    fn mark_preview_stale(&mut self) {
        let generation = match self.preview.status {
            PreviewStatus::Ready { generation }
            | PreviewStatus::Rendering { generation }
            | PreviewStatus::Stale { generation } => generation,
            PreviewStatus::Failed { generation, .. } => generation,
            _ => 0,
        };
        self.preview.status = PreviewStatus::Stale { generation };
    }

    fn apply_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::TextDelta(delta) => match self.transcript.last_mut() {
                Some(TranscriptItem::Message(Message {
                    role: Role::Assistant,
                    text,
                    complete: false,
                })) => text.push_str(&delta),
                _ => self.transcript.push(TranscriptItem::Message(Message {
                    role: Role::Assistant,
                    text: delta,
                    complete: false,
                })),
            },
            AgentEvent::MessageFinished => {
                if let Some(TranscriptItem::Message(message)) = self.transcript.last_mut() {
                    message.complete = true;
                }
            }
            AgentEvent::ToolProposed { id, name, summary } => {
                let index = self.transcript.len();
                self.tool_cards.insert(id.clone(), index);
                self.transcript.push(TranscriptItem::Tool(ToolCard {
                    id,
                    name,
                    summary,
                    detail: String::new(),
                    status: ToolStatus::Proposed,
                }));
            }
            AgentEvent::ToolStarted { id } => self.update_tool(&id, ToolStatus::Running, None),
            AgentEvent::ToolUpdated { id, detail } => {
                self.update_tool(&id, ToolStatus::Running, Some(detail))
            }
            AgentEvent::ToolFinished { id, result } => match result {
                Ok(detail) => self.update_tool(&id, ToolStatus::Succeeded, Some(detail)),
                Err(detail) => self.update_tool(&id, ToolStatus::Failed, Some(detail)),
            },
            AgentEvent::RunFinished => self.run_active = false,
            AgentEvent::RunFailed(error) => {
                self.run_active = false;
                self.transcript.push(TranscriptItem::Message(Message {
                    role: Role::System,
                    text: error,
                    complete: true,
                }));
            }
        }
    }
    fn update_tool(&mut self, id: &str, status: ToolStatus, detail: Option<String>) {
        let Some(index) = self.tool_cards.get(id).copied() else {
            return;
        };
        if let Some(TranscriptItem::Tool(card)) = self.transcript.get_mut(index) {
            card.status = status;
            if let Some(detail) = detail {
                card.detail = detail;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app_with_slides(n: usize) -> App {
        let mut app = App::default();
        app.preview.slides = (0..n)
            .map(|i| SlideItem {
                title: format!("S{i}"),
                image_path: None,
            })
            .collect();
        app.fullscreen = true;
        app
    }
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn navigation_clamps_at_both_ends() {
        let mut app = app_with_slides(3);
        assert!(app.handle_key(key(KeyCode::Left)).is_empty());
        app.handle_key(key(KeyCode::Char('G')));
        assert_eq!(app.preview.active, 2);
        assert!(app.handle_key(key(KeyCode::Right)).is_empty());
    }
    #[test]
    fn next_navigation_emits_active_slide() {
        let mut app = app_with_slides(2);
        assert_eq!(
            app.handle_key(key(KeyCode::Right)),
            vec![AppAction::SetActiveSlide(1)]
        );
    }
    #[test]
    fn empty_deck_navigation_is_safe() {
        let mut app = app_with_slides(0);
        app.handle_key(key(KeyCode::Char('G')));
        assert_eq!(app.preview.active, 0);
    }
    #[test]
    fn tab_is_reserved_for_prompt_completion() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Char('x')));
        assert_eq!(app.input.text, "x");
    }
    #[test]
    fn slide_navigation_requires_the_prefix_outside_presentation() {
        let mut app = app_with_slides(2);
        app.fullscreen = false;

        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.preview.active, 0);
        assert_eq!(app.input.cursor, 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
        assert!(app.prefix_active);
        assert_eq!(
            app.handle_key(key(KeyCode::Char('l'))),
            vec![AppAction::SetActiveSlide(1)]
        );
        assert!(!app.prefix_active);
    }
    #[test]
    fn unicode_input_cursor_stays_valid() {
        let mut input = InputState::default();
        input.insert('é');
        input.insert('x');
        input.move_left();
        input.backspace();
        assert_eq!(input.text, "x");
        assert_eq!(input.cursor, 0);
    }
    #[test]
    fn tool_events_update_card_in_place() {
        let mut app = App::default();
        app.apply_agent_event(AgentEvent::ToolProposed {
            id: "1".into(),
            name: "render".into(),
            summary: "deck".into(),
        });
        app.apply_agent_event(AgentEvent::ToolFinished {
            id: "1".into(),
            result: Ok("done".into()),
        });
        assert_eq!(app.transcript.len(), 1);
        let TranscriptItem::Tool(card) = &app.transcript[0] else {
            panic!()
        };
        assert_eq!(card.status, ToolStatus::Succeeded);
    }
    #[test]
    fn stale_render_completion_cannot_overflow_selection() {
        let mut app = app_with_slides(4);
        app.preview.active = 3;
        app.apply_manifest(2, RenderManifest::default());
        assert_eq!(app.preview.active, 0);
    }
    #[test]
    fn active_image_path_tracks_selected_slide() {
        let mut app = app_with_slides(2);
        app.preview.slides[0].image_path = Some(PathBuf::from("/tmp/a.png"));
        app.preview.slides[1].image_path = Some(PathBuf::from("/tmp/b.png"));
        assert_eq!(
            app.preview.active_image_path(),
            Some(std::path::Path::new("/tmp/a.png"))
        );
        app.preview.select(1);
        assert_eq!(
            app.preview.active_image_path(),
            Some(std::path::Path::new("/tmp/b.png"))
        );
    }

    #[test]
    fn stale_render_results_are_discarded() {
        let mut app = app_with_slides(1);
        app.preview.status = PreviewStatus::Rendering { generation: 5 };
        app.apply(AppEvent::RenderDone {
            generation: 4,
            manifest: RenderManifest::default(),
        });
        assert_eq!(app.preview.slide_count(), 1);
        assert_eq!(
            app.preview.status,
            PreviewStatus::Rendering { generation: 5 }
        );
    }
    #[test]
    fn config_command_opens_grouped_configuration_menu() {
        let mut app = App::default();
        app.input.text = "/config".into();
        app.input.cursor = app.input.text.len();
        assert!(app.handle_key(key(KeyCode::Enter)).is_empty());
        let ModalState::Configuration(state) = &app.modal else {
            panic!("configuration modal was not opened")
        };
        assert!(state.menu.groups.len() >= 4);
        assert!(app.input.text.is_empty());
    }

    #[test]
    fn configuration_save_emits_persistence_action() {
        let mut app = App::default();
        app.modal = ModalState::Configuration(Box::new(ConfigurationState::new(&app.config)));
        let actions = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(actions, vec![AppAction::SaveConfiguration(Box::default())]);
        assert_eq!(app.modal, ModalState::None);
    }

    #[test]
    fn approval_event_produces_owned_response_action() {
        let mut app = App::default();
        app.modal = ModalState::Approval(super::super::event::ApprovalRequest {
            id: "approval-1".into(),
            title: "Write file?".into(),
            detail: "repo/file".into(),
            allow_for_session: true,
        });
        assert_eq!(
            app.handle_key(key(KeyCode::Char('a'))),
            vec![AppAction::RespondApproval {
                id: "approval-1".into(),
                decision: ApprovalDecision::AllowOnce
            }]
        );
        assert_eq!(app.modal, ModalState::None);
    }
}

#[cfg(test)]
#[path = "app_controls_tests.rs"]
mod controls_tests;
