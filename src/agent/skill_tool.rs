use crate::skills::Skill;
use rho_sdk::{
    model::ToolSpec,
    tool::{
        OperationKind, Tool, ToolContext, ToolError, ToolErrorKind, ToolFuture, ToolInvocation,
        ToolMetadata, ToolOutput, ToolSecurity,
    },
    CapabilityKind, CapabilityRequest, CapabilitySource,
};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct LoadSkillTool {
    skills: Arc<Vec<Skill>>,
}

impl LoadSkillTool {
    pub fn new(skills: Vec<Skill>) -> Self {
        Self {
            skills: Arc::new(skills),
        }
    }
}

impl Tool for LoadSkillTool {
    fn spec(&self) -> ToolSpec {
        let names = self
            .skills
            .iter()
            .map(|skill| Value::String(skill.name.clone()))
            .collect::<Vec<_>>();
        ToolSpec {
            name: "load_skill".into(),
            description: "Load the complete instructions for an available skill before performing matching work."
                .into(),
            input_schema: json!({
                "type": "object",
                "required": ["name"],
                "properties": {"name": {"type": "string", "enum": names}},
                "additionalProperties": false
            }),
        }
    }

    fn security(&self) -> ToolSecurity {
        ToolSecurity::built_in([CapabilityKind::Skill])
    }

    fn start_metadata(&self, _: &Value) -> ToolMetadata {
        ToolMetadata::new().operation(OperationKind::Read)
    }

    fn call<'a>(&'a self, invocation: ToolInvocation, context: ToolContext) -> ToolFuture<'a> {
        Box::pin(async move {
            let arguments = invocation.into_arguments();
            let name = arguments
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ToolError::new(ToolErrorKind::InvalidArguments, "name must be a skill name")
                })?;
            let skill = self
                .skills
                .iter()
                .find(|skill| skill.name == name)
                .ok_or_else(|| {
                    ToolError::new(
                        ToolErrorKind::InvalidArguments,
                        format!("skill `{name}` is not available"),
                    )
                })?;
            context
                .authorize(CapabilityRequest::skill(
                    &skill.name,
                    Some(skill.path.clone()),
                    CapabilitySource::host_tool("load_skill"),
                ))
                .await
                .map_err(|error| ToolError::policy_denied(&error))?;
            Ok(ToolOutput::text(skill.contents.clone()).metadata(
                ToolMetadata::new()
                    .operation(OperationKind::Read)
                    .affected_path(&skill.path),
            ))
        })
    }
}

#[cfg(test)]
#[path = "skill_tool_tests.rs"]
mod tests;
