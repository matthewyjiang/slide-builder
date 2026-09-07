//! Slide Builder policy for native Herdr reporting and terminal capability selection.
use ratatui_image::{
    picker::{Picker, ProtocolType},
    FontSize,
};
use slide_builder::{
    integrations::herdr::{HerdrClient, HerdrGraphicsCapability, HerdrReporter, HerdrState},
    tui::{modal::ModalState, AgentEvent, App, AppEvent, ImportDesignStatus, PreviewImage},
};
use std::io;

/// One workspace handle owns graphics discovery, reporting, and release.
/// Probe before raw mode, attach after the session exists, and shut down after
/// the terminal is restored so a hung host cannot freeze the alternate screen.
pub(crate) struct Workspace {
    client: HerdrClient,
    graphics: HerdrGraphicsCapability,
    reporter: Option<HerdrReporter>,
    status: Status,
}

impl Workspace {
    pub(crate) async fn discover() -> Self {
        let client = HerdrClient::from_env();
        let graphics = client.graphics_capability().await;
        Self {
            client,
            graphics,
            reporter: None,
            status: Status::default(),
        }
    }

    pub(crate) fn preview_image(&self, protocol: &str) -> PreviewImage {
        match preview_picker(self.graphics) {
            Some(picker) => PreviewImage::with_picker(protocol, picker),
            None => PreviewImage::detect(protocol),
        }
    }

    pub(crate) fn attach(&mut self, session_id: &str) {
        self.reporter = Some(self.client.start_reporting(session_id));
    }

    pub(crate) fn observe(&mut self, event: &AppEvent) {
        self.status.observe(event);
    }

    pub(crate) fn sync(&mut self, app: &App) {
        let (state, message) = self.status.sync(app);
        if let Some(reporter) = &self.reporter {
            reporter.report(state, Some(message));
        }
    }

    pub(crate) fn closing(&self) {
        if let Some(reporter) = &self.reporter {
            reporter.report(HerdrState::Working, Some("Closing deck"));
        }
    }

    pub(crate) async fn shutdown(mut self) -> io::Result<()> {
        match self.reporter.take() {
            Some(reporter) => reporter.shutdown().await,
            None => Ok(()),
        }
    }
}

#[derive(Default)]
struct Status {
    outcome: Option<&'static str>,
}

impl Status {
    fn observe(&mut self, event: &AppEvent) {
        let outcome = match event {
            AppEvent::Run(AgentEvent::RunFinished) => Some("Deck editing finished"),
            AppEvent::Run(AgentEvent::RunCancelled) => Some("Deck editing cancelled"),
            AppEvent::Run(AgentEvent::RunFailed(_)) => {
                Some("Deck editing failed; see conversation")
            }
            AppEvent::ExportFinished(Ok(_)) => Some("PDF exported"),
            AppEvent::ExportFinished(Err(_)) => Some("PDF export failed; see conversation"),
            AppEvent::ImportDesignCompleted { .. } => Some("Design imported"),
            AppEvent::ImportDesignFailed { .. } => Some("Design import failed; see conversation"),
            AppEvent::ImportDesignCancelled => Some("Design import cancelled"),
            _ => None,
        };
        if outcome.is_some() {
            self.outcome = outcome;
        }
    }

    /// Match input ownership before activity: a visible dialog can require attention
    /// even while a run is active. Preview refreshes never make the composer busy.
    /// Clears a finished-work outcome once the composer is busy again.
    fn sync(&mut self, app: &App) -> (HerdrState, &'static str) {
        let wait = match &app.modal {
            ModalState::Approval(_) => Some("Waiting for tool approval"),
            ModalState::Questionnaire(_) => Some("Waiting for an answer"),
            ModalState::None => None,
            ModalState::DeckPicker(_)
            | ModalState::TemplatePicker(_)
            | ModalState::DesignPicker(_)
            | ModalState::ImportDesignPicker(_)
            | ModalState::ModelPicker(_)
            | ModalState::SessionPicker(_)
            | ModalState::Setup(_)
            | ModalState::Configuration(_)
            | ModalState::CommandPalette(_)
            | ModalState::Help => Some("Close the dialog to return to the deck"),
        };
        if let Some(message) = wait {
            return (HerdrState::Blocked, message);
        }
        if app.fullscreen {
            return (
                HerdrState::Blocked,
                "Presenting slides; Escape returns to the deck",
            );
        }
        let activity = if matches!(
            app.import_design_status,
            Some(ImportDesignStatus::Running(_))
        ) {
            Some("Importing design")
        } else if app.run_active {
            Some("Editing deck")
        } else if app.export_active {
            Some("Exporting PDF")
        } else {
            None
        };
        if let Some(message) = activity {
            self.outcome = None;
            (HerdrState::Working, message)
        } else {
            (
                HerdrState::Idle,
                self.outcome.unwrap_or("Ready for a prompt"),
            )
        }
    }
}

/// Herdr owns host graphics discovery. Do not send terminal probes through its PTY
/// when its socket has already determined whether images can be painted.
fn preview_picker(capability: HerdrGraphicsCapability) -> Option<Picker> {
    match capability {
        HerdrGraphicsCapability::NotHerdr => None,
        HerdrGraphicsCapability::Unpaintable => Some(Picker::halfblocks()),
        HerdrGraphicsCapability::Paintable { width, height } => {
            // Host metrics replace a PTY query; ratatui-image has no other constructor
            // that accepts known cell dimensions.
            #[allow(deprecated)]
            let mut picker = Picker::from_fontsize(FontSize::new(width, height));
            picker.set_protocol_type(ProtocolType::Kitty);
            Some(picker)
        }
    }
}

#[cfg(test)]
#[path = "herdr_status_tests.rs"]
mod tests;
