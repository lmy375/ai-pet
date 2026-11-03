//! Task-oriented multi-agent swarms.
//!
//! Group chat and swarms intentionally have different runtimes. A group reacts
//! to a shared transcript; a swarm owns a persistent task queue and workers
//! claim tasks atomically. Both still hand their turns to the shared
//! `chat::run_agent` facade, so prompt assembly, history normalization and
//! streaming are the same code path. This module keeps the queue, lifecycle and
//! worker conversations together so the GUI can observe it through a small
//! event trait.

use std::collections::HashMap;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::chat::{run_agent, AgentRun, ChatEventSink, ChatOutcome, StreamEvent, UserTurn};
use crate::config::AiConfig;
use crate::logging::{LogSession, LogStore};
use crate::mcp::McpStore;
use crate::item::{ChatItem, ItemBuilder};
use crate::registry::{LiveSession, SessionRegistry, SessionState};
use crate::session::Conversation;
use crate::system_prompt::SystemPrompt;
use crate::shell::ShellStore;
use crate::tools::{RunKind, ToolContext, ToolStores};

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

/// Upper bound on one project's task board, shared by the runtime and the
/// `swarm_create_task` tool so the limit is enforced in one place.
const MAX_TASKS: usize = 10_000;

/// Partial project settings from `swarm_update_config`: `None` keeps the
/// current value. The Tauri command replaces the whole config, so it fills
/// every field.
#[derive(Default)]
pub(crate) struct SwarmConfigPatch {
    pub title: Option<String>,
    pub main_agent_id: Option<String>,
    pub pool: Option<Vec<WorkerProfile>>,
    pub shared_prompt: Option<String>,
    pub orchestrator_prompt: Option<String>,
    pub worker_prompt: Option<String>,
}

impl SwarmConfigPatch {
    fn apply(self, config: &mut SwarmConfig) {
        if let Some(title) = self.title {
            config.title = title;
        }
        if let Some(agent_id) = self.main_agent_id {
            config.main_agent_id = agent_id;
        }
        if let Some(pool) = self.pool {
            config.pool = pool;
        }
        if let Some(prompt) = self.shared_prompt {
            config.shared_prompt = prompt;
        }
        if let Some(prompt) = self.orchestrator_prompt {
            config.orchestrator_prompt = prompt;
        }
        if let Some(prompt) = self.worker_prompt {
            config.worker_prompt = prompt;
        }
    }
}

impl From<SwarmConfig> for SwarmConfigPatch {
    fn from(config: SwarmConfig) -> Self {
        Self {
            title: Some(config.title),
            main_agent_id: Some(config.main_agent_id),
            pool: Some(config.pool),
            shared_prompt: Some(config.shared_prompt),
            orchestrator_prompt: Some(config.orchestrator_prompt),
            worker_prompt: Some(config.worker_prompt),
        }
    }
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

/// The `running` → `done`/`failed` lifecycle of one worker run, serialized as
/// the plain strings the frontend compares (`"running"`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmWorkerRecord {
    /// The per-run id (`<worker slot>-<task prefix>`) the UI keys views by.
    pub id: String,
    /// The worker slot (`<agent_id>-<ordinal>`); the same slot reuses its record
    /// so `workers` stays bounded by the pool size, not the task count.
    #[serde(default)]
    pub worker_id: String,
    pub agent_id: String,
    pub task_id: Option<String>,
    pub status: WorkerStatus,
    #[serde(default)]
    pub items: Vec<ChatItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmSnapshot {
    pub id: String,
    /// Project settings, flattened so disk JSON keeps `title`/`pool`/… at the
    /// top level next to the runtime state.
    #[serde(flatten)]
    pub config: SwarmConfig,
    pub status: SwarmStatus,
    pub tasks: Vec<SwarmTask>,
    pub main: Conversation,
    pub workers: Vec<SwarmWorkerRecord>,
    pub created_at: String,
    pub updated_at: String,
}

/// One observable change of a swarm. A single method keeps the event contract
/// in this module; each frontend (currently only the GUI) maps the variants to
/// its own transport.
pub enum SwarmEvent<'a> {
    Snapshot(&'a SwarmSnapshot),
    MainStream {
        swarm_id: &'a str,
        event: &'a StreamEvent,
    },
    MainDone {
        swarm_id: &'a str,
    },
    WorkerStream {
        swarm_id: &'a str,
        worker_id: &'a str,
        event: &'a StreamEvent,
    },
    WorkerStatus {
        swarm_id: &'a str,
        worker_id: &'a str,
        status: &'a str,
    },
}

pub trait SwarmEvents: Send + Sync {
    fn event(&self, event: SwarmEvent<'_>);
}

struct LiveWorker {
    cancel: CancellationToken,
}

pub struct SwarmSession {
    pub state: Mutex<SwarmSnapshot>,
    workers: Mutex<HashMap<String, LiveWorker>>,
    pub main_cancel: Mutex<Option<CancellationToken>>,
    pub events: Arc<dyn SwarmEvents>,
    /// The project's file, written atomically by [`LiveSession::persist`].
    path: PathBuf,
    /// Set when the swarm is deleted. In-flight Workers and the Orchestrator
    /// still hold this session, so every persistence path checks it before
    /// writing a snapshot back to disk.
    deleted: AtomicBool,
}

impl LiveSession<SwarmSnapshot> for SwarmSession {
    fn deleted(&self) -> &AtomicBool {
        &self.deleted
    }

    fn path(&self) -> &Path {
        &self.path
    }

    async fn cancel_all(&self) {
        for worker in self.workers.lock().await.values() {
            worker.cancel.cancel();
        }
        if let Some(cancel) = self.main_cancel.lock().await.take() {
            cancel.cancel();
        }
    }
}

pub struct SwarmRuntime {
    registry: SessionRegistry<SwarmSnapshot, SwarmSession>,
    mcp_store: McpStore,
    log_store: LogStore,
    shell_store: ShellStore,
    rt: tokio::runtime::Handle,
}

/// A process restart cannot keep old in-memory workers alive. Recover to
/// stopped so reopening the page never spends tokens unexpectedly.
impl SessionState for SwarmSnapshot {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn read(path: &Path) -> Result<Self, String> {
        let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let text = crate::store::read_text(path)?.ok_or_else(|| format!("Unknown swarm {id}"))?;
        serde_json::from_str(&text).map_err(|_| format!("Unknown swarm {id}"))
    }

    fn recover(&mut self) -> bool {
        if !matches!(self.status, SwarmStatus::Running | SwarmStatus::Draining) {
            return false;
        }
        self.status = SwarmStatus::Stopped;
        self.workers.clear();
        for task in &mut self.tasks {
            if task.status == SwarmTaskStatus::Running {
                task.status = SwarmTaskStatus::Pending;
                task.claimed_by = None;
                task.outcome = Some("worker interrupted by application restart".to_string());
            }
        }
        self.updated_at = now();
        true
    }

    fn not_found(id: &str) -> String {
        format!("Unknown swarm {id}")
    }

    fn delete_error(err: std::io::Error) -> String {
        format!("删除项目失败：{err}")
    }
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
        let registry = SessionRegistry::new(crate::store::data_dir("swarms"), move |path, state| {
            Arc::new(SwarmSession {
                state: Mutex::new(state),
                workers: Mutex::new(HashMap::new()),
                main_cancel: Mutex::new(None),
                events: events.clone(),
                path,
                deleted: AtomicBool::new(false),
            })
        });
        Arc::new(Self {
            registry,
            mcp_store,
            log_store,
            shell_store,
            rt,
        })
    }

    async fn session(&self, id: &str) -> Result<Arc<SwarmSession>, String> {
        self.registry.get(id).await
    }

    pub async fn list(&self) -> Vec<SwarmSnapshot> {
        let mut out = self.registry.list();
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        out
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
            config: SwarmConfig {
                title: title.to_string(),
                main_agent_id,
                pool: Vec::new(),
                shared_prompt: String::new(),
                orchestrator_prompt: String::new(),
                worker_prompt: String::new(),
            },
            status: SwarmStatus::Idle,
            tasks: Vec::new(),
            main: Conversation::default(),
            workers: Vec::new(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
        };
        self.registry.create(snapshot.clone()).await?;
        Ok(snapshot)
    }

    pub async fn load(self: &Arc<Self>, id: &str) -> Result<SwarmSnapshot, String> {
        Ok(self.session(id).await?.state.lock().await.clone())
    }

    /// Delete a swarm: cancel anything still running, drop it from memory, and
    /// remove its snapshot file. The tombstone keeps still-running Workers from
    /// reloading the file we are about to remove.
    pub async fn delete(self: &Arc<Self>, id: &str) -> Result<(), String> {
        // Keep the old contract: deleting an unknown swarm is an error, while
        // deleting an existing one is idempotent once its file is gone.
        self.registry.get(id).await?;
        self.registry.delete(id).await
    }

    /// Replace the whole project config, as the Tauri command does. The
    /// `swarm_update_config` tool goes through
    /// [`SwarmSession::update_config`] with a partial patch instead.
    pub async fn update_config(
        self: &Arc<Self>,
        id: &str,
        config: SwarmConfig,
    ) -> Result<(), String> {
        self.session(id).await?.update_config(config.into()).await?;
        Ok(())
    }

    pub async fn create_task(
        self: &Arc<Self>,
        id: &str,
        title: String,
        detail: String,
        parent_task_id: Option<String>,
    ) -> Result<SwarmTask, String> {
        self.session(id)
            .await?
            .create_task(title, detail, parent_task_id)
            .await
    }

    pub async fn delete_task(self: &Arc<Self>, id: &str, task_id: &str) -> Result<(), String> {
        self.session(id).await?.delete_task(task_id).await
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
        session.persist(&state);
        Ok(Some(copy))
    }

    pub async fn complete_task(
        self: &Arc<Self>,
        id: &str,
        task_id: &str,
        worker_id: &str,
        result: String,
        outcome: String,
    ) -> Result<(), String> {
        self.session(id)
            .await?
            .complete_task(task_id, worker_id, result, outcome)
            .await
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
            session.persist(&state);
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
            if state.config.pool.iter().map(|p| p.count).sum::<u32>() == 0 {
                return Err("请先配置至少一个 Worker".to_string());
            }
            if session.is_deleted() {
                return Err(format!("Unknown swarm {id}"));
            }
            state.status = SwarmStatus::Running;
            state.updated_at = now();
            session.persist(&state);
        }
        let profiles = session.state.lock().await.config.pool.clone();
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
            session.persist(&state);
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
        session.persist(&state);
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
            .push(ChatItem::user(&turn.text, &turn.images));
        let prior = state.main.messages.clone();
        state.updated_at = now();
        session.persist(&state);
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
                session.persist(&current);
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
        session.persist(&state);
    }
}

/// Where a prepared run sits inside a swarm: a worker slot claims tasks, the
/// orchestrator answers the owner. Everything else is shared.
#[derive(Clone, Copy)]
enum RunRole<'a> {
    Worker { worker_id: &'a str, run_id: &'a str },
    Orchestrator,
}

/// The system block a prepared run fronts. It owns the strings copied out of
/// the snapshot so the borrowed `SystemPrompt` can be built on demand.
enum PreparedPrompt {
    Worker { project: String, shared: String, role: String },
    Orchestrator { project: String, shared: String, role: String },
}

/// The preamble a worker task and an orchestrator turn share: the agent config,
/// the conversation, the ToolContext and the sink. Callers only add the parts
/// that really differ.
struct PreparedRun {
    config: AiConfig,
    conv: Vec<Value>,
    prompt: PreparedPrompt,
    ctx: ToolContext,
    sink: SwarmSink,
}

impl PreparedRun {
    fn prompt(&self) -> SystemPrompt<'_> {
        match &self.prompt {
            PreparedPrompt::Worker { project, shared, role } => SystemPrompt::SwarmWorker {
                project,
                shared,
                role,
            },
            PreparedPrompt::Orchestrator { project, shared, role } => {
                SystemPrompt::SwarmOrchestrator {
                    agent_id: &self.config.agent_id,
                    project,
                    shared,
                    role,
                }
            }
        }
    }

    async fn run(&mut self, runtime: &SwarmRuntime, turn: UserTurn) -> Result<ChatOutcome, String> {
        let conv = std::mem::take(&mut self.conv);
        let prompt = self.prompt();
        run_agent(AgentRun {
            conv,
            turn: Some(turn),
            prompt,
            sink: &self.sink,
            config: &self.config,
            mcp_store: &runtime.mcp_store,
            ctx: &self.ctx,
            max_turns: None,
        })
        .await
    }
}

fn worker_run_id(worker_id: &str, task_id: &str) -> String {
    format!(
        "{}-{}",
        worker_id,
        task_id.chars().take(8).collect::<String>()
    )
}

/// Resolve the agent's config and build the parts every swarm run needs.
/// Returns `None` when the agent is not configured; the caller decides how to
/// report that (a failed task vs. a finished orchestrator turn).
async fn prepare(
    runtime: &Arc<SwarmRuntime>,
    session: &Arc<SwarmSession>,
    agent_id: &str,
    conv: Vec<Value>,
    role: RunRole<'_>,
    cancel: CancellationToken,
) -> Option<PreparedRun> {
    let (id, title, shared, role_prompt) = {
        let state = session.state.lock().await;
        let role_prompt = match role {
            RunRole::Worker { .. } => &state.config.worker_prompt,
            RunRole::Orchestrator => &state.config.orchestrator_prompt,
        };
        (
            state.id.clone(),
            state.config.title.clone(),
            state.config.shared_prompt.clone(),
            role_prompt.clone(),
        )
    };
    let config = crate::settings::get_settings()
        .ok()
        .and_then(|s| s.agent(agent_id).cloned())
        .and_then(|a| AiConfig::from_agent(&a).ok())?;
    let (session_id, log_session, sink, prompt) = match role {
        RunRole::Worker { worker_id, run_id } => (
            format!("swarm:{id}:{worker_id}"),
            LogSession::swarm_worker(&id, worker_id),
            SwarmSink::worker(session.events.clone(), id.clone(), run_id.to_string()),
            PreparedPrompt::Worker {
                project: title,
                shared,
                role: role_prompt,
            },
        ),
        RunRole::Orchestrator => (
            format!("swarm-main:{id}"),
            LogSession::swarm_main(&id),
            SwarmSink::main(session.events.clone(), id.clone()),
            PreparedPrompt::Orchestrator {
                project: title,
                shared,
                role: role_prompt,
            },
        ),
    };
    let (kind, depth) = match role {
        RunRole::Worker { .. } => (RunKind::SwarmWorker, 1),
        RunRole::Orchestrator => (RunKind::SwarmMain, 0),
    };
    let ctx = ToolContext::for_session(
        ToolStores::new(
            runtime.log_store.clone(),
            runtime.shell_store.clone(),
            runtime.mcp_store.clone(),
        ),
        config.clone(),
        session_id,
    )
    .with_kind(kind)
    .with_depth(depth)
    .with_swarm(Some(session.clone()))
    .with_log_session(log_session)
    .with_cancel(cancel);
    Some(PreparedRun {
        config,
        conv,
        prompt,
        ctx,
        sink,
    })
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
    let id = session.state.lock().await.id.clone();
    let run_id = worker_run_id(worker_id, &task.id);
    let worker_record = SwarmWorkerRecord {
        id: run_id.clone(),
        worker_id: worker_id.to_string(),
        agent_id: profile.agent_id.clone(),
        task_id: Some(task.id.clone()),
        status: WorkerStatus::Running,
        items: Vec::new(),
    };
    session.events.event(SwarmEvent::WorkerStatus {
        swarm_id: &id,
        worker_id: &run_id,
        status: "running",
    });
    {
        let mut state = session.state.lock().await;
        if session.is_deleted() {
            return;
        }
        state.workers.retain(|w| w.worker_id != worker_id);
        state.workers.push(worker_record);
        session.persist(&state);
    }
    let Some(mut prepared) = prepare(
        runtime,
        session,
        &profile.agent_id,
        Vec::new(),
        RunRole::Worker {
            worker_id,
            run_id: &run_id,
        },
        cancel.clone(),
    )
    .await
    else {
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
    };
    prepared.ctx.swarm_task_id = Some(task.id.clone());
    prepared.ctx.swarm_worker_id = Some(worker_id.to_string());
    let turn = UserTurn::text(format!("任务标题：{}\n\n任务详情：{}", task.title, task.detail));
    let result = prepared.run(runtime, turn).await;
    if session.is_deleted() {
        return;
    }
    if cancel.is_cancelled() {
        runtime
            .requeue_task(&id, &task.id, worker_id, "任务被用户停止，等待重新执行")
            .await;
    } else {
        let final_text = result.as_ref().map(|o| o.text.clone()).unwrap_or_else(|e| e.clone());
        let outcome = if result.is_ok() { "success" } else { "failed" };
        let _ = runtime
            .complete_task(&id, &task.id, worker_id, final_text, outcome.to_string())
            .await;
    }
    let items = prepared.sink.take_items();
    let mut state = session.state.lock().await;
    if session.is_deleted() {
        return;
    }
    if let Some(worker) = state.workers.iter_mut().find(|w| w.id == run_id) {
        worker.status = WorkerStatus::Done;
        worker.items.extend(items);
    }
    session.persist(&state);
    session.events.event(SwarmEvent::WorkerStatus {
        swarm_id: &id,
        worker_id: &run_id,
        status: "done",
    });
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
    let (id, agent_id) = {
        let state = session.state.lock().await;
        (state.id.clone(), state.config.main_agent_id.clone())
    };
    let Some(mut prepared) = prepare(
        &runtime,
        &session,
        &agent_id,
        prior,
        RunRole::Orchestrator,
        cancel,
    )
    .await
    else {
        session.events.event(SwarmEvent::MainDone { swarm_id: &id });
        *session.main_cancel.lock().await = None;
        return;
    };
    let result = prepared.run(&runtime, turn).await;
    if session.is_deleted() {
        *session.main_cancel.lock().await = None;
        return;
    }
    let items = prepared.sink.take_items();
    let mut state = session.state.lock().await;
    state.main.items.extend(items);
    if let Ok(outcome) = result {
        state.main.messages = outcome.messages;
    }
    state.updated_at = now();
    session.persist(&state);
    *session.main_cancel.lock().await = None;
    session.events.event(SwarmEvent::MainDone { swarm_id: &id });
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
    fn route(&self, event: StreamEvent) {
        let event = match &self.worker_id {
            Some(worker_id) => SwarmEvent::WorkerStream {
                swarm_id: &self.swarm_id,
                worker_id,
                event: &event,
            },
            None => SwarmEvent::MainStream {
                swarm_id: &self.swarm_id,
                event: &event,
            },
        };
        self.events.event(event);
    }
    fn take_items(&self) -> Vec<ChatItem> {
        self.items.lock().unwrap().take_items()
    }
}

impl ChatEventSink for SwarmSink {
    fn items(&self) -> Option<&std::sync::Mutex<ItemBuilder>> {
        Some(&self.items)
    }

    fn emit(&self, event: StreamEvent) {
        self.route(event);
    }
}

impl SwarmSession {
    fn is_deleted(&self) -> bool {
        self.deleted.load(Ordering::SeqCst)
    }

    /// Every task mutation lives here so the runtime and the swarm tools share
    /// one implementation: the tool layer only parses arguments and resolves
    /// which worker is calling. Each method takes the lock once.
    pub(crate) async fn create_task(
        &self,
        title: String,
        detail: String,
        parent_task_id: Option<String>,
    ) -> Result<SwarmTask, String> {
        let mut state = self.state.lock().await;
        if state.tasks.len() >= MAX_TASKS {
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
        self.persist(&state);
        Ok(task)
    }

    pub(crate) async fn update_task(
        &self,
        task_id: &str,
        worker_id: &str,
        progress: u8,
        summary: String,
    ) -> Result<(), String> {
        let mut state = self.state.lock().await;
        let task = state
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| "任务不存在".to_string())?;
        if task.claimed_by.as_deref() != Some(worker_id) || task.status != SwarmTaskStatus::Running
        {
            return Err("Worker 没有该任务的锁".to_string());
        }
        task.progress = progress;
        task.progress_summary = summary;
        task.updated_at = now();
        state.updated_at = now();
        self.persist(&state);
        Ok(())
    }

    /// A task completed while the project is stopping goes back to Pending so
    /// a restart re-runs it; only a Running task owned by the worker may be
    /// marked Completed.
    pub(crate) async fn complete_task(
        &self,
        task_id: &str,
        worker_id: &str,
        result: String,
        outcome: String,
    ) -> Result<(), String> {
        let mut state = self.state.lock().await;
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
            self.persist(&state);
            return Ok(());
        }
        if task.claimed_by.as_deref() != Some(worker_id) || task.status != SwarmTaskStatus::Running
        {
            return Err("Worker 没有该任务的锁".to_string());
        }
        task.status = SwarmTaskStatus::Completed;
        task.progress = 100;
        task.result = Some(result);
        task.outcome = Some(outcome);
        task.updated_at = now();
        state.updated_at = now();
        self.persist(&state);
        Ok(())
    }

    pub(crate) async fn delete_task(&self, task_id: &str) -> Result<(), String> {
        let mut state = self.state.lock().await;
        let Some(index) = state.tasks.iter().position(|task| task.id == task_id) else {
            return Err("任务不存在".to_string());
        };
        state.tasks.remove(index);
        state.updated_at = now();
        self.persist(&state);
        Ok(())
    }

    pub(crate) async fn update_config(
        &self,
        patch: SwarmConfigPatch,
    ) -> Result<SwarmConfig, String> {
        let mut state = self.state.lock().await;
        if matches!(state.status, SwarmStatus::Running | SwarmStatus::Draining) {
            return Err("项目运行中不能修改设置".to_string());
        }
        let mut config = state.config.clone();
        patch.apply(&mut config);
        config.title = config.title.trim().to_string();
        if config.title.is_empty() {
            return Err("项目名称不能为空".to_string());
        }
        state.config = config;
        state.updated_at = now();
        self.persist(&state);
        Ok(state.config.clone())
    }

    /// Write one snapshot to disk and tell the UI, unless the swarm was deleted
    /// while this update was in flight. All state writers (runtime methods and
    /// the swarm tools) go through here, so a deleted swarm cannot be recreated
    /// by a late Worker or tool call.
    pub fn persist(&self, state: &SwarmSnapshot) {
        if self.is_deleted() {
            return;
        }
        // Persist the full snapshot; the model-facing `main.messages` may hold
        // base64 images and is never rendered by the UI, so strip it from the
        // copy sent over IPC.
        LiveSession::persist(self, state);
        let mut broadcast = state.clone();
        broadcast.main.messages.clear();
        self.events.event(SwarmEvent::Snapshot(&broadcast));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A snapshot exactly as the previous struct wrote it: the project settings
    /// at the top level and `status` as a plain string. One worker record omits
    /// `worker_id`, as files written before that field existed do.
    const DISK: &str = r#"{
        "id": "5622094b-5bfa-4548-b793-d60266b74ee4",
        "title": "代码审计",
        "main_agent_id": "owen",
        "pool": [{ "agent_id": "owen", "count": 6 }],
        "shared_prompt": "shared",
        "orchestrator_prompt": "orchestrator",
        "worker_prompt": "worker",
        "status": "completed",
        "tasks": [],
        "main": { "messages": [], "items": [] },
        "workers": [
            { "id": "owen-1-ad07d4bc", "worker_id": "owen-1", "agent_id": "owen", "task_id": null, "status": "running", "items": [] },
            { "id": "owen-2-ad07d4bc", "worker_id": "owen-2", "agent_id": "owen", "task_id": null, "status": "done", "items": [] },
            { "id": "owen-3-ad07d4bc", "agent_id": "owen", "task_id": null, "status": "failed", "items": [] }
        ],
        "created_at": "2026-10-19T17:40:41.506",
        "updated_at": "2026-11-02T16:35:13.995"
    }"#;

    fn disk_snapshot() -> SwarmSnapshot {
        serde_json::from_str(DISK).expect("old swarm files must parse")
    }

    #[test]
    fn disk_format_swarm_json_keeps_config_fields_at_top_level() {
        let s = disk_snapshot();
        assert_eq!(s.config.title, "代码审计");
        assert_eq!(s.config.main_agent_id, "owen");
        assert_eq!(s.config.pool[0].count, 6);
        assert_eq!(s.config.shared_prompt, "shared");
        assert_eq!(s.status, SwarmStatus::Completed);
        assert_eq!(s.main.messages.len(), 0);
        assert_eq!(s.workers[2].worker_id, "", "missing worker_id defaults to empty");

        // Flatten keeps the settings at the top level instead of under `config`.
        let written = serde_json::to_value(&s).unwrap();
        assert_eq!(written["title"], "代码审计");
        assert_eq!(written["main_agent_id"], "owen");
        assert_eq!(written["worker_prompt"], "worker");
        assert!(written.get("config").is_none());
    }

    #[test]
    fn worker_status_round_trips_as_the_strings_the_ui_compares() {
        let s = disk_snapshot();
        assert_eq!(s.workers[0].status, WorkerStatus::Running);
        assert_eq!(s.workers[1].status, WorkerStatus::Done);
        assert_eq!(s.workers[2].status, WorkerStatus::Failed);

        let written = serde_json::to_value(&s).unwrap();
        assert_eq!(written["workers"][0]["status"], "running");
        assert_eq!(written["workers"][1]["status"], "done");
        assert_eq!(written["workers"][2]["status"], "failed");
    }

    struct NoopEvents;

    impl SwarmEvents for NoopEvents {
        fn event(&self, _: SwarmEvent<'_>) {}
    }

    #[derive(Default)]
    struct RecordingEvents {
        snapshots: std::sync::Mutex<Vec<SwarmSnapshot>>,
    }

    impl SwarmEvents for RecordingEvents {
        fn event(&self, event: SwarmEvent<'_>) {
            if let SwarmEvent::Snapshot(state) = event {
                self.snapshots.lock().unwrap().push(state.clone());
            }
        }
    }

    fn session_with(status: SwarmStatus) -> Arc<SwarmSession> {
        session_with_events(status, Arc::new(NoopEvents))
    }

    fn session_with_events(status: SwarmStatus, events: Arc<dyn SwarmEvents>) -> Arc<SwarmSession> {
        let task = SwarmTask {
            id: "t1".to_string(),
            title: "task".to_string(),
            detail: "detail".to_string(),
            status: SwarmTaskStatus::Running,
            progress: 0,
            progress_summary: String::new(),
            claimed_by: Some("owen-1".to_string()),
            parent_task_id: None,
            result: None,
            outcome: None,
            attempt: 1,
            created_at: "2026-01-01T00:00:00".to_string(),
            updated_at: "2026-01-01T00:00:00".to_string(),
        };
        let snapshot = SwarmSnapshot {
            id: "test-swarm".to_string(),
            config: SwarmConfig {
                title: "test".to_string(),
                main_agent_id: "owen".to_string(),
                pool: Vec::new(),
                shared_prompt: String::new(),
                orchestrator_prompt: String::new(),
                worker_prompt: String::new(),
            },
            status,
            tasks: vec![task],
            main: Conversation::default(),
            workers: Vec::new(),
            created_at: "2026-01-01T00:00:00".to_string(),
            updated_at: "2026-01-01T00:00:00".to_string(),
        };
        Arc::new(SwarmSession {
            state: Mutex::new(snapshot),
            workers: Mutex::new(HashMap::new()),
            main_cancel: Mutex::new(None),
            events,
            path: std::env::temp_dir()
                .join(format!("pet-swarm-test-{}.json", uuid::Uuid::new_v4())),
            deleted: AtomicBool::new(false),
        })
    }

    #[tokio::test]
    async fn snapshot_broadcast_drops_main_messages_but_the_file_keeps_them() {
        let events = Arc::new(RecordingEvents::default());
        let session = session_with_events(SwarmStatus::Running, events.clone());
        let mut state = session.state.lock().await.clone();
        state.main.messages = vec![serde_json::json!({ "role": "user", "content": "hi" })];

        session.persist(&state);

        let captured = events.snapshots.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert!(
            captured[0].main.messages.is_empty(),
            "the IPC copy must drop model-facing messages"
        );

        let written: SwarmSnapshot =
            serde_json::from_str(&std::fs::read_to_string(&session.path).unwrap()).unwrap();
        assert_eq!(
            written.main.messages.len(),
            1,
            "the file keeps the full conversation"
        );
    }

    #[tokio::test]
    async fn completing_a_task_after_stop_requeues_it() {
        let session = session_with(SwarmStatus::Stopped);
        session
            .complete_task("t1", "owen-1", "done".to_string(), "success".to_string())
            .await
            .unwrap();
        let state = session.state.lock().await;
        let task = state.tasks.iter().find(|t| t.id == "t1").unwrap();
        assert_eq!(task.status, SwarmTaskStatus::Pending);
        assert_eq!(task.claimed_by, None);
        assert_eq!(
            task.outcome.as_deref(),
            Some("任务被用户停止，等待重新执行")
        );
        assert!(
            task.result.is_none(),
            "a stopped task must not record a result"
        );
        assert_ne!(state.updated_at, "2026-01-01T00:00:00");
    }

    #[tokio::test]
    async fn updating_progress_refreshes_the_project_timestamp() {
        let session = session_with(SwarmStatus::Running);
        session
            .update_task("t1", "owen-1", 40, "half".to_string())
            .await
            .unwrap();
        let state = session.state.lock().await;
        let task = state.tasks.iter().find(|t| t.id == "t1").unwrap();
        assert_eq!(task.progress, 40);
        assert_eq!(task.progress_summary, "half");
        assert_ne!(state.updated_at, "2026-01-01T00:00:00");
        assert_ne!(task.updated_at, "2026-01-01T00:00:00");
    }
}
