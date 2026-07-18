use crate::design::DesignPackage;
use crate::skills::{Skill, SkillSource};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const BASE_PROMPT: &str = r#"You are slide-builder, a terminal-first agent that creates and edits PowerPoint decks with native deck tools.

Work on the active deck only unless the user explicitly asks to create or select another deck. Inspect before editing. Prefer semantic deck tools, preserve stable element IDs, validate meaningful changes, render every slide, and use visual feedback to fix defects. Never claim a mutation or render succeeded without a tool result. Keep deck content accurate, concise, legible, and appropriate for the user's audience.

Repository reads are available for research and asset generation. Deck writes are normal product work. Repository writes and processes may require approval. Network access from agent tools is unavailable. Do not use raw OOXML or optional external OfficeCLI unless native tools explicitly cannot perform the operation."#;

pub struct PromptContext<'a> {
    pub active_deck: &'a Path,
    pub decks_dir: &'a Path,
    pub repo_cwd: &'a Path,
    pub design: Option<&'a DesignPackage>,
    pub skills: &'a [Skill],
    pub slide_index: usize,
    pub slide_count: usize,
    pub deck_generation: u64,
}

pub fn assemble(context: &PromptContext<'_>) -> Result<String> {
    let mut prompt = String::from(BASE_PROMPT);
    prompt.push_str(&format!(
        "\n\n<workspace>\n<active_deck>{}</active_deck>\n<decks_dir>{}</decks_dir>\n<repo_cwd>{}</repo_cwd>\n</workspace>",
        xml(&absolute(context.active_deck)?), xml(&absolute(context.decks_dir)?), xml(&absolute(context.repo_cwd)?)
    ));

    if let Some(design) = context.design {
        prompt.push_str("\n\n<design_guidelines>");
        prompt.push_str(&xml_text(&design.guidelines));
        prompt.push_str("</design_guidelines>");
        if !design.supplementary_files.is_empty() {
            let paths = design
                .supplementary_files
                .iter()
                .map(|relative| design.path.join(relative).display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            prompt.push_str("\nDesign package supplementary files (read on demand): ");
            prompt.push_str(&paths);
        }
    }

    for (path, contents) in agent_instruction_files(context.repo_cwd)? {
        prompt.push_str(&format!(
            "\n\n<agents_instructions path=\"{}\">\n{}\n</agents_instructions>",
            xml(&path),
            xml_text(&contents)
        ));
    }

    if !context.skills.is_empty() {
        prompt.push_str("\n\n<available_skills>");
        for skill in context.skills {
            prompt.push_str(&format!(
                "\n  <skill>\n    <name>{}</name>\n    <description>{}</description>\n    <source>{}</source>\n  </skill>",
                xml_text(&skill.name), xml_text(&skill.description), xml_text(&source_text(&skill.source))
            ));
        }
        prompt.push_str("\n</available_skills>");
    }

    let active = if context.slide_count == 0 {
        0
    } else {
        context.slide_index.min(context.slide_count)
    };
    prompt.push_str(&format!(
        "\n\n<deck_state>active_slide={active}; slide_count={}; generation={}</deck_state>",
        context.slide_count, context.deck_generation
    ));
    Ok(prompt)
}

pub fn transition_message(
    old_deck: &Path,
    new_deck: &Path,
    old_design: Option<&str>,
    new_design: Option<&str>,
) -> String {
    format!(
        "[slide-builder context transition] Active deck changed from '{}' to '{}'; design package changed from '{}' to '{}'. Disregard stale deck paths, element IDs, slide state, and design guidance from the previous context. Inspect the new active deck before editing.",
        old_deck.display(), new_deck.display(), old_design.unwrap_or("None"), new_design.unwrap_or("None")
    )
}

fn agent_instruction_files(cwd: &Path) -> Result<Vec<(PathBuf, String)>> {
    let cwd = absolute(cwd)?;
    let mut directories = cwd.ancestors().map(Path::to_path_buf).collect::<Vec<_>>();
    directories.reverse();
    let mut output = Vec::new();
    for directory in directories {
        let path = directory.join("AGENTS.md");
        match std::fs::read_to_string(&path) {
            Ok(contents) => output.push((path, contents)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("reading instructions {}", path.display()))
            }
        }
    }
    Ok(output)
}

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .context("determining current directory")?
            .join(path))
    }
}
fn xml(path: &Path) -> String {
    xml_text(&path.display().to_string())
}
fn xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn source_text(source: &SkillSource) -> String {
    match source {
        SkillSource::Project(path) => format!("project: {}", path.display()),
        SkillSource::User(path) => format!("user: {}", path.display()),
        SkillSource::BuiltIn(path) => format!("built in: {}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "slide-builder-prompt-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn prompt_sections_have_required_order_and_escape_content() {
        let root = temp_dir();
        let repo = root.join("repo/sub");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(root.join("repo/AGENTS.md"), "parent rules").unwrap();
        std::fs::write(repo.join("AGENTS.md"), "specific <rules>").unwrap();
        let design = DesignPackage {
            name: "Brand".into(),
            path: root.join("design"),
            guidelines: "Use <blue>".into(),
            supplementary_files: vec!["TEMPLATE-REFERENCE.md".into()],
            templates: Vec::new(),
        };
        let skill = Skill {
            name: "test-skill".into(),
            description: "Helpful & safe".into(),
            source: SkillSource::Project(repo.clone()),
            path: repo.join("SKILL.md"),
            contents: String::new(),
        };
        let prompt = assemble(&PromptContext {
            active_deck: &root.join("deck.pptx"),
            decks_dir: &root,
            repo_cwd: &repo,
            design: Some(&design),
            skills: &[skill],
            slide_index: 2,
            slide_count: 5,
            deck_generation: 9,
        })
        .unwrap();
        let design_at = prompt.find("<design_guidelines>").unwrap();
        let parent_at = prompt.find("parent rules").unwrap();
        let child_at = prompt.find("specific &lt;rules&gt;").unwrap();
        let skills_at = prompt.find("<available_skills>").unwrap();
        let state_at = prompt.find("<deck_state>").unwrap();
        assert!(
            design_at < parent_at
                && parent_at < child_at
                && child_at < skills_at
                && skills_at < state_at
        );
        assert!(prompt.contains("Use &lt;blue&gt;"));
        assert!(prompt.contains("Helpful &amp; safe"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn transition_names_both_old_and_new_contexts() {
        let message = transition_message(
            Path::new("/old.pptx"),
            Path::new("/new.pptx"),
            Some("old-brand"),
            None,
        );
        assert!(message.contains("/old.pptx") && message.contains("/new.pptx"));
        assert!(message.contains("old-brand") && message.contains("None"));
        assert!(message.contains("Inspect the new active deck"));
    }

    #[test]
    fn empty_deck_uses_slide_zero() {
        let root = temp_dir();
        let prompt = assemble(&PromptContext {
            active_deck: &root.join("deck.pptx"),
            decks_dir: &root,
            repo_cwd: &root,
            design: None,
            skills: &[],
            slide_index: 8,
            slide_count: 0,
            deck_generation: 0,
        })
        .unwrap();
        assert!(prompt.contains("active_slide=0; slide_count=0"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
