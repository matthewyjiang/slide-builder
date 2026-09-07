use super::*;
use rho_sdk::{model::ModelIdentity, CompactionState, Revision, SessionId};

fn snapshot() -> SessionSnapshot {
    SessionSnapshot::new(
        SessionId::default(),
        Revision::INITIAL,
        Vec::new(),
        ModelIdentity::new("scripted", "scripted", "test"),
        CompactionState::default(),
    )
}

fn state() -> SessionState {
    SessionState {
        deck: "/decks/example.pptx".into(),
        cwd: "/workspace".into(),
        provider: "scripted".into(),
        auth: "api-key".into(),
        model: "test".into(),
        active_slide: 2,
        design_name: "Studio".into(),
        pending_design_context: Some("selected guidelines".into()),
        transcript: vec![],
        draft: "next edit".into(),
        attach_active_slide: true,
    }
}

#[test]
fn atomic_roundtrip_metadata_updates_and_stale_writers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.sqlite3");
    let first = SessionStore::open(&path).unwrap();
    let mut session = first.create(snapshot(), state()).unwrap();
    let second = SessionStore::open(&path).unwrap();
    let mut stale = second.load(&session.id).unwrap();
    session.state.draft = "saved edit".into();
    first.save(&mut session).unwrap();
    assert!(second
        .save(&mut stale)
        .unwrap_err()
        .to_string()
        .contains("changed or was deleted"));
    assert_eq!(first.load(&session.id).unwrap().state.draft, "saved edit");
    second.rename(&session.id, "Renamed deck").unwrap();
    first.check_revision(&session).unwrap();
    first.save(&mut session).unwrap();
    let renamed = second.load(&session.id).unwrap();
    assert_eq!(renamed.snapshot, session.snapshot);
    assert_eq!(renamed.state.draft, "saved edit");
    assert_eq!(second.list().unwrap()[0].name, "Renamed deck");
    second.delete(&session.id).unwrap();
    assert!(first.save(&mut session).is_err());
    assert!(first.load(&session.id).is_err());
    assert!(first.list().unwrap().is_empty());
}

#[test]
fn incompatible_or_corrupt_state_never_replaced_and_ids_must_match() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.sqlite3");
    let store = SessionStore::open(&path).unwrap();
    let mut session = store.create(snapshot(), state()).unwrap();
    let original = session.snapshot.clone();
    session.snapshot = snapshot();
    assert!(store.save(&mut session).is_err());
    assert_eq!(store.load(&session.id).unwrap().snapshot, original);
    store
        .connection
        .execute(
            "UPDATE sessions SET state_version=2 WHERE id=?1",
            [&session.id],
        )
        .unwrap();
    assert!(store
        .load(&session.id)
        .unwrap_err()
        .to_string()
        .contains("unsupported session state"));
    store
        .connection
        .execute(
            "UPDATE sessions SET state_version=1,snapshot='broken' WHERE id=?1",
            [&session.id],
        )
        .unwrap();
    assert!(store.load(&session.id).is_err());
    assert_eq!(store.list().unwrap().len(), 1);
    store
        .connection
        .pragma_update(None, "user_version", 2)
        .unwrap();
    assert!(SessionStore::open(&path).is_err());
}

#[test]
fn database_is_owner_only_and_names_are_validated() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.sqlite3");
    let store = SessionStore::open(&path).unwrap();
    let session = store.create(snapshot(), state()).unwrap();
    assert!(store.rename(&session.id, "\n").is_err());
    assert!(store.rename("missing", "name").is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn most_recent_uses_write_order_even_when_timestamps_tie() {
    let directory = tempfile::tempdir().unwrap();
    let store = SessionStore::open(&directory.path().join("app.sqlite3")).unwrap();
    let mut first = store.create(snapshot(), state()).unwrap();
    let second = store.create(snapshot(), state()).unwrap();
    assert_eq!(store.list().unwrap()[0].id, second.id);
    store.save(&mut first).unwrap();
    // Deliberately tie wall-clock timestamps; recency uses transactional activity order.
    store
        .connection
        .execute("UPDATE sessions SET updated_at=1", [])
        .unwrap();
    assert_eq!(store.list().unwrap()[0].id, first.id);
    store.rename(&second.id, "most recent").unwrap();
    store
        .connection
        .execute("UPDATE sessions SET updated_at=1", [])
        .unwrap();
    assert_eq!(store.list().unwrap()[0].id, second.id);
}
