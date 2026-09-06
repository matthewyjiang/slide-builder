//! Agent events update transcript data independently of activity presentation.
use super::{AgentEvent, App, Message, Role, ToolCard, ToolStatus, TranscriptItem};

impl App {
    pub(super) fn apply_agent_event(&mut self, event: AgentEvent) {
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
            AgentEvent::ToolProposed {
                id,
                name,
                summary,
                arguments,
            } => {
                self.tool_activity.run_active = self.run_active;
                let index = self.transcript.len();
                self.tool_cards.insert(id.clone(), index);
                self.transcript.push(TranscriptItem::Tool(ToolCard {
                    id,
                    name,
                    summary,
                    arguments,
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
            AgentEvent::RunFinished => {
                self.finish_tool_activity("Run ended before this operation completed");
            }
            AgentEvent::RunCancelled => {
                self.finish_tool_activity("Run cancelled before this operation completed");
            }
            AgentEvent::RunFailed(error) => {
                self.finish_tool_activity(&error);
                self.transcript.push(TranscriptItem::Message(Message {
                    role: Role::System,
                    text: error,
                    complete: true,
                }));
            }
        }
        self.tool_activity.normalize_focus(&self.transcript);
    }

    fn finish_tool_activity(&mut self, reason: &str) {
        self.run_active = false;
        for item in &mut self.transcript {
            if let TranscriptItem::Tool(card) = item {
                if matches!(card.status, ToolStatus::Proposed | ToolStatus::Running) {
                    card.status = ToolStatus::Failed;
                    card.detail = reason.to_owned();
                }
            }
        }
        self.tool_activity.finish_run(self.transcript.len());
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
