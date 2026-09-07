//! Shared executable discovery and validation for renderers and sandbox launchers.
use anyhow::{bail, Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub(super) fn executable_path(configured: &Path, candidates: &[&str]) -> Result<PathBuf> {
    if configured != Path::new("auto") {
        return validate_executable(configured);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    let directories: Vec<_> = std::env::split_paths(&path)
        .chain([PathBuf::from("/usr/bin"), PathBuf::from("/usr/local/bin")])
        .collect();
    for name in candidates {
        for directory in &directories {
            if let Ok(path) = validate_executable(&directory.join(name)) {
                return Ok(path);
            }
        }
    }
    bail!("executable not found on PATH: {}", candidates.join(", "))
}

pub(super) fn validate_executable(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("renderer path must be absolute: {}", path.display());
    }
    let metadata = fs::metadata(path)
        .with_context(|| format!("cannot inspect executable {}", path.display()))?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        bail!("not an executable file: {}", path.display());
    }
    fs::canonicalize(path).with_context(|| format!("resolve executable {}", path.display()))
}
