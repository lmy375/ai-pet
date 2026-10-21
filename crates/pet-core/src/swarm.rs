//! Task-oriented multi-agent swarms.
//!
//! Group chat and swarms intentionally have different runtimes. A group reacts
//! to a shared transcript; a swarm owns a persistent task queue and workers
//! claim tasks atomically. This module keeps the queue, lifecycle and worker
//! conversations together so the GUI can observe it through a small event
//! trait.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::chat::{run_agent_loop, ChatEventSink, ItemBuilder, StreamEvent, UserTurn};
use crate::config::AiConfig;
use crate::logging::{LogSession, LogStore};
use crate::mcp::McpStore;
use crate::prompt;
use crate::shell::ShellStore;
use crate::tools::ToolContext;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SwarmStatus {
    Idle,
    Running,
    Draining,
    Stopped,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SwarmTaskStatus {
    Pending,
    Running,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerProfile {
    pub agent_id: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmConfig {
    pub title: String,
    pub main_agent_id: String,
    pub pool: Vec<WorkerProfile>,
    #[serde(default)]
    pub shared_prompt: String,
    #[serde(default)]
    pub orchestrator_prompt: String,
    #[serde(default)]
    pub worker_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmTask {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub status: SwarmTaskStatus,
    pub progress: u8,
    #[serde(default)]
    pub progress_summary: String,
    #[serde(default)]
    pub claimed_by: Option<String>,
    #[serde(default)]
    pub parent_task_id: Option<String>,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub outcome: Option<String>,
    pub attempt: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SwarmConversation {
    #[serde(default)]
    pub messages: Vec<Value>,
    #[serde(default)]
    pub items: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmWorkerRecord {
    pub id: String,
    pub agent_id: String,
    pub task_id: Option<String>,
    pub status: String,
    #[serde(default)]
    pub items: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmSnapshot {
    pub id: String,
    pub title: String,
    pub main_agent_id: String,
    pub pool: Vec<WorkerProfile>,
    #[serde(default)]
    pub shared_prompt: String,
    #[serde(default)]
    pub orchestrator_prompt: String,
    #[serde(default)]
    pub worker_prompt: String,
    pub status: SwarmStatus,
    pub tasks: Vec<SwarmTask>,
    pub main: SwarmConversation,
    pub workers: Vec<SwarmWorkerRecord>,
    pub created_at: String,
    pub updated_at: String,
}

pub trait SwarmEvents: Send + Sync {
    fn snapshot(&self, state: &SwarmSnapshot);
    fn main_stream(&self, swarm_id: &str, event: &StreamEvent);
    fn main_done(&self, swarm_id: &str);
    fn worker_stream(&self, swarm_id: &str, worker_id: &str, event: &StreamEvent);
    fn worker_status(&self, swarm_id: &str, worker_id: &str, status: &str);
}

struct LiveWorker {
    cancel: CancellationToken,
}

pub struct SwarmSession {
    pub state: Mutex<SwarmSnapshot>,
    workers: Mutex<HashMap<String, LiveWorker>>,
    pub main_cancel: Mutex<Option<CancellationToken>>,
    pub events: Arc<dyn SwarmEvents>,
    pub mcp_store: McpStore,
    pub log_store: LogStore,
    pub shell_store: ShellStore,
    /// Set when the swarm is deleted. In-flight Workers and the Orchestrator
    /// still hold this session, so every persistence path checks it before
    /// writing a snapshot back to disk.
    deleted: AtomicBool,
}

pub struct SwarmRuntime {
    sessions: Mutex<HashMap<String, Arc<SwarmSession>>>,
    /// Ids whose snapshot file was deleted. `session()` refuses to reload them,
    /// so a Worker that is still winding down cannot resurrect a deleted swarm.
    deleted: Mutex<HashSet<String>>,
    events: Arc<dyn SwarmEvents>,
    mcp_store: McpStore,
    log_store: LogStore,
    shell_store: ShellStore,
    rt: tokio::runtime::Handle,
}

fn swarms_dir() -> Result<PathBuf, String> {
    let dir = crate::common::config_dir()?.join("swarms");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create swarms dir: {e}"))?;
    Ok(dir)
}

fn swarm_path(id: &str) -> Result<PathBuf, String> {
    Ok(swarms_dir()?.join(format!("{id}.json")))
}

fn save_snapshot(snapshot: &SwarmSnapshot) {
    let Ok(path) = swarm_path(&snapshot.id) else {
        return;
    };
    let Ok(text) = serde_json::to_string_pretty(snapshot) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(tmp, path);
    }
}

fn load_snapshot(id: &str) -> Option<SwarmSnapshot> {
    let path = swarm_path(id).ok()?;
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Write one snapshot to disk and tell the UI, unless the swarm was deleted
/// while this update was in flight. All state writers go through here so a
/// deleted swarm cannot be recreated by a late Worker or tool call.
fn persist_state(session: &SwarmSession, snapshot: &SwarmSnapshot) {
    if session.is_deleted() {
        return;
    }
    save_snapshot(snapshot);
    session.events.snapshot(snapshot);
}

pub async fn persist_session(session: &Arc<SwarmSession>, snapshot: SwarmSnapshot) {
    persist_state(session, &snapshot);
}

fn list_snapshots() -> Vec<SwarmSnapshot> {
    let Ok(dir) = swarms_dir() else {
        return Vec::new();
    };
    let mut out = fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|text| serde_json::from_str::<SwarmSnapshot>(&text).ok())
        .collect::<Vec<_>>();
    for snapshot in &mut out {
        if matches!(snapshot.status, SwarmStatus::Running | SwarmStatus::Draining) {
            snapshot.status = SwarmStatus::Stopped;
            snapshot.workers.clear();
            for task in &mut snapshot.tasks {
                if task.status == SwarmTaskStatus::Running {
                    task.status = SwarmTaskStatus::Pending;
                    task.claimed_by = None;
                    task.outcome = Some("worker interrupted by application restart".to_string());
                }
            }
            snapshot.updated_at = now();
            save_snapshot(snapshot);
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}

fn now() -> String {
    crate::common::iso_now()
}

impl SwarmRuntime {
    pub fn new(
        events: Arc<dyn SwarmEvents>,
        mcp_store: McpStore,
        log_store: LogStore,
        shell_store: ShellStore,
        rt: tokio::runtime::Handle,
    ) -> Arc<Self> {
        Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            deleted: Mutex::new(HashSet::new()),
            events,
            mcp_store,
            log_store,
            shell_store,
            rt,
        })
    }

    async fn session(self: &Arc<Self>, id: &str) -> Result<Arc<SwarmSession>, String> {
        if self.deleted.lock().await.contains(id) {
            return Err(format!("Unknown swarm {id}"));
        }
        if let Some(s) = self.sessions.lock().await.get(id).cloned() {
            return Ok(s);
        }
        let mut snapshot = load_snapshot(id).ok_or_else(|| format!("Unknown swarm {id}"))?;
        // A process restart cannot keep old in-memory workers alive. Recover to
        // stopped so reopening the page never spends tokens unexpectedly.
        if matches!(
            snapshot.status,
            SwarmStatus::Running | SwarmStatus::Draining
        ) {
            snapshot.status = SwarmStatus::Stopped;
            snapshot.workers.clear();
            for task in &mut snapshot.tasks {
                if task.status == SwarmTaskStatus::Running {
                    task.status = SwarmTaskStatus::Pending;
                    task.claimed_by = None;
                    task.outcome = Some("worker interrupted by application restart".to_string());
                }
            }
            snapshot.updated_at = now();
            save_snapshot(&snapshot);
        }
        let session = Arc::new(SwarmSession {
            state: Mutex::new(snapshot),
            workers: Mutex::new(HashMap::new()),
            main_cancel: Mutex::new(None),
            events: self.events.clone(),
            mcp_store: self.mcp_store.clone(),
            log_store: self.log_store.clone(),
            shell_store: self.shell_store.clone(),
            deleted: AtomicBool::new(false),
        });
        self.sessions
            .lock()
            .await
            .insert(id.to_string(), session.clone());
        Ok(session)
    }

    pub async fn list(&self) -> Vec<SwarmSnapshot> {
        list_snapshots()
    }

    pub async fn create(
        self: &Arc<Self>,
        title: String,
        main_agent_id: String,
    ) -> Result<SwarmSnapshot, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = now();
        let title = title.trim();
        if title.is_empty() {
            return Err("项目名称不能为空".to_string());
        }
        let snapshot = SwarmSnapshot {
            id: id.clone(),
            title: title.to_string(),
            main_agent_id,
            pool: Vec::new(),
            shared_prompt: String::new(),
            orchestrator_prompt: String::new(),
            worker_prompt: String::new(),
            status: SwarmStatus::Idle,
            tasks: Vec::new(),
            main: SwarmConversation::default(),
            workers: Vec::new(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
        };
        save_snapshot(&snapshot);
        let session = Arc::new(SwarmSession {
            state: Mutex::new(snapshot.clone()),
            workers: Mutex::new(HashMap::new()),
            main_cancel: Mutex::new(None),
            events: self.events.clone(),
            mcp_store: self.mcp_store.clone(),
            log_store: self.log_store.clone(),
            shell_store: self.shell_store.clone(),
            deleted: AtomicBool::new(false),
        });
        self.sessions.lock().await.insert(id, session);
        Ok(snapshot)
    }

    pub async fn load(self: &Arc<Self>, id: &str) -> Result<SwarmSnapshot, String> {
        Ok(self.session(id).await?.state.lock().await.clone())
    }

    /// Delete a swarm: cancel anything still running, drop it from memory, and
    /// remove its snapshot file. The tombstone keeps still-running Workers from
    /// reloading the file we are about to remove.
    pub async fn delete(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let session = self.session(id).await?;
        let path = swarm_path(id)?;
        self.deleted.lock().await.insert(id.to_string());
        session.deleted.store(true, Ordering::SeqCst);
        for worker in session.workers.lock().await.values() {
            worker.cancel.cancel();
        }
        if let Some(cancel) = session.main_cancel.lock().await.take() {
            cancel.cancel();
        }
        self.sessions.lock().await.remove(id);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => {
                // Keep the project usable instead of leaving a file that no
                // longer has a matching in-memory session.
                self.deleted.lock().await.remove(id);
                session.deleted.store(false, Ordering::SeqCst);
                self.sessions.lock().await.insert(id.to_string(), session);
                Err(format!("删除项目失败：{e}"))
            }
        }
    }

    pub async fn update_config(
        self: &Arc<Self>,
        id: &str,
        config: SwarmConfig,
    ) -> Result<(), String> {
        let title = config.title.trim();
        if title.is_empty() {
            return Err("项目名称不能为空".to_string());
        }
        let session = self.session(id).await?;
        let mut state = session.state.lock().await;
        if matches!(state.status, SwarmStatus::Running | SwarmStatus::Draining) {
            return Err("项目运行中不能修改设置".to_string());
        }
        state.title = title.to_string();
        state.main_agent_id = config.main_agent_id;
        state.pool = config.pool;
        state.shared_prompt = config.shared_prompt;
        state.orchestrator_prompt = config.orchestrator_prompt;
        state.worker_prompt = config.worker_prompt;
        state.updated_at = now();
        persist_state(&session, &state);
        Ok(())
    }

    pub async fn create_task(
        self: &Arc<Self>,
        id: &str,
        title: String,
        detail: String,
        parent_task_id: Option<String>,
    ) -> Result<SwarmTask, String> {
        let session = self.session(id).await?;
        let task = {
            let mut state = session.state.lock().await;
            if state.tasks.len() >= 10_000 {
                return Err("项目任务数量已达到 10000 上限".to_string());
            }
            let task = SwarmTask {
                id: uuid::Uuid::new_v4().to_string(),
                title: title.trim().to_string(),
                detail: detail.trim().to_string(),
                status: SwarmTaskStatus::Pending,
                progress: 0,
                progress_summary: String::new(),
                claimed_by: None,
                parent_task_id,
                result: None,
                outcome: None,
                attempt: 0,
                created_at: now(),
                updated_at: now(),
            };
            if task.title.is_empty() || task.detail.is_empty() {
                return Err("任务标题和详情不能为空".to_string());
            }
            state.tasks.push(task.clone());
            state.updated_at = now();
            persist_state(&session, &state);
            task
        };
        Ok(task)
    }

    pub async fn delete_task(self: &Arc<Self>, id: &str, task_id: &str) -> Result<(), String> {
        let session = self.session(id).await?;
        let mut state = session.state.lock().await;
        let Some(index) = state.tasks.iter().position(|task| task.id == task_id) else {
            return Err("任务不存在".to_string());
        };
        state.tasks.remove(index);
        state.updated_at = now();
        let snapshot = state.clone();
        drop(state);
        persist_session(&session, snapshot).await;
        Ok(())
    }

    pub async fn summary(self: &Arc<Self>, id: &str) -> Result<Value, String> {
        let state = self.session(id).await?.state.lock().await.clone();
        let pending = state
            .tasks
            .iter()
            .filter(|t| t.status == SwarmTaskStatus::Pending)
            .count();
        let running = state
            .tasks
            .iter()
            .filter(|t| t.status == SwarmTaskStatus::Running)
            .count();
        let completed = state
            .tasks
            .iter()
            .filter(|t| t.status == SwarmTaskStatus::Completed)
            .count();
        Ok(serde_json::json!({
            "status": state.status,
            "pending": pending,
            "running": running,
            "completed": completed,
            "total": state.tasks.len(),
        }))
    }

    pub async fn claim_task(
        self: &Arc<Self>,
        id: &str,
        worker_id: &str,
    ) -> Result<Option<SwarmTask>, String> {
        let session = self.session(id).await?;
        let mut state = session.state.lock().await;
        if state.status != SwarmStatus::Running {
            return Ok(None);
        }
        let task = state
            .tasks
            .iter_mut()
            .find(|task| task.status == SwarmTaskStatus::Pending);
        let Some(task) = task else { return Ok(None) };
        task.status = SwarmTaskStatus::Running;
        task.claimed_by = Some(worker_id.to_string());
        task.attempt += 1;
        task.updated_at = now();
        let copy = task.clone();
        state.updated_at = now();
        persist_state(&session, &state);
        Ok(Some(copy))
    }

    pub async fn update_task(
        self: &Arc<Self>,
        id: &str,
        task_id: &str,
        worker_id: &str,
        progress: u8,
        summary: String,
    ) -> Result<(), String> {
        let session = self.session(id).await?;
        let mut state = session.state.lock().await;
        let task = state
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| "任务不存在".to_string())?;
        if task.claimed_by.as_deref() != Some(worker_id) || task.status != SwarmTaskStatus::Running
        {
            return Err("Worker 没有该任务的锁".to_string());
        }
        task.progress = progress.min(100);
        task.progress_summary = summary;
        task.updated_at = now();
        state.updated_at = now();
        persist_state(&session, &state);
        Ok(())
    }

    pub async fn complete_task(
        self: &Arc<Self>,
        id: &str,
        task_id: &str,
        worker_id: &str,
        result: String,
        outcome: String,
    ) -> Result<(), String> {
        let session = self.session(id).await?;
        let mut state = session.state.lock().await;
        let stopped = state.status == SwarmStatus::Stopped;
        let task = state
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| "任务不存在".to_string())?;
        if stopped {
            task.status = SwarmTaskStatus::Pending;
            task.claimed_by = None;
            task.outcome = Some("任务被用户停止，等待重新执行".to_string());
            task.updated_at = now();
            state.updated_at = now();
            let snapshot = state.clone();
            drop(state);
            persist_session(&session, snapshot).await;
            return Ok(());
        }
        if task.claimed_by.as_deref() != Some(worker_id) || task.status != SwarmTaskStatus::Running {
            return Err("Worker 没有该任务的锁".to_string());
        }
        task.status = SwarmTaskStatus::Completed;
        task.progress = 100;
        task.result = Some(result);
        task.outcome = Some(outcome);
        task.updated_at = now();
        state.updated_at = now();
        persist_state(&session, &state);
        Ok(())
    }

    async fn requeue_task(self: &Arc<Self>, id: &str, task_id: &str, worker_id: &str, reason: &str) {
        let Ok(session) = self.session(id).await else { return };
        let mut state = session.state.lock().await;
        let Some(task) = state.tasks.iter_mut().find(|t| t.id == task_id) else { return };
        if task.status == SwarmTaskStatus::Running && task.claimed_by.as_deref() == Some(worker_id) {
            task.status = SwarmTaskStatus::Pending;
            task.claimed_by = None;
            task.outcome = Some(reason.to_string());
            task.updated_at = now();
            state.updated_at = now();
            let snapshot = state.clone();
            drop(state);
            persist_session(&session, snapshot).await;
        }
    }

    pub async fn start(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let session = self.session(id).await?;
        if session.is_deleted() {
            return Err(format!("Unknown swarm {id}"));
        }
        if !session.workers.lock().await.is_empty() {
            return Err("项目仍在停止当前 Worker，请稍后再开始".to_string());
        }
        {
            let mut state = session.state.lock().await;
            if matches!(state.status, SwarmStatus::Running | SwarmStatus::Draining) {
                return Err("项目已经在运行".to_string());
            }
            if state.pool.iter().map(|p| p.count).sum::<u32>() == 0 {
                return Err("请先配置至少一个 Worker".to_string());
            }
            if session.is_deleted() {
                return Err(format!("Unknown swarm {id}"));
            }
            state.status = SwarmStatus::Running;
            state.updated_at = now();
            persist_state(&session, &state);
        }
        let profiles = session.state.lock().await.pool.clone();
        for profile in profiles {
            for ordinal in 0..profile.count {
                let worker_id = format!("{}-{}", profile.agent_id, ordinal + 1);
                let token = CancellationToken::new();
                session.workers.lock().await.insert(
                    worker_id.clone(),
                    LiveWorker {
                        cancel: token.clone(),
                    },
                );
                let rt = self.clone();
                let s = session.clone();
                let p = profile.clone();
                self.rt
                    .spawn(async move { worker_loop(rt, s, p, worker_id, token).await });
            }
        }
        Ok(())
    }

    pub async fn end(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let session = self.session(id).await?;
        let mut state = session.state.lock().await;
        if state.status == SwarmStatus::Running {
            state.status = SwarmStatus::Draining;
            state.updated_at = now();
            persist_state(&session, &state);
        }
        Ok(())
    }

    pub async fn stop(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let session = self.session(id).await?;
        for worker in session.workers.lock().await.values() {
            worker.cancel.cancel();
        }
        if let Some(cancel) = session.main_cancel.lock().await.take() {
            cancel.cancel();
        }
        let mut state = session.state.lock().await;
        state.status = SwarmStatus::Stopped;
        for task in &mut state.tasks {
            if task.status == SwarmTaskStatus::Running {
                task.status = SwarmTaskStatus::Pending;
                task.claimed_by = None;
                task.outcome = Some("任务被用户停止，等待重新执行".to_string());
            }
        }
        state.updated_at = now();
        persist_state(&session, &state);
        Ok(())
    }

    pub async fn send_main(self: &Arc<Self>, id: &str, turn: UserTurn) -> Result<(), String> {
        let session = self.session(id).await?;
        if session.is_deleted() {
            return Err(format!("Unknown swarm {id}"));
        }
        let cancel = CancellationToken::new();
        {
            let mut current = session.main_cancel.lock().await;
            if current.is_some() {
                return Err("Orchestrator 正在回复".to_string());
            }
            *current = Some(cancel.clone());
        }
        let mut state = session.state.lock().await;
        state
            .main
            .items
            .push(crate::session::user_item(&turn.text, &turn.images));
        let prior = state.main.messages.clone();
        state.updated_at = now();
        persist_state(&session, &state);
        drop(state);

        let rt = self.clone();
        self.rt
            .spawn(async move { run_main(rt, session, prior, turn, cancel).await });
        Ok(())
    }

    pub async fn cancel_main(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let session = self.session(id).await?;
        if let Some(cancel) = session.main_cancel.lock().await.as_ref() {
            cancel.cancel();
        }
        Ok(())
    }
}

async fn worker_loop(
    runtime: Arc<SwarmRuntime>,
    session: Arc<SwarmSession>,
    profile: WorkerProfile,
    worker_id: String,
    cancel: CancellationToken,
) {
    loop {
        if cancel.is_cancelled() || session.is_deleted() {
            break;
        }
        let state = session.state.lock().await.clone();
        if state.status != SwarmStatus::Running {
            break;
        }
        let Some(task) = runtime
            .claim_task(&state.id, &worker_id)
            .await
            .ok()
            .flatten()
        else {
            if session.is_deleted() {
                break;
            }
            let mut current = session.state.lock().await;
            let has_pending = current.tasks.iter().any(|task| task.status == SwarmTaskStatus::Pending);
            let has_running = current.tasks.iter().any(|task| task.status == SwarmTaskStatus::Running);
            if current.status == SwarmStatus::Running && !has_pending && !has_running {
                if session.is_deleted() {
                    break;
                }
                current.status = SwarmStatus::Completed;
                current.updated_at = now();
                persist_state(&session, &current);
                break;
            }
            tokio::time::sleep(Duration::from_millis(700)).await;
            continue;
        };
        run_worker_task(
            &runtime,
            &session,
            &profile,
            &worker_id,
            &task,
            cancel.clone(),
        )
        .await;
        let state = session.state.lock().await.clone();
        if state.status != SwarmStatus::Running {
            break;
        }
    }
    session.workers.lock().await.remove(&worker_id);
    if session.is_deleted() {
        return;
    }
    let mut state = session.state.lock().await;
    let has_running = state
        .tasks
        .iter()
        .any(|t| t.status == SwarmTaskStatus::Running);
    let has_pending = state
        .tasks
        .iter()
        .any(|t| t.status == SwarmTaskStatus::Pending);
    if matches!(state.status, SwarmStatus::Draining) && !has_running {
        state.status = if has_pending {
            SwarmStatus::Stopped
        } else {
            SwarmStatus::Completed
        };
        state.updated_at = now();
        persist_state(&session, &state);
    }
}

async fn run_worker_task(
    runtime: &Arc<SwarmRuntime>,
    session: &Arc<SwarmSession>,
    profile: &WorkerProfile,
    worker_id: &str,
    task: &SwarmTask,
    cancel: CancellationToken,
) {
    if session.is_deleted() {
        return;
    }
    let (id, project_name, shared_prompt, worker_prompt) = {
        let state = session.state.lock().await;
        (
            state.id.clone(),
            state.title.clone(),
            state.shared_prompt.clone(),
            state.worker_prompt.clone(),
        )
    };
    let run_id = format!(
        "{}-{}",
        worker_id,
        task.id.chars().take(8).collect::<String>()
    );
    let worker_record = SwarmWorkerRecord {
        id: run_id.clone(),
        agent_id: profile.agent_id.clone(),
        task_id: Some(task.id.clone()),
        status: "running".to_string(),
        items: Vec::new(),
    };
    session.events.worker_status(&id, &run_id, "running");
    {
        let mut state = session.state.lock().await;
        if session.is_deleted() {
            return;
        }
        state.workers.retain(|w| w.id != worker_id);
        state.workers.push(worker_record);
        persist_state(session, &state);
    }
    let config = match crate::settings::get_settings()
        .ok()
        .and_then(|s| s.agent(&profile.agent_id).cloned())
        .and_then(|a| AiConfig::from_agent(&a).ok())
    {
        Some(config) => config,
        None => {
            let _ = runtime
                .complete_task(
                    &id,
                    &task.id,
                    worker_id,
                    "Worker Agent 配置不可用".to_string(),
                    "failed".to_string(),
                )
                .await;
            return;
        }
    };
    let mut conv = vec![crate::llm::store_message(&crate::llm::user_message(
        &format!(
            "项目：{}\n\n任务标题：{}\n\n任务详情：{}",
            project_name, task.title, task.detail
        ),
        &[],
    ))];
    prompt::prepend_swarm_worker_system_messages(
        &mut conv,
        &project_name,
        &shared_prompt,
        &worker_prompt,
    );
    let mut ctx = ToolContext::new(
        runtime.log_store.clone(),
        runtime.shell_store.clone(),
        config.clone(),
        runtime.mcp_store.clone(),
        format!("swarm:{}:{}", id, worker_id),
        None,
        None,
        false,
    );
    ctx.depth = 1;
    ctx.cancel = cancel.clone();
    ctx.swarm = Some(session.clone());
    ctx.swarm_task_id = Some(task.id.clone());
    ctx.swarm_worker_id = Some(worker_id.to_string());
    ctx.log_session = LogSession::swarm_worker(&id, worker_id);
    let sink = SwarmSink::worker(session.events.clone(), id.clone(), run_id.clone());
    let result = run_agent_loop(conv, &sink, &config, &runtime.mcp_store, &ctx).await;
    if session.is_deleted() {
        return;
    }
    if cancel.is_cancelled() {
        runtime.requeue_task(&id, &task.id, worker_id, "任务被用户停止，等待重新执行").await;
    } else {
        let final_text = result.as_ref().map(|(text, _)| text.clone()).unwrap_or_else(|e| e.clone());
        let outcome = if result.is_ok() { "success" } else { "failed" };
        let _ = runtime.complete_task(&id, &task.id, worker_id, final_text, outcome.to_string()).await;
    }
    let items = sink.take_items();
    let mut state = session.state.lock().await;
    if session.is_deleted() {
        return;
    }
    if let Some(worker) = state.workers.iter_mut().find(|w| w.id == run_id) {
        worker.status = "done".to_string();
        worker.items.extend(items);
    }
    persist_state(session, &state);
    session.events.worker_status(&id, &run_id, "done");
}

async fn run_main(
    runtime: Arc<SwarmRuntime>,
    session: Arc<SwarmSession>,
    prior: Vec<Value>,
    turn: UserTurn,
    cancel: CancellationToken,
) {
    if session.is_deleted() {
        *session.main_cancel.lock().await = None;
        return;
    }
    let (id, agent_id, project_name, shared_prompt, orchestrator_prompt) = {
        let state = session.state.lock().await;
        (
            state.id.clone(),
            state.main_agent_id.clone(),
            state.title.clone(),
            state.shared_prompt.clone(),
            state.orchestrator_prompt.clone(),
        )
    };
    let config = crate::settings::get_settings()
        .ok()
        .and_then(|s| s.agent(&agent_id).cloned())
        .and_then(|a| AiConfig::from_agent(&a).ok());
    let Some(config) = config else {
        session.events.main_done(&id);
        *session.main_cancel.lock().await = None;
        return;
    };
    let mut conv = prior;
    conv.push(crate::llm::store_message(&crate::llm::user_message(
        &turn.text,
        &turn.images,
    )));
    prompt::prepend_system_messages(&mut conv, &config.agent_id);
    prompt::prepend_swarm_orchestrator_system_messages(
        &mut conv,
        &project_name,
        &shared_prompt,
        &orchestrator_prompt,
    );
    let mut ctx = ToolContext::new(
        runtime.log_store.clone(),
        runtime.shell_store.clone(),
        config.clone(),
        runtime.mcp_store.clone(),
        format!("swarm-main:{id}"),
        None,
        None,
        false,
    );
    ctx.cancel = cancel;
    ctx.swarm = Some(session.clone());
    ctx.log_session = LogSession::swarm_main(&id);
    let sink = SwarmSink::main(session.events.clone(), id.clone());
    let result = run_agent_loop(conv, &sink, &config, &runtime.mcp_store, &ctx).await;
    if session.is_deleted() {
        *session.main_cancel.lock().await = None;
        return;
    }
    let items = sink.take_items();
    let mut state = session.state.lock().await;
    state.main.items.extend(items);
    if let Ok((_, messages)) = result {
        state.main.messages = messages
            .into_iter()
            .skip_while(crate::llm::is_system_message)
            .collect();
    }
    state.updated_at = now();
    persist_state(&session, &state);
    *session.main_cancel.lock().await = None;
    session.events.main_done(&id);
}

struct SwarmSink {
    events: Arc<dyn SwarmEvents>,
    swarm_id: String,
    worker_id: Option<String>,
    items: std::sync::Mutex<ItemBuilder>,
}

impl SwarmSink {
    fn main(events: Arc<dyn SwarmEvents>, swarm_id: String) -> Self {
        Self {
            events,
            swarm_id,
            worker_id: None,
            items: std::sync::Mutex::new(ItemBuilder::default()),
        }
    }
    fn worker(events: Arc<dyn SwarmEvents>, swarm_id: String, worker_id: String) -> Self {
        Self {
            events,
            swarm_id,
            worker_id: Some(worker_id),
            items: std::sync::Mutex::new(ItemBuilder::default()),
        }
    }
    fn emit(&self, event: StreamEvent) {
        if let Some(worker_id) = &self.worker_id {
            self.events.worker_stream(&self.swarm_id, worker_id, &event);
        } else {
            self.events.main_stream(&self.swarm_id, &event);
        }
    }
    fn take_items(&self) -> Vec<Value> {
        self.items.lock().unwrap().take_items()
    }
}

impl ChatEventSink for SwarmSink {
    fn send_chunk(&self, text: &str) {
        self.items.lock().unwrap().chunk(text);
        self.emit(StreamEvent::Chunk {
            text: text.to_string(),
        });
    }
    fn send_reasoning(&self, text: &str) {
        self.items.lock().unwrap().reasoning(text);
        self.emit(StreamEvent::Reasoning {
            text: text.to_string(),
        });
    }
    fn send_tool_start(&self, name: &str, arguments: &str) {
        self.items.lock().unwrap().tool_start(name, arguments);
        self.emit(StreamEvent::ToolStart {
            name: name.to_string(),
            arguments: arguments.to_string(),
        });
    }
    fn send_tool_result(&self, name: &str, result: &str) {
        self.items.lock().unwrap().tool_result(name, result);
        self.emit(StreamEvent::ToolResult {
            name: name.to_string(),
            result: result.to_string(),
        });
    }
    fn send_image(&self, data_url: &str) {
        self.items.lock().unwrap().image(data_url);
        self.emit(StreamEvent::Image {
            data_url: data_url.to_string(),
        });
    }
    fn send_usage(&self, prompt_tokens: u64, total_tokens: u64, context_window: u32) {
        self.emit(StreamEvent::Usage {
            prompt_tokens,
            total_tokens,
            context_window,
        });
    }
    fn send_done(&self) {
        self.items.lock().unwrap().done();
        self.emit(StreamEvent::Done {});
    }
    fn send_error(&self, message: &str) {
        self.items.lock().unwrap().error(message);
        self.emit(StreamEvent::Error {
            message: message.to_string(),
        });
    }
}

impl SwarmSession {
    fn is_deleted(&self) -> bool {
        self.deleted.load(Ordering::SeqCst)
    }

    pub async fn task_for_worker(
        &self,
        task_id: &str,
        worker_id: &str,
    ) -> Result<SwarmTask, String> {
        let state = self.state.lock().await;
        state
            .tasks
            .iter()
            .find(|t| t.id == task_id && t.claimed_by.as_deref() == Some(worker_id))
            .cloned()
            .ok_or_else(|| "任务不存在或不属于当前 Worker".to_string())
    }
}
