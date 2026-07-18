use crate::paths::{expand_tilde, AppPaths};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub schema_version: u32,
    pub decks_dir: PathBuf,
    pub provider: String,
    pub model: String,
    pub reasoning: String,
    pub permission_mode: PermissionMode,
    pub preview: PreviewConfig,
    pub render: RenderConfig,
    pub compat: CompatConfig,
    pub design_packages: Vec<DesignPackageConfig>,
    pub design_scan_dirs: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            decks_dir: PathBuf::from("~/decks"),
            provider: "anthropic".into(),
            model: String::new(),
            reasoning: "medium".into(),
            permission_mode: PermissionMode::Supervised,
            preview: PreviewConfig::default(),
            render: RenderConfig::default(),
            compat: CompatConfig::default(),
            design_packages: Vec::new(),
            design_scan_dirs: vec![PathBuf::from("~/slide-designs")],
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        Self::load_from(&AppPaths::discover()?.config_file())
    }

    /// Missing files yield defaults. Existing malformed or unsupported files are errors.
    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let config: Self =
            toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&AppPaths::discover()?.config_file())
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path.parent().context("config path has no parent")?;
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating config directory {}", parent.display()))?;
        let text = toml::to_string_pretty(self).context("serializing config")?;
        let temporary = parent.join(format!(
            ".{}.tmp-{}",
            path.file_name().unwrap_or_default().to_string_lossy(),
            std::process::id()
        ));
        std::fs::write(&temporary, text)
            .with_context(|| format!("writing temporary config {}", temporary.display()))?;
        if let Err(error) = std::fs::rename(&temporary, path) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error).with_context(|| format!("publishing config {}", path.display()));
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != CONFIG_SCHEMA_VERSION {
            bail!(
                "unsupported config schema version {} (expected {})",
                self.schema_version,
                CONFIG_SCHEMA_VERSION
            );
        }
        if self.provider.trim().is_empty() {
            bail!("provider cannot be empty");
        }
        if self.reasoning.trim().is_empty() {
            bail!("reasoning cannot be empty");
        }
        if self.preview.width == 0 {
            bail!("preview width must be greater than zero");
        }
        if self.preview.scale == 0 {
            bail!("preview scale must be greater than zero");
        }
        if self.render.timeout_ms == 0 {
            bail!("render timeout must be greater than zero");
        }
        if self.render.keep_generations == 0 {
            bail!("render keep_generations must be greater than zero");
        }
        for package in &self.design_packages {
            if package.name.trim().is_empty() {
                bail!("design package name cannot be empty");
            }
            if package.path.as_os_str().is_empty() {
                bail!("design package path cannot be empty");
            }
        }
        Ok(())
    }

    pub fn expanded_decks_dir(&self) -> Result<PathBuf> {
        expand_tilde(&self.decks_dir)
    }
    pub fn expanded_design_scan_dirs(&self) -> Result<Vec<PathBuf>> {
        self.design_scan_dirs.iter().map(expand_tilde).collect()
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Auto,
    Plan,
    #[default]
    Supervised,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PreviewConfig {
    pub enabled: bool,
    pub protocol: String,
    pub width: u32,
    pub scale: u32,
}
impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            protocol: "kitty".into(),
            width: 1600,
            scale: 2,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RenderConfig {
    pub browser_path: PathBuf,
    pub debounce_ms: u64,
    pub timeout_ms: u64,
    pub keep_generations: usize,
}
impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            browser_path: "auto".into(),
            debounce_ms: 1500,
            timeout_ms: 60_000,
            keep_generations: 5,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CompatConfig {
    pub officecli_path: PathBuf,
    pub detect_optional: bool,
}
impl Default for CompatConfig {
    fn default() -> Self {
        Self {
            officecli_path: "officecli".into(),
            detect_optional: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesignPackageConfig {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ProjectConfig {
    pub repo_path: PathBuf,
    pub active_deck: Option<PathBuf>,
    pub design_package: Option<String>,
    pub session_id: Option<String>,
}

impl ProjectConfig {
    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading project state {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing project state {}", path.display()))
    }
    pub fn save_to(&self, path: &Path) -> Result<()> {
        let parent = path.parent().context("project state path has no parent")?;
        std::fs::create_dir_all(parent)?;
        std::fs::write(path, toml::to_string_pretty(self)?)
            .with_context(|| format!("writing project state {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "slide-builder-config-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
            .join(name)
    }

    #[test]
    fn config_round_trip_preserves_values() {
        let path = temp_file("config.toml");
        let mut config = Config::default();
        config.model = "claude-test".into();
        config.design_packages.push(DesignPackageConfig {
            name: "brand".into(),
            path: "/design".into(),
        });
        config.save_to(&path).unwrap();
        assert_eq!(Config::load_from(&path).unwrap(), config);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn defaults_match_documented_runtime_defaults() {
        let config = Config::default();
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.permission_mode, PermissionMode::Supervised);
        assert_eq!(config.render.debounce_ms, 1500);
        assert_eq!(config.render.timeout_ms, 60_000);
    }

    #[test]
    fn invalid_schema_and_dimensions_are_rejected() {
        let mut config = Config::default();
        config.schema_version = 2;
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("schema"));
        config.schema_version = 1;
        config.preview.width = 0;
        assert!(config.validate().unwrap_err().to_string().contains("width"));
    }

    #[test]
    fn missing_config_uses_defaults_but_malformed_does_not() {
        let path = temp_file("missing/config.toml");
        assert_eq!(Config::load_from(&path).unwrap(), Config::default());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not valid = [").unwrap();
        assert!(Config::load_from(&path).is_err());
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
