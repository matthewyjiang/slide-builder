use rho_sdk::{
    model::ToolSpec,
    tool::{Tool, ToolContext, ToolError, ToolErrorKind, ToolFuture, ToolInvocation, ToolOutput},
};
use serde_json::json;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum UiToolCommand {
    Render,
    SetActiveSlide(usize),
}
#[derive(Debug, Clone)]
pub struct UiTool {
    name: &'static str,
    tx: mpsc::UnboundedSender<UiToolCommand>,
}
impl UiTool {
    pub fn render(tx: mpsc::UnboundedSender<UiToolCommand>) -> Self {
        Self {
            name: "render_deck",
            tx,
        }
    }
    pub fn set_active(tx: mpsc::UnboundedSender<UiToolCommand>) -> Self {
        Self {
            name: "set_active_slide",
            tx,
        }
    }
}
impl Tool for UiTool {
    fn spec(&self) -> ToolSpec {
        if self.name == "render_deck" {
            ToolSpec {
                name: self.name.into(),
                description: "Render every slide and wait for the generation result".into(),
                input_schema: json!({"type":"object"}),
            }
        } else {
            ToolSpec {
                name: self.name.into(),
                description: "Synchronize the TUI preview to a one-based slide index".into(),
                input_schema: json!({"type":"object","required":["index"],"properties":{"index":{"type":"integer","minimum":1}}}),
            }
        }
    }
    fn call<'a>(&'a self, inv: ToolInvocation, _: ToolContext) -> ToolFuture<'a> {
        Box::pin(async move {
            let cmd = if self.name == "render_deck" {
                UiToolCommand::Render
            } else {
                let i = inv
                    .arguments()
                    .get("index")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| {
                        ToolError::new(
                            ToolErrorKind::InvalidArguments,
                            "index must be a positive integer",
                        )
                    })?;
                UiToolCommand::SetActiveSlide(i as usize)
            };
            self.tx.send(cmd).map_err(|_| {
                ToolError::new(ToolErrorKind::Execution, "UI event loop is unavailable")
            })?;
            Ok(ToolOutput::text("queued"))
        })
    }
}
