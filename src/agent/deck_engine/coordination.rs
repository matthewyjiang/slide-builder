//! Share transaction locks and generations across engine handles for the same deck.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{atomic::AtomicU64, Arc, Mutex, OnceLock, Weak},
};
use tokio::sync::Mutex as AsyncMutex;

#[derive(Default)]
struct Handles {
    lock: Arc<AsyncMutex<()>>,
    generation: Arc<AtomicU64>,
}
#[derive(Default)]
struct WeakHandles {
    lock: Weak<AsyncMutex<()>>,
    generation: Weak<AtomicU64>,
}

pub(super) fn handles(path: &Path) -> (Arc<AsyncMutex<()>>, Arc<AtomicU64>) {
    static REGISTRY: OnceLock<Mutex<HashMap<PathBuf, WeakHandles>>> = OnceLock::new();
    let mut registry = REGISTRY
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    registry.retain(|_, handles| handles.lock.strong_count() > 0);
    let existing = registry.entry(path.to_owned()).or_default();
    if let (Some(lock), Some(generation)) = (existing.lock.upgrade(), existing.generation.upgrade())
    {
        return (lock, generation);
    }
    let handles = Handles::default();
    *existing = WeakHandles {
        lock: Arc::downgrade(&handles.lock),
        generation: Arc::downgrade(&handles.generation),
    };
    (handles.lock, handles.generation)
}
