//! Fullscreen terminal user interface state, event protocol, and rendering.
//!
//! The application runtime owns the event loop. It forwards every asynchronous
//! source as an [`AppEvent`], calls [`App::apply`], executes returned
//! [`AppAction`] values, and redraws with [`render`].

pub mod app;
pub mod chat;
mod composer;
mod conversation_entry;
mod conversation_images;
mod conversation_markdown;
mod conversation_media;
pub mod event;
pub mod layout;
mod markdown;
mod markdown_image;
mod markdown_theme;
pub mod modal;
pub mod mouse;
pub mod outline;
pub mod preview;
pub mod preview_image;
mod render;
pub mod slideshow;
pub mod statusline;
mod syntax;
mod terminal_graph;
pub mod theme;
mod tool_activity;
mod tool_activity_render;

pub use app::{
    App, ImportDesignStatus, ImportProgress, InputState, Message, PreviewState, PreviewStatus,
    Role, SlideItem, ToolCard, ToolStatus, TranscriptItem,
};
pub use event::{
    AgentEvent, AppAction, AppEvent, ApprovalDecision, ApprovalRequest, ImportDesignStage,
    RenderManifest, SlideRender,
};
pub use layout::{render, render_with_preview};
pub use preview_image::PreviewImage;
