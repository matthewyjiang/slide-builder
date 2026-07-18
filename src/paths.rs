use anyhow::{bail, Context, Result};
use directories::{BaseDirs, ProjectDirs};
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

pub const APP_NAME: &str = "slide-builder";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let dirs = ProjectDirs::from("", "", APP_NAME)
            .context("could not determine the user's configuration and data directories")?;
        Ok(Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_dir().to_path_buf(),
        })
    }

    pub fn from_roots(config_dir: impl Into<PathBuf>, data_dir: impl Into<PathBuf>) -> Self {
        Self {
            config_dir: config_dir.into(),
            data_dir: data_dir.into(),
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }
    pub fn projects_dir(&self) -> PathBuf {
        self.data_dir.join("projects")
    }
    pub fn skills_dir(&self) -> PathBuf {
        self.data_dir.join("skills")
    }
    pub fn project_dir(&self, cwd: &Path) -> Result<PathBuf> {
        Ok(self.projects_dir().join(project_key(cwd)?))
    }
    pub fn project_file(&self, cwd: &Path) -> Result<PathBuf> {
        Ok(self.project_dir(cwd)?.join("project.toml"))
    }
    pub fn sessions_dir(&self, cwd: &Path) -> Result<PathBuf> {
        Ok(self.project_dir(cwd)?.join("sessions"))
    }
    pub fn render_cache_dir(&self, cwd: &Path) -> Result<PathBuf> {
        Ok(self.project_dir(cwd)?.join("render-cache"))
    }

    pub fn create_app_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.config_dir)
            .with_context(|| format!("creating {}", self.config_dir.display()))?;
        std::fs::create_dir_all(&self.data_dir)
            .with_context(|| format!("creating {}", self.data_dir.display()))?;
        Ok(())
    }
}

pub fn home_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf())
}

/// Expands a leading `~` or `~/`. Other uses of `~` are left untouched.
pub fn expand_tilde(path: impl AsRef<Path>) -> Result<PathBuf> {
    expand_tilde_with_home(path.as_ref(), home_dir().as_deref())
}

pub fn expand_tilde_with_home(path: &Path, home: Option<&Path>) -> Result<PathBuf> {
    let mut components = path.components();
    match components.next() {
        Some(Component::Normal(first)) if first == "~" => {
            let home = home.context("cannot expand '~': home directory is unavailable")?;
            let mut expanded = home.to_path_buf();
            expanded.extend(components);
            Ok(expanded)
        }
        _ => Ok(path.to_path_buf()),
    }
}

/// Stable 16-character project identifier derived from the absolute cwd.
pub fn project_key(cwd: &Path) -> Result<String> {
    let absolute = if cwd.is_absolute() {
        cwd.to_path_buf()
    } else {
        std::env::current_dir()
            .context("determining current directory")?
            .join(cwd)
    };
    // Canonicalize when possible so aliases of an existing directory share state. Missing paths
    // are still hashable, which makes this helper useful before a project is created.
    let normalized = std::fs::canonicalize(&absolute).unwrap_or(absolute);
    let bytes = normalized.as_os_str().as_encoded_bytes();
    if bytes.is_empty() {
        bail!("project path cannot be empty");
    }
    let digest = Sha256::digest(bytes);
    Ok(digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_expansion_is_leading_only() {
        let home = Path::new("/home/tester");
        assert_eq!(
            expand_tilde_with_home(Path::new("~/decks"), Some(home)).unwrap(),
            home.join("decks")
        );
        assert_eq!(
            expand_tilde_with_home(Path::new("/tmp/~/decks"), Some(home)).unwrap(),
            Path::new("/tmp/~/decks")
        );
        assert!(expand_tilde_with_home(Path::new("~"), None).is_err());
    }

    #[test]
    fn project_keys_are_short_hex_and_stable() {
        let first = project_key(Path::new("/tmp/example-project")).unwrap();
        let second = project_key(Path::new("/tmp/example-project")).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 16);
        assert!(first.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(first, project_key(Path::new("/tmp/other-project")).unwrap());
    }

    #[test]
    fn app_paths_have_expected_shape() {
        let paths = AppPaths::from_roots("/cfg", "/data");
        assert_eq!(paths.config_file(), Path::new("/cfg/config.toml"));
        assert_eq!(paths.skills_dir(), Path::new("/data/skills"));
        assert!(paths
            .project_dir(Path::new("/repo"))
            .unwrap()
            .starts_with("/data/projects"));
    }
}
