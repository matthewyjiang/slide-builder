//! Application-wide session database. Snapshot and UI state commit together;
//! compare-and-swap revisions prevent stale writers from resurrecting deleted sessions.
use anyhow::{bail, Context, Result};
use rho_sdk::SessionSnapshot;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionState {
    pub deck: PathBuf,
    pub cwd: PathBuf,
    pub provider: String,
    pub auth: String,
    pub model: String,
    pub active_slide: usize,
    pub design_name: String,
    pub pending_design_context: Option<String>,
    pub transcript: Vec<crate::tui::TranscriptItem>,
    pub draft: String,
    pub attach_active_slide: bool,
}

#[derive(Clone, Debug)]
pub struct StoredSession {
    pub id: String,
    pub revision: i64,
    pub snapshot: SessionSnapshot,
    pub state: SessionState,
}

#[derive(Debug, Serialize)]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub deck: String,
    pub cwd: String,
    pub provider: String,
    pub model: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub revision: i64,
}

pub struct SessionStore {
    connection: Connection,
}

impl SessionStore {
    pub fn open(path: &Path) -> Result<Self> {
        let parent = path.parent().context("database path has no parent")?;
        std::fs::create_dir_all(parent)?;
        // Create privately before SQLite opens it, following Rho's owner-only store.
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        let mut connection = Connection::open(path)?;
        // Match the pinned Rho application's SQLite lock wait budget.
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let version: i64 =
            transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
        match version {
            0 => transaction.execute_batch(
                "CREATE TABLE sessions (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL,
                    deck TEXT NOT NULL, cwd TEXT NOT NULL,
                    provider TEXT NOT NULL, model TEXT NOT NULL,
                    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
                    activity INTEGER NOT NULL,
                    revision INTEGER NOT NULL DEFAULT 1,
                    state_version INTEGER NOT NULL DEFAULT 1,
                    snapshot TEXT NOT NULL, state TEXT NOT NULL
                );
                CREATE INDEX sessions_recent ON sessions(activity DESC);
                PRAGMA user_version = 1;",
            )?,
            1 => {}
            _ => bail!("unsupported application database version {version}; update slide-builder"),
        }
        transaction.commit()?;
        Ok(Self { connection })
    }

    pub fn create(&self, snapshot: SessionSnapshot, state: SessionState) -> Result<StoredSession> {
        let id = snapshot.session_id().to_string();
        let name = state.deck.file_name().unwrap_or_default().to_string_lossy();
        self.connection.execute(
            "INSERT INTO sessions(id,name,deck,cwd,provider,model,snapshot,state,activity) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,(SELECT COALESCE(MAX(activity),0)+1 FROM sessions))",
            params![id, name, state.deck.to_string_lossy(), state.cwd.to_string_lossy(), state.provider, state.model, snapshot.to_json()?, serde_json::to_string(&state)?],
        )?;
        Ok(StoredSession {
            id,
            revision: 1,
            snapshot,
            state,
        })
    }

    pub fn load(&self, id: &str) -> Result<StoredSession> {
        let (revision, version, snapshot, state): (i64, i64, String, String) = self
            .connection
            .query_row(
                "SELECT revision,state_version,snapshot,state FROM sessions WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .with_context(|| {
                format!("session {id} not found; use `slide-builder sessions list`")
            })?;
        if version != 1 {
            bail!("unsupported session state version {version}");
        }
        let snapshot = SessionSnapshot::from_json(&snapshot)
            .context("session snapshot is corrupt or incompatible")?;
        if snapshot.session_id().to_string() != id {
            bail!("session snapshot ID does not match database ID");
        }
        Ok(StoredSession {
            id: id.into(),
            revision,
            snapshot,
            state: serde_json::from_str(&state)
                .context("session UI state is corrupt or incompatible")?,
        })
    }

    pub fn check_revision(&self, session: &StoredSession) -> Result<()> {
        let revision: Option<i64> = self
            .connection
            .query_row(
                "SELECT revision FROM sessions WHERE id=?1",
                [&session.id],
                |row| row.get(0),
            )
            .optional()?;
        if revision != Some(session.revision) {
            bail!(
                "session {} changed or was deleted by another process; reopen it before continuing",
                session.id
            );
        }
        Ok(())
    }

    pub fn save(&self, session: &mut StoredSession) -> Result<()> {
        if session.snapshot.session_id().to_string() != session.id {
            bail!("session snapshot ID does not match database ID");
        }
        let state = &session.state;
        let changed = self.connection.execute(
            "UPDATE sessions SET snapshot=?1,state=?2,deck=?3,cwd=?4,provider=?5,model=?6,revision=revision+1,updated_at=unixepoch(),activity=(SELECT COALESCE(MAX(activity),0)+1 FROM sessions) WHERE id=?7 AND revision=?8",
            params![session.snapshot.to_json()?, serde_json::to_string(state)?, state.deck.to_string_lossy(), state.cwd.to_string_lossy(), state.provider, state.model, session.id, session.revision],
        )?;
        if changed != 1 {
            bail!("session {} changed or was deleted by another process; this checkpoint was not saved", session.id);
        }
        session.revision += 1;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<SessionSummary>> {
        let mut query = self.connection.prepare("SELECT id,name,deck,cwd,provider,model,created_at,updated_at,revision FROM sessions ORDER BY activity DESC")?;
        let rows = query
            .query_map([], |row| {
                Ok(SessionSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    deck: row.get(2)?,
                    cwd: row.get(3)?,
                    provider: row.get(4)?,
                    model: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    revision: row.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<()> {
        if name.trim().is_empty() || name.chars().any(char::is_control) {
            bail!("session name must be nonempty text without control characters");
        }
        // Metadata-only SQL: never rewrite a snapshot read before this transaction.
        if self.connection.execute(
            "UPDATE sessions SET name=?1,updated_at=unixepoch(),activity=(SELECT COALESCE(MAX(activity),0)+1 FROM sessions) WHERE id=?2",
            params![name.trim(), id],
        )? != 1
        {
            bail!("session {id} not found");
        }
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        if self
            .connection
            .execute("DELETE FROM sessions WHERE id=?1", [id])?
            != 1
        {
            bail!("session {id} not found");
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "session_store_tests.rs"]
mod tests;
