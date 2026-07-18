use crate::config::{Config, DesignPackageConfig};
use crate::paths::{expand_tilde, AppPaths};
use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignPackage {
    pub name: String,
    pub path: PathBuf,
    pub guidelines: String,
    pub supplementary_files: Vec<PathBuf>,
    pub templates: Vec<DeckTemplate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeckTemplate {
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub size_bytes: u64,
}

impl DesignPackage {
    pub fn load(path: &Path, configured_name: Option<&str>) -> Result<Self> {
        let path = expand_tilde(path)?;
        if !path.is_dir() {
            bail!("design package is not a directory: {}", path.display());
        }
        let design_file = path.join("DESIGN.md");
        let guidelines = std::fs::read_to_string(&design_file)
            .with_context(|| format!("reading required {}", design_file.display()))?;
        let heading = first_h1(&guidelines);
        let name = configured_name
            .filter(|name| !name.trim().is_empty())
            .map(str::to_owned)
            .or(heading)
            .ok_or_else(|| {
                anyhow::anyhow!("{} must contain an H1 display name", design_file.display())
            })?;
        let mut all_files = Vec::new();
        collect_files(&path, &path, &mut all_files)?;
        all_files.sort();
        let mut templates = Vec::new();
        let mut supplementary_files = Vec::new();
        for relative in all_files {
            if relative == Path::new("DESIGN.md") {
                continue;
            }
            let absolute = path.join(&relative);
            if is_pptx(&relative) {
                templates.push(DeckTemplate {
                    size_bytes: std::fs::metadata(&absolute)?.len(),
                    path: absolute,
                    relative_path: relative,
                });
            } else {
                supplementary_files.push(relative);
            }
        }
        templates.sort_by_key(template_sort_key);
        Ok(Self {
            name,
            path,
            guidelines,
            supplementary_files,
            templates,
        })
    }
}

/// Discovers explicit packages first, then the scan directory itself and its immediate children.
/// Invalid scanned candidates are ignored; invalid explicit entries are reported to the user.
pub fn discover(config: &Config) -> Result<Vec<DesignPackage>> {
    let managed = AppPaths::discover()?.design_packages_dir();
    discover_with_managed(config, Some(&managed))
}

pub fn discover_with_managed(
    config: &Config,
    managed_root: Option<&Path>,
) -> Result<Vec<DesignPackage>> {
    let mut packages = Vec::new();
    let mut seen = HashSet::new();
    for entry in &config.design_packages {
        let package = load_configured(entry)?;
        if seen.insert(path_identity(&package.path)) {
            packages.push(package);
        }
    }
    let mut scan_roots = Vec::new();
    if let Some(managed_root) = managed_root {
        scan_roots.push(managed_root.to_path_buf());
    }
    scan_roots.extend(config.expanded_design_scan_dirs()?);
    for root in scan_roots {
        let mut candidates = Vec::new();
        if root.join("DESIGN.md").is_file() {
            candidates.push(root.clone());
        }
        if let Ok(entries) = std::fs::read_dir(&root) {
            candidates.extend(
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.is_dir()
                            && path.join("DESIGN.md").is_file()
                            && !path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .is_some_and(|name| name.starts_with('.'))
                    }),
            );
        }
        candidates.sort();
        for candidate in candidates {
            if seen.contains(&path_identity(&candidate)) {
                continue;
            }
            if let Ok(package) = DesignPackage::load(&candidate, None) {
                seen.insert(path_identity(&package.path));
                packages.push(package);
            }
        }
    }
    Ok(packages)
}

fn load_configured(entry: &DesignPackageConfig) -> Result<DesignPackage> {
    DesignPackage::load(&entry.path, Some(&entry.name))
        .with_context(|| format!("loading configured design package '{}'", entry.name))
}

fn first_h1(markdown: &str) -> Option<String> {
    markdown.lines().find_map(|line| {
        let value = line.strip_prefix("# ")?.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn collect_files(root: &Path, directory: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = std::fs::read_dir(directory)
        .with_context(|| format!("reading design package directory {}", directory.display()))?
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect_files(root, &path, output)?;
        } else if file_type.is_file() {
            output.push(
                path.strip_prefix(root)
                    .expect("walk remains under package root")
                    .to_path_buf(),
            );
        }
    }
    Ok(())
}

fn is_pptx(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("pptx"))
}

fn template_sort_key(template: &DeckTemplate) -> (bool, String) {
    let preferred = template
        .relative_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("template.pptx"));
    (
        !preferred,
        template.relative_path.to_string_lossy().to_lowercase(),
    )
}

fn path_identity(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "slide-builder-design-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    fn package(root: &Path, name: &str) -> PathBuf {
        let path = root.join(name);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("DESIGN.md"), format!("# {name}\n\nRules.")).unwrap();
        path
    }

    #[test]
    fn loads_heading_supplements_and_ordered_templates() {
        let root = temp_dir();
        let path = package(&root, "Brand Name");
        std::fs::create_dir(path.join("nested")).unwrap();
        std::fs::write(path.join("z.pptx"), [0; 3]).unwrap();
        std::fs::write(path.join("nested/template.PPTX"), [0; 7]).unwrap();
        std::fs::write(path.join("TEMPLATE-REFERENCE.md"), "reference").unwrap();
        let loaded = DesignPackage::load(&path, None).unwrap();
        assert_eq!(loaded.name, "Brand Name");
        assert_eq!(
            loaded.templates[0].relative_path,
            Path::new("nested/template.PPTX")
        );
        assert_eq!(loaded.templates[0].size_bytes, 7);
        assert_eq!(
            loaded.supplementary_files,
            vec![PathBuf::from("TEMPLATE-REFERENCE.md")]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_packages_precede_scanned_and_are_deduplicated() {
        let root = temp_dir();
        let explicit = package(&root, "explicit");
        let _scanned = package(&root, "scanned");
        let config = Config {
            design_packages: vec![DesignPackageConfig {
                name: "Configured".into(),
                path: explicit,
            }],
            design_scan_dirs: vec![root.clone()],
            ..Config::default()
        };
        let found = discover_with_managed(&config, None).unwrap();
        assert_eq!(
            found.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            vec!["Configured", "scanned"]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_managed_packages_before_configured_scan_roots() {
        let root = temp_dir();
        let managed = root.join("managed");
        let scanned = root.join("scanned");
        let _managed_package = package(&managed, "managed-design");
        let _hidden_staging = package(&managed, ".import-staging");
        let _scanned_package = package(&scanned, "scanned-design");
        let config = Config {
            design_scan_dirs: vec![scanned],
            ..Config::default()
        };
        let found = discover_with_managed(&config, Some(&managed)).unwrap();
        assert_eq!(
            found
                .iter()
                .map(|package| package.name.as_str())
                .collect::<Vec<_>>(),
            vec!["managed-design", "scanned-design"]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_design_file_is_actionable() {
        let root = temp_dir();
        assert!(DesignPackage::load(&root, None)
            .unwrap_err()
            .to_string()
            .contains("DESIGN.md"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
