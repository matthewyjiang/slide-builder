use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillSource {
    Project(PathBuf),
    User(PathBuf),
    BuiltIn(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub source: SkillSource,
    pub path: PathBuf,
    pub contents: String,
}

const BUILTINS: &[(&str, &str)] = &[
    (
        "slide-builder-pptx",
        include_str!("builtin_skills/slide-builder-pptx/SKILL.md"),
    ),
    (
        "slide-design",
        include_str!("builtin_skills/slide-design/SKILL.md"),
    ),
    (
        "deck-assets",
        include_str!("builtin_skills/deck-assets/SKILL.md"),
    ),
    (
        "design-package-import",
        include_str!("builtin_skills/design-package-import/SKILL.md"),
    ),
];

/// Materializes embedded skills so future relative scripts and references resolve from disk.
pub fn materialize_builtins(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root)
        .with_context(|| format!("creating built-in skills directory {}", root.display()))?;
    for (name, contents) in BUILTINS {
        // Validate product assets before publishing them.
        parse_skill(
            contents,
            root.join(name).join("SKILL.md"),
            SkillSource::BuiltIn(root.join(name)),
        )?;
        let directory = root.join(name);
        std::fs::create_dir_all(&directory)?;
        let target = directory.join("SKILL.md");
        let unchanged = std::fs::read_to_string(&target).ok().as_deref() == Some(*contents);
        if !unchanged {
            let temporary = directory.join(format!(".SKILL.md.tmp-{}", std::process::id()));
            std::fs::write(&temporary, contents)?;
            std::fs::rename(&temporary, &target)
                .with_context(|| format!("publishing built-in skill {}", target.display()))?;
        }
    }
    Ok(())
}

/// Discovers skills in precedence order. The closest project ancestor wins, followed by user
/// skills and finally embedded built-ins. Invalid third-party skills are skipped.
pub fn discover(cwd: &Path, builtin_root: &Path, home: Option<&Path>) -> Result<Vec<Skill>> {
    materialize_builtins(builtin_root)?;
    let mut candidates = Vec::new();
    let mut current = Some(cwd);
    while let Some(directory) = current {
        candidates.extend(read_root(&directory.join(".agents/skills"), |path| {
            SkillSource::Project(path)
        }));
        current = directory.parent();
    }
    if let Some(home) = home {
        candidates.extend(read_root(&home.join(".agents/skills"), |path| {
            SkillSource::User(path)
        }));
    }
    candidates.extend(read_root(builtin_root, SkillSource::BuiltIn));

    let mut seen = HashSet::new();
    let mut skills = Vec::new();
    for (path, source) in candidates {
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(skill) = parse_skill(&contents, path, source) else {
            continue;
        };
        if seen.insert(skill.name.clone()) {
            skills.push(skill);
        }
    }
    Ok(skills)
}

fn read_root<F>(root: &Path, source: F) -> Vec<(PathBuf, SkillSource)>
where
    F: Fn(PathBuf) -> SkillSource,
{
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut directories = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    directories.sort();
    directories
        .into_iter()
        .map(|directory| (directory.join("SKILL.md"), source(directory)))
        .collect()
}

pub fn parse_skill(contents: &str, path: PathBuf, source: SkillSource) -> Result<Skill> {
    let fields = parse_frontmatter(contents)?;
    let name = field(&fields, "name")
        .context("missing required skill name")?
        .to_string();
    let description = field(&fields, "description")
        .context("missing required skill description")?
        .to_string();
    validate_name(&name)?;
    if description.is_empty() || description.len() > 1024 {
        bail!("skill description must be 1-1024 characters");
    }
    let directory_name = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .context("skill path has no UTF-8 directory name")?;
    if directory_name != name {
        bail!("skill name '{name}' must match directory name '{directory_name}'");
    }
    Ok(Skill {
        name,
        description,
        source,
        path,
        contents: contents.to_string(),
    })
}

fn field<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.as_str())
}

fn parse_frontmatter(contents: &str) -> Result<Vec<(String, String)>> {
    let lines = contents.lines().collect::<Vec<_>>();
    if lines.first().copied() != Some("---") {
        bail!("SKILL.md must start with YAML frontmatter");
    }
    let mut fields = Vec::new();
    let mut index = 1;
    while index < lines.len() {
        let line = lines[index];
        if line == "---" {
            return Ok(fields);
        }
        index += 1;
        if line.trim().is_empty() || line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let Some((key, raw_value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if !matches!(key, "name" | "description") {
            continue;
        }
        let raw_value = raw_value.trim();
        let value = if matches!(raw_value, "|" | "|-" | "|+" | ">" | ">-" | ">+") {
            let folded = raw_value.starts_with('>');
            let mut block = Vec::new();
            while index < lines.len() {
                let block_line = lines[index];
                if block_line == "---"
                    || (!block_line.is_empty()
                        && !block_line.starts_with(' ')
                        && !block_line.starts_with('\t'))
                {
                    break;
                }
                block.push(block_line.trim());
                index += 1;
            }
            let joined = if folded {
                block.join(" ")
            } else {
                block.join("\n")
            };
            joined.trim().to_string()
        } else {
            unquote(raw_value).to_string()
        };
        if fields.iter().any(|(existing, _)| existing == key) {
            bail!("duplicate skill frontmatter field '{key}'");
        }
        fields.push((key.to_string(), value));
    }
    bail!("unterminated YAML frontmatter")
}

fn unquote(value: &str) -> &str {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 64 {
        bail!("skill name must be 1-64 characters");
    }
    if name.starts_with('-')
        || name.ends_with('-')
        || name.contains("--")
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        bail!("skill name must be lowercase alphanumeric with single hyphen separators");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "slide-builder-skills-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    fn write_skill(root: &Path, name: &str, description: &str) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n# Body\n"),
        )
        .unwrap();
    }

    #[test]
    fn builtins_materialize_and_parse() {
        let root = temp_dir();
        materialize_builtins(&root).unwrap();
        for (name, _) in BUILTINS {
            assert!(root.join(name).join("SKILL.md").is_file());
        }
        let skills = discover(Path::new("/nonexistent"), &root, None).unwrap();
        assert_eq!(skills.len(), 4);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_shadows_user_and_builtin() {
        let root = temp_dir();
        let project = root.join("repo/sub");
        let home = root.join("home");
        std::fs::create_dir_all(&project).unwrap();
        write_skill(
            &project.join(".agents/skills"),
            "slide-design",
            "project version",
        );
        write_skill(&home.join(".agents/skills"), "slide-design", "user version");
        let skills = discover(&project, &root.join("builtins"), Some(&home)).unwrap();
        let selected = skills
            .iter()
            .find(|skill| skill.name == "slide-design")
            .unwrap();
        assert_eq!(selected.description, "project version");
        assert!(matches!(selected.source, SkillSource::Project(_)));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_folded_description_frontmatter() {
        let path = PathBuf::from("/skills/right/SKILL.md");
        let contents =
            "---\nname: right\ndescription: >\n  useful for decks\n  and layouts\n---\n# Body\n";
        let skill = parse_skill(contents, path.clone(), SkillSource::User(path)).unwrap();
        assert_eq!(skill.description, "useful for decks and layouts");
    }

    #[test]
    fn rejects_bad_frontmatter_and_directory_mismatch() {
        let path = PathBuf::from("/skills/right/SKILL.md");
        let mismatch = "---\nname: wrong\ndescription: useful\n---\n";
        assert!(parse_skill(
            mismatch,
            path.clone(),
            SkillSource::User(path.parent().unwrap().into())
        )
        .is_err());
        let malformed = "name: right\ndescription: useful";
        assert!(parse_skill(malformed, path.clone(), SkillSource::User(path)).is_err());
    }
}
