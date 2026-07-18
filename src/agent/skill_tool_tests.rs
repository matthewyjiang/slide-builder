use super::LoadSkillTool;
use crate::skills::{Skill, SkillSource};
use rho_sdk::tool::Tool;
use std::path::PathBuf;

#[test]
fn schema_advertises_only_discovered_skills() {
    let tool = LoadSkillTool::new(vec![Skill {
        name: "slide-builder-pptx".into(),
        description: "Deck instructions".into(),
        source: SkillSource::BuiltIn(PathBuf::from("/skills/slide-builder-pptx")),
        path: PathBuf::from("/skills/slide-builder-pptx/SKILL.md"),
        contents: "full instructions".into(),
    }]);

    let spec = tool.spec();
    assert_eq!(spec.name, "load_skill");
    assert_eq!(
        spec.input_schema["properties"]["name"]["enum"],
        serde_json::json!(["slide-builder-pptx"])
    );
}
