//! Events entering the TUI and actions leaving it.
//!
//! Integrations translate their native event types into these deliberately small,
//! app-owned values. This keeps the event loop independent of rho-sdk and render
//! pipeline implementation details.

use std::path::PathBuf;
use std::time::Instant;

use crate::config::Config;
use crossterm::event::Event as TerminalEvent;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlideRender {
    pub index: usize,
    pub image_path: PathBuf,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderManifest {
    pub slides: Vec<SlideRender>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentEvent {
    TextDelta(String),
    MessageFinished,
    ToolProposed {
        id: String,
        name: String,
        summary: String,
    },
    ToolStarted {
        id: String,
    },
    ToolUpdated {
        id: String,
        detail: String,
    },
    ToolFinished {
        id: String,
        /// Successful output remains in the agent session and is intentionally
        /// omitted from the user-facing transcript event.
        result: Result<(), String>,
    },
    RunFinished,
    RunCancelled,
    RunFailed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalRequest {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub allow_for_session: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportDesignStage {
    Reading,
    Analyzing,
    Building,
    Installing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppEvent {
    Input(TerminalEvent),
    Run(AgentEvent),
    RunHandleReady {
        run_id: String,
    },
    Approval(ApprovalRequest),
    RenderStarted {
        generation: u64,
    },
    RenderDone {
        generation: u64,
        manifest: RenderManifest,
    },
    RenderFailed {
        generation: u64,
        error: String,
    },
    RendererUnavailable(String),
    AgentRenderRequested,
    AgentSetActiveSlide(usize),
    DeckFileChanged,
    ImportDesignPickerOpened {
        start_directory: PathBuf,
    },
    ImportDesignStarted {
        source: PathBuf,
    },
    ImportDesignProgress {
        stage: ImportDesignStage,
        percent: Option<u8>,
    },
    ImportDesignCompleted {
        design_name: String,
    },
    ImportDesignFailed {
        error: String,
    },
    ImportDesignCancelled,
    DesignPickerOpened {
        entries: Vec<(String, PathBuf)>,
    },
    Tick(Instant),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalDecision {
    AllowOnce,
    AllowForSession,
    Deny,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppAction {
    None,
    Quit,
    SendMessage {
        text: String,
        attach_active_slide: bool,
    },
    CancelRun,
    RequestRender,
    OpenDeckPicker,
    OpenDesignPicker,
    SelectDesign(PathBuf),
    OpenImportDesignPicker,
    ImportDesign(PathBuf),
    SaveConfiguration(Box<Config>),
    RespondApproval {
        id: String,
        decision: ApprovalDecision,
    },
    SetActiveSlide(usize),
    CopyText(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integration_events_are_owned_and_comparable() {
        let event = AppEvent::Run(AgentEvent::ToolStarted {
            id: "call-1".into(),
        });
        assert_eq!(
            event,
            AppEvent::Run(AgentEvent::ToolStarted {
                id: "call-1".into()
            })
        );
    }

    #[test]
    fn render_manifest_has_safe_empty_default() {
        assert!(RenderManifest::default().slides.is_empty());
    }
}
