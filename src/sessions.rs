//! CLI session commands and the persisted subset of the interactive workspace.
use anyhow::{bail, Context, Result};
use slide_builder::{
    agent::{
        deck_engine::DeckEngine,
        runtime::AgentHandle,
        session_store::{SessionState, SessionStore, StoredSession},
    },
    config::Config,
    paths::AppPaths,
    tui::App,
};
use std::{
    ffi::OsString,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
};

pub enum Command {
    List,
    Continue(Option<String>),
    New(PathBuf),
    Rename { id: String, name: String },
    Delete(String),
}

pub enum Launch {
    Exit,
    Fresh(PathBuf),
    Continue(Box<StoredSession>),
}

/// Apply manager actions without allowing deletion of the live checkpoint record.
pub enum SessionManagement {
    Rename { id: String, name: String },
    Delete(String),
}

pub fn manage(
    store: &SessionStore,
    current_id: &str,
    action: SessionManagement,
) -> Result<Vec<slide_builder::tui::modal::session_picker::SessionPickerEntry>> {
    match action {
        SessionManagement::Rename { id, name } => store.rename(&id, &name)?,
        SessionManagement::Delete(id) => {
            if id == current_id {
                bail!("Switch to another session before deleting the current session.");
            }
            store.delete(&id)?;
        }
    }
    picker_entries(store, current_id)
}

pub async fn run(command: Command) -> Result<Launch> {
    let store = SessionStore::open(&AppPaths::discover()?.database_file())?;
    match command {
        Command::List => println!("{}", serde_json::to_string_pretty(&store.list()?)?),
        Command::Rename { id, name } => {
            store.rename(&id, &name)?;
            println!("Renamed session {id}.");
        }
        Command::Delete(id) => {
            store.delete(&id)?;
            println!("Deleted session {id}. Deck files were not changed.");
        }
        Command::Continue(id) => {
            let id = match id {
                Some(id) => id,
                None => store
                    .list()?
                    .first()
                    .context("no saved sessions; use `slide-builder sessions new DECK.pptx`")?
                    .id
                    .clone(),
            };
            let session = load_for_resume(&store, &id)?;
            if !io::stdout().is_terminal() {
                bail!("continuing a session requires an interactive terminal");
            }
            return Ok(Launch::Continue(Box::new(session)));
        }
        Command::New(deck) => {
            if io::stdout().is_terminal() {
                return Ok(Launch::Fresh(deck));
            }
            let engine = super::open_engine(&deck).await?;
            let config = Config::load()?;
            let state = initial_state(engine.path(), &std::env::current_dir()?, &config);
            let snapshot = rho_sdk::SessionSnapshot::new(
                rho_sdk::SessionId::default(),
                rho_sdk::Revision::INITIAL,
                Vec::new(),
                rho_sdk::model::ModelIdentity::new(
                    &config.provider,
                    &config.provider,
                    &config.model,
                ),
                rho_sdk::CompactionState::default(),
            );
            println!("{}", store.create(snapshot, state)?.id);
        }
    }
    Ok(Launch::Exit)
}

/// Validate saved paths without ever creating a replacement deck or workspace.
pub fn load_for_resume(store: &SessionStore, id: &str) -> Result<StoredSession> {
    let session = store.load(id)?;
    if !session.state.deck.is_file() {
        bail!(
            "saved deck is missing: {}; restore the file before continuing",
            session.state.deck.display()
        );
    }
    if !session.state.cwd.is_dir() {
        bail!(
            "saved workspace is missing: {}",
            session.state.cwd.display()
        );
    }
    Ok(session)
}

/// Open and inspect the destination before leaving the current interactive session.
pub async fn prepare_resume(store: &SessionStore, id: &str) -> Result<(StoredSession, DeckEngine)> {
    let session = load_for_resume(store, id)?;
    let engine = DeckEngine::new(&session.state.deck)?;
    engine
        .snapshot()
        .await
        .context("could not open saved deck")?;
    Ok((session, engine))
}

pub fn picker_entries(
    store: &SessionStore,
    current: &str,
) -> Result<Vec<slide_builder::tui::modal::session_picker::SessionPickerEntry>> {
    use slide_builder::tui::modal::session_picker::SessionPickerEntry;
    Ok(store
        .list()?
        .into_iter()
        .map(|session| SessionPickerEntry {
            current: session.id == current,
            id: session.id,
            name: session.name,
            deck: session.deck,
            model: format!("{}/{}", session.provider, session.model),
        })
        .collect())
}

impl Command {
    pub fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Self> {
        let verb = args
            .next()
            .context("sessions requires list, continue, new, rename, or delete")?;
        let mut text = || -> Result<String> {
            args.next()
                .context("missing session argument")?
                .into_string()
                .map_err(|_| anyhow::anyhow!("session argument must be UTF-8"))
        };
        let command = match verb.to_str() {
            Some("list") => Self::List,
            Some("continue") => Self::Continue(
                args.next()
                    .map(|id| {
                        id.into_string()
                            .map_err(|_| anyhow::anyhow!("session ID must be UTF-8"))
                    })
                    .transpose()?,
            ),
            Some("new") => Self::New(
                args.next()
                    .map(PathBuf::from)
                    .context("sessions new requires DECK.pptx")?,
            ),
            Some("rename") => Self::Rename {
                id: text()?,
                name: text()?,
            },
            Some("delete") => Self::Delete(text()?),
            _ => bail!("unknown sessions command; use list, continue, new, rename, or delete"),
        };
        if args.next().is_some() {
            bail!("unexpected extra session argument");
        }
        Ok(command)
    }
}

pub fn initial_state(deck: &Path, cwd: &Path, config: &Config) -> SessionState {
    SessionState {
        deck: deck.into(),
        cwd: cwd.into(),
        provider: config.provider.clone(),
        auth: config.auth.clone(),
        model: config.model.clone(),
        active_slide: 0,
        design_name: "Default".into(),
        pending_design_context: None,
        transcript: Vec::new(),
        draft: String::new(),
        attach_active_slide: false,
    }
}

pub fn restore_app(app: &mut App, state: &SessionState, slide_count: usize) {
    app.transcript = state.transcript.clone();
    settle_transcript(&mut app.transcript);
    app.preview.active = state.active_slide.min(slide_count.saturating_sub(1));
    app.design_name = state.design_name.clone();
    app.input.text = state.draft.clone();
    app.input.cursor = app.input.text.len();
    app.input.attach_active_slide = state.attach_active_slide;
}

/// Resuming a model is session-local. Saving another setting must not silently
/// turn that restored model into the global default.
pub fn configuration_for_save(next: &Config, current: &Config, persisted: &Config) -> Config {
    let mut saved = next.clone();
    if (&next.provider, &next.auth, &next.model)
        == (&current.provider, &current.auth, &current.model)
    {
        saved.provider = persisted.provider.clone();
        saved.auth = persisted.auth.clone();
        saved.model = persisted.model.clone();
    }
    saved
}

pub fn checkpoint(
    store: &SessionStore,
    session: &mut StoredSession,
    agent: &AgentHandle,
    app: &App,
    config: &Config,
    pending_design_context: &Option<String>,
) -> Result<()> {
    if agent.is_active() {
        bail!("cannot checkpoint while the agent is active");
    }
    session.snapshot = agent.snapshot();
    session.state.provider = config.provider.clone();
    session.state.auth = config.auth.clone();
    session.state.model = config.model.clone();
    session.state.active_slide = app.preview.active;
    session.state.design_name = app.design_name.clone();
    session.state.pending_design_context = pending_design_context.clone();
    session.state.transcript = app.transcript.clone();
    settle_transcript(&mut session.state.transcript);
    session.state.draft = app.input.text.clone();
    session.state.attach_active_slide = app.input.attach_active_slide;
    store.save(session)
}

/// Saved transcripts describe settled history, never a live streaming operation.
fn settle_transcript(transcript: &mut [slide_builder::tui::TranscriptItem]) {
    use slide_builder::tui::{ToolStatus, TranscriptItem};
    for item in transcript {
        match item {
            TranscriptItem::Message(message) => message.complete = true,
            TranscriptItem::Tool(card) => match card.status {
                ToolStatus::Proposed | ToolStatus::Running => {
                    card.status = ToolStatus::Failed;
                    card.detail = "Session ended before this operation completed.".into();
                }
                ToolStatus::Succeeded | ToolStatus::Failed => {}
            },
        }
    }
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
