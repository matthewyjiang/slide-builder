use rho_sdk::{
    model::ToolSpec,
    tool::{Tool, ToolContext, ToolError, ToolErrorKind, ToolFuture, ToolInvocation, ToolOutput},
};
use serde_json::json;
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
pub enum UiToolCommand {
    Render {
        response: oneshot::Sender<Result<Vec<PathBuf>, String>>,
    },
    SetActiveSlide {
        index: usize,
        response: oneshot::Sender<Result<(), String>>,
    },
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
                description: "Render every slide, wait for completion, and return the generated image paths. The paths synchronize the user-facing preview but do not attach image bytes to the model."
                    .into(),
                input_schema: json!({"type":"object","additionalProperties":false}),
            }
        } else {
            ToolSpec {
                name: self.name.into(),
                description: "Synchronize the TUI preview to a one-based slide index".into(),
                input_schema: json!({"type":"object","required":["index"],"properties":{"index":{"type":"integer","minimum":1}},"additionalProperties":false}),
            }
        }
    }
    fn call<'a>(&'a self, inv: ToolInvocation, context: ToolContext) -> ToolFuture<'a> {
        Box::pin(async move {
            if self.name == "render_deck" {
                let (response, receiver) = oneshot::channel();
                self.tx
                    .send(UiToolCommand::Render { response })
                    .map_err(|_| unavailable())?;
                let paths = await_response(receiver, &context).await?;
                let mut output = format!("Rendered {} slides.", paths.len());
                for (index, path) in paths.iter().enumerate() {
                    output.push_str(&format!("\nslide {}: {}", index + 1, path.display()));
                }
                output.push_str(
                    "\nThe images are visible in the TUI preview but are not attached to the model. Ask the user to attach a slide when visual inspection is required.",
                );
                Ok(ToolOutput::text(output))
            } else {
                let index = positive_index(inv.arguments())?;
                let (response, receiver) = oneshot::channel();
                self.tx
                    .send(UiToolCommand::SetActiveSlide { index, response })
                    .map_err(|_| unavailable())?;
                await_response(receiver, &context).await?;
                Ok(ToolOutput::text(format!("Active slide set to {index}.")))
            }
        })
    }
}

fn positive_index(arguments: &serde_json::Value) -> Result<usize, ToolError> {
    let index = arguments
        .get("index")
        .and_then(|value| value.as_u64())
        .filter(|index| *index > 0 && *index <= usize::MAX as u64)
        .ok_or_else(|| {
            ToolError::new(
                ToolErrorKind::InvalidArguments,
                "index must be a positive integer",
            )
        })?;
    Ok(index as usize)
}

async fn await_response<T>(
    receiver: oneshot::Receiver<Result<T, String>>,
    context: &ToolContext,
) -> Result<T, ToolError> {
    tokio::select! {
        result = receiver => result
            .map_err(|_| unavailable())?
            .map_err(|error| ToolError::new(ToolErrorKind::Execution, error)),
        () = context.cancellation().cancelled() => {
            Err(ToolError::new(ToolErrorKind::Cancelled, "tool call cancelled"))
        }
    }
}

fn unavailable() -> ToolError {
    ToolError::new(ToolErrorKind::Execution, "UI event loop is unavailable")
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
