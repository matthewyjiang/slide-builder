use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSession {
    pub schema_version: u32,
    pub session_id: String,
    pub snapshot: Value,
    #[serde(default)]
    pub transcript: Value,
}
#[derive(Debug, Clone)]
pub struct SessionStore {
    dir: PathBuf,
}
impl SessionStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
    pub fn path(&self, id: &str) -> Result<PathBuf> {
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            anyhow::bail!("invalid session id")
        };
        Ok(self.dir.join(format!("{id}.json")))
    }
    pub fn save(&self, value: &StoredSession) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.path(&value.session_id)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }
    pub fn load(&self, id: &str) -> Result<StoredSession> {
        let path = self.path(id)?;
        serde_json::from_slice(
            &std::fs::read(&path).with_context(|| format!("read {}", path.display()))?,
        )
        .context("session is corrupt or incompatible")
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let d = tempfile::tempdir().unwrap();
        let s = SessionStore::new(d.path());
        let v = StoredSession {
            schema_version: 1,
            session_id: "abc".into(),
            snapshot: Value::Null,
            transcript: Value::Null,
        };
        s.save(&v).unwrap();
        assert_eq!(s.load("abc").unwrap().session_id, "abc");
    }
}
