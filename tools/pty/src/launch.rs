use std::{fs, path::Path};

use anyhow::Result;
use tempfile::TempDir;

/// Owns temporary application state. Keep alive until the child has exited.
pub struct IsolatedWorkspace {
    root: TempDir,
}

impl IsolatedWorkspace {
    pub fn new() -> Result<Self> {
        let root = tempfile::tempdir()?;
        for dir in ["home", "config", "data", "cache", "work"] {
            fs::create_dir(root.path().join(dir))?;
        }
        Ok(Self { root })
    }

    pub fn cwd(&self) -> std::path::PathBuf {
        self.root.path().join("work")
    }

    pub fn root(&self) -> &Path {
        self.root.path()
    }

    /// Complete child environment; credentials and host terminal markers are excluded.
    pub fn environment(&self) -> Vec<(String, String)> {
        let mut env = vec![
            ("TERM".into(), "xterm-256color".into()),
            ("LANG".into(), "C.UTF-8".into()),
            ("SLIDE_BUILDER_FORCE_FIRST_RUN".into(), "1".into()),
        ];
        for (key, dir) in [
            ("HOME", "home"),
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_CACHE_HOME", "cache"),
        ] {
            env.push((key.into(), self.root.path().join(dir).display().to_string()));
        }
        for key in ["PATH", "LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH"] {
            if let Ok(value) = std::env::var(key) {
                env.push((key.into(), value));
            }
        }
        env
    }
}
