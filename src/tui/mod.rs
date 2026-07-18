//! Fullscreen terminal user interface state, event protocol, and rendering.
//!
//! The application runtime owns the event loop. It forwards every asynchronous
//! source as an [`AppEvent`], calls [`App::apply`], executes returned
//! [`AppAction`] values, and redraws with [`render`].

pub mod app;
pub mod chat;
pub mod event;
pub mod layout;
pub mod modal;
pub mod mouse;
pub mod outline;
pub mod preview;
pub mod preview_image;
pub mod slideshow;
pub mod statusline;
pub mod theme;

pub use app::{
    App, InputState, Message, PreviewState, PreviewStatus, Role, SlideItem, ToolCard, ToolStatus,
    TranscriptItem,
};
pub use event::{
    AgentEvent, AppAction, AppEvent, ApprovalDecision, ApprovalRequest, RenderManifest, SlideRender,
};
pub use layout::{render, render_with_preview};
pub use preview_image::PreviewImage;
