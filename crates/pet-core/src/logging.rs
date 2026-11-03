use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct LogStore(pub Arc<Mutex<Vec<String>>>);

/// Buffer backing [`LogStore::detached`]. Identified by pointer in `write_log`
/// so writers skip the in-memory tail and only append to `app.log`.
static DETACHED_BUFFER: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();

impl LogStore {
    /// A store that only appends to `app.log` and keeps no in-memory copy.
    /// For processes that never read the buffer back (the CLI).
    pub fn detached() -> Self {
        Self(DETACHED_BUFFER.get_or_init(|| Arc::new(Mutex::new(Vec::new()))).clone())
    }
}

/// Return the log directory: `<config dir>/logs/`. Same root as `config.yaml`,
/// `sessions/` and `memory/` (see `common::config_dir`) — logs used to live at
/// `~/.config/pet/logs` instead, which meant the app's state was split across
/// two unrelated roots on macOS.
pub fn log_dir() -> PathBuf {
    crate::common::config_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("logs")
}

/// One directory per-conversation LLM logs live in, plus their shared index.
pub fn llm_log_dir() -> PathBuf {
    log_dir().join("llm-log")
}

fn llm_index_path() -> PathBuf {
    llm_log_dir().join("index.jsonl")
}

// ---------------------------------------------------------------------------
// Tunables (set once at startup from `AppSettings`, see `configure`)
// ---------------------------------------------------------------------------

const DEFAULT_APP_LOG_MAX_MB: u64 = 10;
const DEFAULT_LLM_LOG_KEEP: usize = 100;

static APP_LOG_MAX_BYTES: AtomicU64 = AtomicU64::new(DEFAULT_APP_LOG_MAX_MB * 1024 * 1024);
static LLM_LOG_KEEP_PER_KIND: AtomicUsize = AtomicUsize::new(DEFAULT_LLM_LOG_KEEP);

/// Apply the user's log limits. Called when the settings snapshot is (re)parsed
/// — startup, an external config edit — and after every save, so a change takes
/// effect without a restart. Never on a plain settings read.
pub fn configure(app_log_max_mb: u64, llm_log_keep_per_kind: usize) {
    APP_LOG_MAX_BYTES.store(app_log_max_mb.max(1) * 1024 * 1024, Ordering::Relaxed);
    LLM_LOG_KEEP_PER_KIND.store(llm_log_keep_per_kind.max(1), Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// app.log — a live tail, never read back by the app
// ---------------------------------------------------------------------------

/// Byte count of `app.log`, so the size cap costs no `stat` per line. `None`
/// until the first write reads the existing file's length.
static APP_LOG_BYTES: OnceLock<Mutex<Option<u64>>> = OnceLock::new();

/// Append a line to a file (create if missing). Errors are silently ignored.
/// The parent directory is NOT created here — this runs once per log line, and
/// both interfaces create `log_dir()` at startup (`write_llm_log` creates its
/// own subdirectory once per write instead).
pub fn append_to_file(path: &Path, line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", line);
    }
}

/// Append to `app.log`, truncating it once it passes the configured size.
///
/// There is deliberately no rollover file: nothing in the app ever reads
/// `app.log` back (the Debug window's "app log" tab renders the in-memory store
/// below), so it exists purely as a live tail for a human — only the newest
/// lines are worth keeping, and a second file would just double the footprint.
fn append_app_log(line: &str) {
    let path = log_dir().join("app.log");
    let mut guard = match APP_LOG_BYTES.get_or_init(|| Mutex::new(None)).lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    let mut size = match *guard {
        Some(n) => n,
        None => fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
    };
    let added = line.len() as u64 + 1;
    if size + added > APP_LOG_MAX_BYTES.load(Ordering::Relaxed) {
        let _ = fs::remove_file(&path);
        size = 0;
    }
    append_to_file(&path, line);
    *guard = Some(size + added);
}

/// Write one formatted log line to both the in-memory store and app.log.
pub fn write_log(store: &Arc<Mutex<Vec<String>>>, message: &str) {
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
    let line = format!("[{}] {}", ts, message);

    // In-memory, unless this is the detached store (never read back by CLI).
    if !DETACHED_BUFFER.get().is_some_and(|buffer| Arc::ptr_eq(buffer, store)) {
        let mut logs = store.lock().unwrap();
        logs.push(line.clone());
        if logs.len() > 500 {
            let drain = logs.len() - 500;
            logs.drain(0..drain);
        }
    }

    append_app_log(&line);
}

pub fn get_logs(store: &LogStore) -> Vec<String> {
    store.0.lock().unwrap().clone()
}

pub fn clear_logs(store: &LogStore) {
    store.0.lock().unwrap().clear();
}

// ---------------------------------------------------------------------------
// LLM logs — one body file per conversation + a shared metadata index
// ---------------------------------------------------------------------------

/// What kind of conversation produced an LLM log entry. Only used for display
/// and for picking the retention bucket — the body format is identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogKind {
    Chat,
    Sub,
    Group,
    Heartbeat,
    Swarm,
}

impl LogKind {
    /// Retention bucket. Sub-agent runs count against the chat quota: they are
    /// work done on behalf of a chat session, and only the metadata tells them
    /// apart.
    fn quota(self) -> &'static str {
        match self {
            LogKind::Chat | LogKind::Sub => "chat",
            LogKind::Group => "group",
            LogKind::Heartbeat => "heartbeat",
            LogKind::Swarm => "swarm",
        }
    }
}

/// Identifies the conversation an LLM request belongs to.
///
/// `id` is used verbatim as the body filename, which is safe precisely because
/// every constructor below supplies a UUID — a chat session's own id for a chat
/// (so successive turns overwrite one file, each request being a superset of the
/// last), and a freshly minted one for heartbeat / group / sub-agent runs, which
/// are independent conversations that must not evict or merge with each other.
#[derive(Debug, Clone)]
pub struct LogSession {
    pub id: String,
    pub kind: LogKind,
    /// Display tag: the agent id for a group run, the parent session id for a
    /// sub-agent, empty for a plain chat or heartbeat.
    pub label: String,
}

impl LogSession {
    pub fn chat(session_id: String) -> Self {
        Self { id: session_id, kind: LogKind::Chat, label: String::new() }
    }

    pub fn sub(parent_session_id: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind: LogKind::Sub,
            label: parent_session_id.to_string(),
        }
    }

    pub fn group(group_id: &str, agent_id: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind: LogKind::Group,
            label: format!("{group_id}:{agent_id}"),
        }
    }

    pub fn heartbeat() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind: LogKind::Heartbeat,
            label: String::new(),
        }
    }

    pub fn swarm_main(swarm_id: &str) -> Self {
        Self { id: uuid::Uuid::new_v4().to_string(), kind: LogKind::Swarm, label: format!("{swarm_id}:main") }
    }

    pub fn swarm_worker(swarm_id: &str, worker_id: &str) -> Self {
        Self { id: uuid::Uuid::new_v4().to_string(), kind: LogKind::Swarm, label: format!("{swarm_id}:{worker_id}") }
    }
}

/// Timing for one round of the tool-calling loop.
///
/// The body only stores the final round's messages, since each round's request
/// contains every earlier round's. Latency does not work that way, so this is
/// where the per-round numbers survive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundStat {
    pub round: usize,
    pub ttft_ms: Option<i64>,
    pub total_ms: i64,
    pub tools: Vec<String>,
}

/// One line of `index.jsonl` — everything the log list renders, and nothing
/// else. Small enough (a few hundred bytes) that the whole index loads in well
/// under a millisecond; the messages live in the body file, fetched on click.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMeta {
    pub id: String,
    pub kind: LogKind,
    pub label: String,
    pub model: String,
    pub request_time: String,
    pub rounds: Vec<RoundStat>,
    /// First line of the newest user message, for the collapsed row.
    pub preview: String,
}

/// Everything that goes into the log index for one round.
pub struct LlmLogMeta<'a> {
    pub session: &'a LogSession,
    pub model: &'a str,
    pub rounds: &'a [RoundStat],
    /// First line of the newest user message, for the collapsed row.
    pub preview: &'a str,
    pub request_time: &'a str,
    /// This round's response. Every round appends it to the one-line `llm.log`
    /// (the format the evals parse); the first and final rounds also store it
    /// in the body file.
    pub response_text: &'a str,
    pub reasoning: &'a str,
    pub tool_calls: &'a [serde_json::Value],
}

/// The full request/response body. Written only on the first and last rounds:
/// each round's request is a superset of the previous one, so the last body
/// already holds every message. The metadata lives only in `index.jsonl` (that
/// is where the UI reads it), so the body carries no second copy of it.
pub struct LlmLogBody<'a> {
    pub messages: &'a [serde_json::Value],
    pub first_token_time: Option<&'a str>,
    pub done_time: &'a str,
}

/// `session_id` in the one-line `llm.log` format the evals parse: a plain chat
/// keeps its session id verbatim, a sub-agent is `{parent}:sub:{id}`, and the
/// independent run types get their own prefix.
fn line_session_id(session: &LogSession) -> String {
    match session.kind {
        LogKind::Chat => session.id.clone(),
        LogKind::Sub => format!("{}:sub:{}", session.label, session.id),
        LogKind::Group => format!("group:{}", session.label),
        LogKind::Heartbeat => format!("heartbeat:{}", session.id),
        LogKind::Swarm => format!("swarm:{}", session.label),
    }
}

/// Write the atomically-replaceable body file (when `body` is present), append
/// this round's metadata to the index, and append this round's response to
/// `llm.log`.
///
/// Body first, index second: an index line always has a body to open, whereas
/// the reverse order would leave a row that 404s. A crash between the two only
/// leaves an orphan body, which the next compaction sweeps up.
pub fn write_llm_log(meta: LlmLogMeta<'_>, body: Option<LlmLogBody<'_>>) {
    let stored_meta = LlmMeta {
        id: meta.session.id.clone(),
        kind: meta.session.kind,
        label: meta.session.label.clone(),
        model: meta.model.to_string(),
        request_time: meta.request_time.to_string(),
        rounds: meta.rounds.to_vec(),
        preview: meta.preview.to_string(),
    };

    let dir = llm_log_dir();
    if fs::create_dir_all(&dir).is_err() {
        return;
    }

    if let Some(body) = body {
        let json = serde_json::json!({

            "first_token_time": body.first_token_time,
            "done_time": body.done_time,
            "messages": body.messages,
            "response": {
                "text": meta.response_text,
                // Chain-of-thought from a reasoning model; null for models without one
                // so the body doesn't carry an empty string per round.
                "reasoning": if meta.reasoning.is_empty() { serde_json::Value::Null } else { serde_json::json!(meta.reasoning) },
                "tool_calls": meta.tool_calls,
            }
        });
        let path = dir.join(format!("{}.json", meta.session.id));
        // Logs may swallow a write failure (see the module rule): losing one
        // body only breaks one Debug entry, and the next round rewrites it.
        if let Err(e) = crate::store::write_atomic(&path, json.to_string()) {
            eprintln!("Failed to write LLM log body {}: {e}", path.display());
        }
    }

    // The metadata's only serialization: the body no longer embeds a copy.
    if let Ok(line) = serde_json::to_string(&stored_meta) {
        append_to_file(&llm_index_path(), &line);
    }

    // One line per round for the evals (`pet-eval-common` / `eval-private` /
    // the two Docker harnesses): they read `session_id` (":sub:" marks a
    // sub-agent) and `response.tool_calls` (genai's `fn_name` / `fn_arguments`).
    let line = serde_json::json!({
        "session_id": line_session_id(meta.session),
        "round": meta.rounds.last().map(|r| r.round).unwrap_or(0),
        "request_time": meta.request_time,
        "response": {
            "text": meta.response_text,
            "reasoning": if meta.reasoning.is_empty() { serde_json::Value::Null } else { serde_json::json!(meta.reasoning) },
            "tool_calls": meta.tool_calls,
        }
    });
    append_to_file(&log_dir().join("llm.log"), &line.to_string());
    maybe_compact();
}

/// Parse `index.jsonl`, keeping only the newest line per conversation. Each
/// round appends a line rather than rewriting one, so that concurrent writers
/// (the CLI and the GUI, plus heartbeat / group / sub-agent runs inside one
/// process) only ever need an atomic append — deduping is the reader's job.
fn load_index() -> Vec<LlmMeta> {
    // Logs are best-effort: a missing (first run) or unreadable index is an
    // empty list, not an error the UI could act on.
    let content = match crate::store::read_text(&llm_index_path()) {
        Ok(Some(content)) => content,
        Ok(None) => return Vec::new(),
        Err(e) => {
            eprintln!("{e}");
            return Vec::new();
        }
    };
    let mut by_id: HashMap<String, LlmMeta> = HashMap::new();
    for line in content.lines() {
        if let Ok(meta) = serde_json::from_str::<LlmMeta>(line) {
            by_id.insert(meta.id.clone(), meta);
        }
    }
    let mut metas: Vec<LlmMeta> = by_id.into_values().collect();
    metas.sort_by(|a, b| b.request_time.cmp(&a.request_time));
    metas
}

/// The log list: one entry per conversation, newest first.
pub fn read_llm_index() -> Vec<LlmMeta> {
    load_index()
}

/// The full body of one conversation, or `None` once it has been compacted away.
pub fn read_llm_entry(id: &str) -> Option<serde_json::Value> {
    // `id` comes back from the index, but it reaches us as a command argument,
    // so refuse anything that could escape the log directory.
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return None;
    }
    let raw = match crate::store::read_text(&llm_log_dir().join(format!("{}.json", id))) {
        Ok(Some(raw)) => raw,
        Ok(None) => return None,
        Err(e) => {
            eprintln!("{e}");
            return None;
        }
    };
    serde_json::from_str(&raw).ok()
}

// ---------------------------------------------------------------------------
// Compaction
// ---------------------------------------------------------------------------

static LAST_COMPACT: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
const COMPACT_INTERVAL: Duration = Duration::from_secs(30);

/// Compact at most every 30s. Appending is the hot path and needs no lock;
/// rewriting the index does, so it runs rarely and off to the side.
fn maybe_compact() {
    let cell = LAST_COMPACT.get_or_init(|| Mutex::new(None));
    {
        let mut last = match cell.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        match *last {
            Some(t) if t.elapsed() < COMPACT_INTERVAL => return,
            _ => *last = Some(Instant::now()),
        }
    }
    compact_llm_logs();
}

/// The conversations that survive compaction: the newest `keep_per_kind` of
/// each retention bucket, given `metas` already sorted newest-first.
fn select_survivors(metas: Vec<LlmMeta>, keep_per_kind: usize) -> Vec<LlmMeta> {
    let mut per_kind: HashMap<&str, usize> = HashMap::new();
    let mut keep = Vec::new();
    for meta in metas {
        let slot = per_kind.entry(meta.kind.quota()).or_insert(0);
        if *slot >= keep_per_kind {
            continue;
        }
        *slot += 1;
        keep.push(meta);
    }
    keep
}

/// Enforce the per-kind retention limit and collapse the index.
///
/// Keeps the newest N conversations of each kind, deletes every body file that
/// is no longer indexed (which also sweeps up orphans from an interrupted
/// write), and rewrites `index.jsonl` with one line per survivor.
pub fn compact_llm_logs() {
    let keep = select_survivors(load_index(), LLM_LOG_KEEP_PER_KIND.load(Ordering::Relaxed));

    let dir = llm_log_dir();
    let keep_ids: HashSet<&str> = keep.iter().map(|m| m.id.as_str()).collect();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s,
                None => continue,
            };
            let doomed = match path.extension().and_then(|e| e.to_str()) {
                // A body file survives only while it is still indexed.
                Some("json") => !keep_ids.contains(stem),
                // Leftovers from an interrupted atomic write.
                Some("tmp") => true,
                // index.jsonl, and anything a human dropped in here.
                _ => false,
            };
            if doomed {
                let _ = fs::remove_file(&path);
            }
        }
    }

    let body: String = keep
        .iter()
        .filter_map(|m| serde_json::to_string(m).ok())
        .map(|l| l + "\n")
        .collect();
    if let Err(e) = crate::store::write_atomic(&llm_index_path(), body) {
        eprintln!("Failed to rewrite LLM log index: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(id: &str, kind: LogKind) -> LlmMeta {
        LlmMeta {
            id: id.to_string(),
            kind,
            label: String::new(),
            model: "m".to_string(),
            request_time: String::new(),
            rounds: Vec::new(),
            preview: String::new(),
        }
    }

    fn ids(metas: &[LlmMeta]) -> Vec<&str> {
        metas.iter().map(|m| m.id.as_str()).collect()
    }

    #[test]
    fn evals_line_marks_sub_agent_sessions() {
        // The evals count a round only when `session_id` has no ":sub:", so the
        // sub-agent form must keep the parent id and the marker.
        assert_eq!(line_session_id(&LogSession::chat("s1".to_string())), "s1");
        let sub = LogSession::sub("s1");
        assert!(line_session_id(&sub).starts_with("s1:sub:"));
    }

    #[test]
    fn each_kind_gets_its_own_quota() {
        // A burst of heartbeats must not evict chat logs: the buckets are
        // independent, which is the whole reason the kind is recorded.
        let metas = vec![
            meta("h1", LogKind::Heartbeat),
            meta("h2", LogKind::Heartbeat),
            meta("h3", LogKind::Heartbeat),
            meta("c1", LogKind::Chat),
            meta("g1", LogKind::Group),
        ];
        let kept = select_survivors(metas, 2);
        assert_eq!(ids(&kept), vec!["h1", "h2", "c1", "g1"]);
    }

    #[test]
    fn sub_agent_runs_count_against_the_chat_quota() {
        // Sub-agents are chat work; only the metadata tells them apart, so they
        // share the bucket rather than getting one of their own.
        let metas = vec![
            meta("s1", LogKind::Sub),
            meta("c1", LogKind::Chat),
            meta("s2", LogKind::Sub),
        ];
        let kept = select_survivors(metas, 2);
        assert_eq!(ids(&kept), vec!["s1", "c1"]);
    }

}
