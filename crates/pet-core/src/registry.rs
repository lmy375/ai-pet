//! The session registry shared by the group and swarm runtimes.
//!
//! Both runtimes own a set of sessions backed by one JSON file each: lazy load
//! on first use, a tombstone that stops a deleted session from being reloaded or
//! written back, and a cancel sweep on delete. Only that machinery lives here;
//! each runtime keeps its own session struct, events and coordination model.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::Mutex;

/// The persisted state of one session.
pub trait SessionState: Serialize + DeserializeOwned + Send + 'static {
    fn id(&self) -> String;
    /// Read one session file. Each runtime keeps its own error wording.
    fn read(path: &Path) -> Result<Self, String>;
    /// Repair state left behind by a process restart, returning whether it
    /// changed (and therefore needs writing back).
    fn recover(&mut self) -> bool {
        false
    }
    /// Error for an id that is absent or tombstoned.
    fn not_found(id: &str) -> String;
    /// Error for a failed file deletion.
    fn delete_error(err: std::io::Error) -> String;
}

/// One live session: the tombstone and file path the registry needs, plus the
/// cancel sweep `delete` runs.
pub trait LiveSession<S: SessionState>: Send + Sync {
    fn deleted(&self) -> &AtomicBool;
    fn path(&self) -> &Path;
    /// Cancel everything this session is currently running.
    fn cancel_all(&self) -> impl Future<Output = ()> + Send;

    /// Atomic write unless the session was deleted while the update was in
    /// flight, so a late worker can never resurrect its file.
    fn persist(&self, state: &S) {
        if self.deleted().load(Ordering::SeqCst) {
            return;
        }
        // `persist` is called from inside runtime state locks where a `Result`
        // has nowhere to go; report the failure instead of losing it silently.
        if let Err(e) = crate::store::write_json(self.path(), state) {
            eprintln!("{e}");
        }
    }
}

/// Owns every session of one runtime: lazy loading, tombstones and atomic
/// persistence. `build` constructs a live session from its loaded state.
pub struct SessionRegistry<S: SessionState, L: LiveSession<S>> {
    dir: Result<PathBuf, String>,
    sessions: Mutex<HashMap<String, Arc<L>>>,
    deleted: Mutex<HashSet<String>>,
    build: Box<dyn Fn(PathBuf, S) -> Arc<L> + Send + Sync>,
}

impl<S: SessionState, L: LiveSession<S>> SessionRegistry<S, L> {
    pub fn new(
        dir: Result<PathBuf, String>,
        build: impl Fn(PathBuf, S) -> Arc<L> + Send + Sync + 'static,
    ) -> Self {
        Self {
            dir,
            sessions: Mutex::new(HashMap::new()),
            deleted: Mutex::new(HashSet::new()),
            build: Box::new(build),
        }
    }

    /// Session ids are generated UUIDs. Refuse anything else so a client-supplied
    /// id can never escape the session directory.
    fn path(&self, id: &str) -> Result<PathBuf, String> {
        let dir = self.dir.as_ref().map_err(|e| e.clone())?;
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(S::not_found(id));
        }
        Ok(dir.join(format!("{id}.json")))
    }

    /// Load on first use; a tombstoned id is never reloaded.
    pub async fn get(&self, id: &str) -> Result<Arc<L>, String> {
        let mut sessions = self.sessions.lock().await;
        // Same lock order as `delete` (sessions → deleted), so a concurrent
        // delete can't load a fresh session while its file is being removed.
        if self.deleted.lock().await.contains(id) {
            return Err(S::not_found(id));
        }
        if let Some(session) = sessions.get(id) {
            return Ok(session.clone());
        }
        let path = self.path(id)?;
        let mut state = S::read(&path)?;
        if state.recover() {
            crate::store::write_json(&path, &state)?;
        }
        let session = (self.build)(path, state);
        sessions.insert(id.to_string(), session.clone());
        Ok(session)
    }

    /// Every session on disk, with restart recovery applied. Unsorted — the
    /// caller applies its own view/order.
    pub fn list(&self) -> Vec<S> {
        let Ok(dir) = &self.dir else {
            return Vec::new();
        };
        let Ok(entries) = fs::read_dir(dir) else {
            eprintln!("Failed to list {}", dir.display());
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    return None;
                }
                // An unreadable record is reported and skipped rather than hiding the
                // whole list: the other sessions stay usable.
                let mut state = match S::read(&path) {
                    Ok(state) => state,
                    Err(e) => {
                        eprintln!("{e}");
                        return None;
                    }
                };
                if state.recover() {
                    if let Err(e) = crate::store::write_json(&path, &state) {
                        eprintln!("{e}");
                    }
                }
                Some(state)
            })
            .collect()
    }

    /// Write a new session file and make it live immediately.
    pub async fn create(&self, state: S) -> Result<Arc<L>, String> {
        let id = state.id();
        let path = self.path(&id)?;
        crate::store::write_json(&path, &state)?;
        let session = (self.build)(path, state);
        self.sessions.lock().await.insert(id, session.clone());
        Ok(session)
    }

    /// Cancel everything the session runs, tombstone it, then remove its file.
    /// A failed removal rolls the tombstone back and keeps serving the session.
    pub async fn delete(&self, id: &str) -> Result<(), String> {
        let path = self.path(id)?;
        let mut sessions = self.sessions.lock().await;
        let removed = sessions.remove(id);
        self.deleted.lock().await.insert(id.to_string());
        if let Some(session) = &removed {
            session.deleted().store(true, Ordering::SeqCst);
            session.cancel_all().await;
        }
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => {
                if let Some(session) = removed {
                    session.deleted().store(false, Ordering::SeqCst);
                    sessions.insert(id.to_string(), session);
                }
                self.deleted.lock().await.remove(id);
                Err(S::delete_error(e))
            }
        }
    }

    /// Persist `state` for a live session (tombstone check + atomic write).
    pub fn persist(&self, live: &L, state: &S) {
        live.persist(state);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Fake {
        id: String,
        n: u32,
    }

    impl SessionState for Fake {
        fn id(&self) -> String {
            self.id.clone()
        }

        fn read(path: &Path) -> Result<Self, String> {
            let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
            serde_json::from_str(&text).map_err(|e| e.to_string())
        }

        fn not_found(id: &str) -> String {
            format!("missing {id}")
        }

        fn delete_error(err: std::io::Error) -> String {
            err.to_string()
        }
    }

    struct FakeLive {
        state: Mutex<Fake>,
        deleted: AtomicBool,
        path: PathBuf,
    }

    impl LiveSession<Fake> for FakeLive {
        fn deleted(&self) -> &AtomicBool {
            &self.deleted
        }

        fn path(&self) -> &Path {
            &self.path
        }

        async fn cancel_all(&self) {}
    }

    fn registry(dir: &Path) -> SessionRegistry<Fake, FakeLive> {
        let dir = dir.to_path_buf();
        SessionRegistry::new(Ok(dir.clone()), move |path, state| {
            Arc::new(FakeLive {
                state: Mutex::new(state),
                deleted: AtomicBool::new(false),
                path,
            })
        })
    }

    #[tokio::test]
    async fn create_get_and_delete_respect_the_tombstone() {
        let dir = std::env::temp_dir().join(format!("pet-registry-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let registry = registry(&dir);

        let created = registry
            .create(Fake {
                id: "s1".into(),
                n: 1,
            })
            .await
            .unwrap();
        assert_eq!(created.state.lock().await.n, 1);

        // A second get returns the same live session; a new update reaches disk.
        let same = registry.get("s1").await.unwrap();
        assert!(Arc::ptr_eq(&created, &same));
        registry.persist(&same, &Fake { id: "s1".into(), n: 2 });
        assert_eq!(Fake::read(&dir.join("s1.json")).unwrap().n, 2);

        // Delete tombstones, cancels and removes the file; the held Arc can no
        // longer write it back.
        registry.delete("s1").await.unwrap();
        assert!(registry.get("s1").await.is_err());
        registry.persist(&created, &Fake { id: "s1".into(), n: 3 });
        assert!(!dir.join("s1.json").exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
