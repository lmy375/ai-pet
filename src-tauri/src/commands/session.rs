use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

fn sessions_dir() -> Result<PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or_else(|| "Cannot determine config directory".to_string())?
        .join("pet")
        .join("sessions");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create sessions dir: {e}"))?;
    Ok(dir)
}

fn index_path() -> Result<PathBuf, String> {
    Ok(sessions_dir()?.join("index.json"))
}

fn session_path(id: &str) -> Result<PathBuf, String> {
    Ok(sessions_dir()?.join(format!("{id}.json")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    /// R93: 会话内可见 chat item 总数（user / assistant / tool / error；
    /// 不含 system message）。serde default 让旧 index.json 不带此字段时
    /// 反序列化到 0；下次 save_session 自动填入实际值，迁移自然发生。
    #[serde(default)]
    pub item_count: usize,
    /// 钉住的会话永远排在列表前；让长期项目的会话不被新建会话淹没。
    /// 旧 index.json 没此字段 → false（默认）。
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionIndex {
    pub active_id: String,
    pub sessions: Vec<SessionMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<serde_json::Value>,
    pub items: Vec<serde_json::Value>,
}

fn read_index() -> SessionIndex {
    let path = match index_path() {
        Ok(p) => p,
        Err(_) => {
            return SessionIndex {
                active_id: String::new(),
                sessions: vec![],
            }
        }
    };
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or(SessionIndex {
            active_id: String::new(),
            sessions: vec![],
        }),
        Err(_) => SessionIndex {
            active_id: String::new(),
            sessions: vec![],
        },
    }
}

fn write_index(index: &SessionIndex) -> Result<(), String> {
    let path = index_path()?;
    let json = serde_json::to_string_pretty(index)
        .map_err(|e| format!("Failed to serialize index: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write index: {e}"))
}

fn now_iso() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%.3f")
        .to_string()
}

#[tauri::command]
pub fn list_sessions() -> SessionIndex {
    read_index()
}

#[tauri::command]
pub fn load_session(id: String) -> Result<Session, String> {
    let path = session_path(&id)?;
    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read session {id}: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse session {id}: {e}"))
}

#[tauri::command]
pub fn save_session(mut session: Session) -> Result<(), String> {
    let path = session_path(&session.id)?;

    // Preserve created_at from existing file if not provided
    if session.created_at.is_empty() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(existing) = serde_json::from_str::<Session>(&content) {
                session.created_at = existing.created_at;
            }
        }
    }

    // Write session file
    let json = serde_json::to_string_pretty(&session)
        .map_err(|e| format!("Failed to serialize session: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write session: {e}"))?;

    // Update index
    let mut index = read_index();
    index.active_id = session.id.clone();
    if let Some(meta) = index.sessions.iter_mut().find(|m| m.id == session.id) {
        // 只覆写实际变化的字段；pinned 状态由 set_session_pinned 单独管，
        // save_session 不可意外把它复位 false。
        meta.title = session.title.clone();
        meta.updated_at = session.updated_at.clone();
        meta.item_count = session.items.len();
    } else {
        index.sessions.push(SessionMeta {
            id: session.id.clone(),
            title: session.title.clone(),
            created_at: session.created_at.clone(),
            updated_at: session.updated_at.clone(),
            item_count: session.items.len(),
            pinned: false,
        });
    }
    write_index(&index)
}

/// 切换 session 的 pinned 状态。失败 → 找不到 id（边缘 race），返回 Err。
#[tauri::command]
pub fn set_session_pinned(id: String, pinned: bool) -> Result<(), String> {
    let mut index = read_index();
    let Some(meta) = index.sessions.iter_mut().find(|m| m.id == id) else {
        return Err(format!("session {id} not found"));
    };
    meta.pinned = pinned;
    write_index(&index)
}

#[tauri::command]
pub fn create_session() -> Result<Session, String> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();

    // Load current SOUL.md as system message
    let soul = super::settings::get_soul().unwrap_or_default();
    let system_msg = serde_json::json!({ "role": "system", "content": soul });

    let session = Session {
        id: id.clone(),
        title: "新会话".to_string(),
        created_at: now.clone(),
        updated_at: now,
        messages: vec![system_msg],
        items: vec![],
    };

    // Save session file
    save_session(session.clone())?;

    Ok(session)
}

/// pure：粗略估算 token 数（与前端 `estimateInputTokens` 同算法）：
/// CJK 字符 ~1 token/字，非 CJK 非空白字符 ~1 token/4 字。各家 LLM
/// tokenizer 都对不上，但 ±25% 误差对"该不该 /reset"的决策足够。
///
/// 实现走 `chars()` 迭代（O(n) Unicode-safe）；CJK 范围覆盖最常见的
/// Unified Ideographs + 假名 + 韩文音节。其它语种归 "non-CJK"。
pub fn estimate_tokens(s: &str) -> u32 {
    let mut cjk: u32 = 0;
    let mut other: u32 = 0;
    for ch in s.chars() {
        let code = ch as u32;
        let is_cjk = (0x4E00..=0x9FFF).contains(&code)
            || (0x3040..=0x30FF).contains(&code)
            || (0xAC00..=0xD7AF).contains(&code);
        if is_cjk {
            cjk = cjk.saturating_add(1);
        } else if !ch.is_whitespace() {
            other = other.saturating_add(1);
        }
    }
    // ceil(other / 4) = (other + 3) / 4。CJK + ceil(other/4) 总和。
    cjk.saturating_add((other.saturating_add(3)) / 4)
}

/// pure：从 OpenAI compatible content 字段（可能是 string 或 multipart
/// `[{type:"text", text:"..."}, {type:"image_url",...}]`）抽出文本片段
/// 拼成一个 string。multipart 中非 text 段（image_url）忽略。
fn content_value_text(content: &serde_json::Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        let mut out = String::new();
        for part in arr {
            if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(t);
            }
        }
        return out;
    }
    String::new()
}

/// 当前 active session 的 LLM 上下文规模 —— 给 PanelDebugStats 卡片做
/// "该不该 /reset" 决策支持。**排除 role=="system"** —— system 是 SOUL
/// 人设 / 工具说明，/reset 保留；本数字反映的是"会被 /reset 砍掉的部分"。
///
/// 任何读失败（索引读不到 / session 文件丢 / parse 失败）都退到 0 而非
/// Err，让 PanelDebugStats 渲染"干净状态（0 条）"而非 toast 报错 ——
/// 这卡片是辅助决策，挂掉不该挡用户其它操作。
#[derive(Debug, Clone, Serialize)]
pub struct SessionContextStats {
    pub messages: u32,
    pub chars: u32,
    pub tokens: u32,
    /// 当前 active session 的 id，给 UI 显"哪个 session"。空 = 没有 active
    /// session（极少 — 初次启动 create_session 前）。
    pub session_id: String,
    /// 同上的 title。
    pub session_title: String,
}

#[tauri::command]
pub fn get_active_session_context_stats() -> SessionContextStats {
    let idx = read_index();
    if idx.active_id.is_empty() {
        return SessionContextStats {
            messages: 0,
            chars: 0,
            tokens: 0,
            session_id: String::new(),
            session_title: String::new(),
        };
    }
    let session = match load_session(idx.active_id.clone()) {
        Ok(s) => s,
        Err(_) => {
            return SessionContextStats {
                messages: 0,
                chars: 0,
                tokens: 0,
                session_id: idx.active_id.clone(),
                session_title: String::new(),
            };
        }
    };
    let mut messages = 0u32;
    let mut chars = 0u32;
    let mut tokens = 0u32;
    for msg in &session.messages {
        let role = msg
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if role == "system" {
            continue;
        }
        messages = messages.saturating_add(1);
        let empty_val = serde_json::Value::Null;
        let content_val = msg.get("content").unwrap_or(&empty_val);
        let text = content_value_text(content_val);
        let c = text.chars().count() as u32;
        chars = chars.saturating_add(c);
        tokens = tokens.saturating_add(estimate_tokens(&text));
    }
    SessionContextStats {
        messages,
        chars,
        tokens,
        session_id: session.id,
        session_title: session.title,
    }
}

/// 跨会话搜索的命中条目。`item_index` 是 session.items 数组的下标 —— 前端
/// 把它和 session_id 一起拿来 scrollIntoView 跳到原会话原位。
///
/// `match_start` / `match_len` 都按 char 计算（中文友好），用来给前端做精确
/// 高亮 —— 用 byte 偏移在中文场景下会切错字。
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub session_id: String,
    pub session_title: String,
    pub session_updated_at: String,
    pub item_index: usize,
    pub role: String,
    pub snippet: String,
    pub match_start: usize,
    pub match_len: usize,
}

const SEARCH_DEFAULT_LIMIT: usize = 50;
const SEARCH_SNIPPET_CTX_CHARS: usize = 80;

/// `search_sessions` Tauri 命令。空 / 全空白关键字 → 空 vec（让前端能用
/// "输入框空"等价于"无搜索状态"）。结果按会话 `updated_at` 降序（同会话内
/// 按 item_index 升序），前端拿到的列表顶端就是最近会话的早 → 晚顺序。
///
/// R96: `session_id` Some 时只搜该会话，跳过其它会话；让"只搜当前会话"
/// 模式不被其它 session 命中吃满 limit（比前端 post-filter 准）。
#[tauri::command]
pub fn search_sessions(
    keyword: String,
    limit: Option<usize>,
    session_id: Option<String>,
) -> Vec<SearchHit> {
    let kw = keyword.trim();
    if kw.is_empty() {
        return vec![];
    }
    let cap = limit.unwrap_or(SEARCH_DEFAULT_LIMIT);
    if cap == 0 {
        return vec![];
    }
    let kw_lower = kw.to_lowercase();
    let kw_char_len = kw_lower.chars().count();

    // 索引按 updated_at 降序遍历 —— 最近会话的命中先填满 limit，相关性更高。
    let mut index = read_index();
    index
        .sessions
        .sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    let mut hits: Vec<SearchHit> = Vec::new();
    'outer: for meta in &index.sessions {
        if let Some(ref sid) = session_id {
            if &meta.id != sid {
                continue;
            }
        }
        let Ok(session) = load_session(meta.id.clone()) else {
            continue;
        };
        for hit in search_session_items(&session, &kw_lower, kw_char_len, SEARCH_SNIPPET_CTX_CHARS) {
            hits.push(hit);
            if hits.len() >= cap {
                break 'outer;
            }
        }
    }
    hits
}

/// Pure：把单个 session 里所有 user / assistant 命中关键字的 item 转成
/// `SearchHit`。`kw_lower` 须是已 to_lowercase 的关键字（外层只算一次）；
/// `kw_char_len` 是关键字的 char 数。
///
/// 同 item 多次出现 → 只取**首个匹配**（用户点击就跳到这行，多匹配在同行
/// 不需要拆多个 hit）。
pub(crate) fn search_session_items(
    session: &Session,
    kw_lower: &str,
    kw_char_len: usize,
    ctx_chars: usize,
) -> Vec<SearchHit> {
    let mut out = Vec::new();
    for (idx, item) in session.items.iter().enumerate() {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let Some(role) = obj.get("type").and_then(|v| v.as_str()) else {
            continue;
        };
        if role != "user" && role != "assistant" {
            continue;
        }
        let Some(content) = obj.get("content").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some((snippet, match_start_in_snippet)) =
            find_match_snippet(content, kw_lower, ctx_chars)
        else {
            continue;
        };
        out.push(SearchHit {
            session_id: session.id.clone(),
            session_title: session.title.clone(),
            session_updated_at: session.updated_at.clone(),
            item_index: idx,
            role: role.to_string(),
            snippet,
            match_start: match_start_in_snippet,
            match_len: kw_char_len,
        });
    }
    out
}

/// Pure：在 `content`（原始大小写）里找首个 `kw_lower`（已 lower）的 char-
/// 索引位置，并按 `ctx_chars` 各侧抽取 snippet。返回 `(snippet, match_start_
/// in_snippet)` 或 None（未命中）。
///
/// 边界：
/// - 命中位置左侧 ≤ ctx → 不加前导 `…`；右侧同理。
/// - 关键字本身保留**原始大小写**（不替换为关键字小写），让 snippet 显示
///   贴近原文，前端高亮按 match_start / match_len 切片即可。
pub(crate) fn find_match_snippet(
    content: &str,
    kw_lower: &str,
    ctx_chars: usize,
) -> Option<(String, usize)> {
    let chars: Vec<char> = content.chars().collect();
    let lower_chars: Vec<char> = content.to_lowercase().chars().collect();
    let kw_chars: Vec<char> = kw_lower.chars().collect();
    if kw_chars.is_empty() || kw_chars.len() > lower_chars.len() {
        return None;
    }
    // O(n*m) substring 找首位置；m 很小，content 通常 < 1KB → 远超优化阈值。
    let mut hit: Option<usize> = None;
    'outer: for i in 0..=lower_chars.len() - kw_chars.len() {
        for j in 0..kw_chars.len() {
            if lower_chars[i + j] != kw_chars[j] {
                continue 'outer;
            }
        }
        hit = Some(i);
        break;
    }
    let i = hit?;
    let start = i.saturating_sub(ctx_chars);
    let end = (i + kw_chars.len() + ctx_chars).min(chars.len());
    let mut snippet = String::new();
    let prefix_dots = start > 0;
    let suffix_dots = end < chars.len();
    if prefix_dots {
        snippet.push('…');
    }
    snippet.extend(&chars[start..end]);
    if suffix_dots {
        snippet.push('…');
    }
    let match_start_in_snippet = (i - start) + if prefix_dots { 1 } else { 0 };
    Some((snippet, match_start_in_snippet))
}

/// 扫所有 session 文件，返回"items 里至少一条 tool item 含 propose_task /
/// task_create 工具调用"的 session id 列表。与 list_sessions_with_images
/// 对偶，让 PanelChat dropdown 的"📋 含派单"过滤标记"工作场景"的 session。
#[tauri::command]
pub fn list_sessions_with_task_calls() -> Vec<String> {
    const TASK_TOOL_NAMES: &[&str] = &["propose_task", "task_create"];
    let index = read_index();
    let mut out = Vec::new();
    for meta in &index.sessions {
        let Ok(session) = load_session(meta.id.clone()) else {
            continue;
        };
        let has_task_call = session.items.iter().any(|item| {
            // 前端 ChatItem.toolCalls: ToolCall[]，每个有 name 字段。
            // type==="tool" 的 item 才会有 toolCalls；其它 type 没有这个键。
            item.get("toolCalls")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter().any(|tc| {
                        tc.get("name")
                            .and_then(|n| n.as_str())
                            .map(|name| TASK_TOOL_NAMES.contains(&name))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        });
        if has_task_call {
            out.push(meta.id.clone());
        }
    }
    out
}

/// 扫所有 session 文件，返回"items 里至少一条有非空 images 字段"的 session id
/// 列表。给 PanelChat dropdown 的"📷 含图片"过滤用 —— 全量 list 太大时筛掉
/// 纯文本 session。
///
/// 性能：load_session × N，JSON 解析在 session 通常 < 1KB 时单条 < 1ms；50
/// session 用 < 100ms。每次 toggle 重新算（让用户刚生图的会话立刻命中）；不缓
/// 存到 index.json 避免 schema migration 成本。
#[tauri::command]
pub fn list_sessions_with_images() -> Vec<String> {
    let index = read_index();
    let mut out = Vec::new();
    for meta in &index.sessions {
        let Ok(session) = load_session(meta.id.clone()) else {
            continue;
        };
        let has_image = session.items.iter().any(|item| {
            // items 是 Vec<serde_json::Value>，前端写的 ChatItem.images 是
            // string[]。判定"非空 images"即 has_image。后端 give_image 工具
            // 的 _attachments 不入 items —— 那条只在 tool 块里临时存活，不会
            // 持久化到 session.items。所以这里只看 ChatItem.images。
            item.get("images")
                .and_then(|v| v.as_array())
                .map(|arr| !arr.is_empty())
                .unwrap_or(false)
        });
        if has_image {
            out.push(meta.id.clone());
        }
    }
    out
}

/// 全部 sessions 打包成 base64(JSON) 快照，让用户换机时不必手动 cp 一堆
/// 文件。version 字段给 schema 演进留口子；import 路径会校验版本号。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionsSnapshot {
    /// schema 版本，当前固定 1。
    version: u32,
    /// 完整 session 索引（含 pinned / item_count 等 meta）。
    index: SessionIndex,
    /// 索引里每条 session 的完整内容。
    sessions: Vec<Session>,
}

#[tauri::command]
pub fn export_sessions_snapshot() -> Result<String, String> {
    use base64::Engine;
    let index = read_index();
    let mut sessions = Vec::with_capacity(index.sessions.len());
    for meta in &index.sessions {
        // load_session 失败时 skip 该条 session（snapshot 仍能复用其它）。
        if let Ok(s) = load_session(meta.id.clone()) {
            sessions.push(s);
        }
    }
    let snapshot = SessionsSnapshot {
        version: 1,
        index,
        sessions,
    };
    let json = serde_json::to_string(&snapshot)
        .map_err(|e| format!("序列化 sessions 失败: {}", e))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(json.as_bytes()))
}

#[tauri::command]
pub fn import_sessions_snapshot(
    payload: String,
    prune_orphans: Option<bool>,
) -> Result<u32, String> {
    use base64::Engine;
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return Err("剪贴板为空 / 没有 snapshot 字符串。".to_string());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(trimmed)
        .map_err(|e| format!("base64 解码失败：{}", e))?;
    let json = std::str::from_utf8(&bytes)
        .map_err(|e| format!("UTF-8 解析失败：{}", e))?;
    let snapshot: SessionsSnapshot = serde_json::from_str(json)
        .map_err(|e| format!("JSON 解析失败：{}", e))?;
    if snapshot.version != 1 {
        return Err(format!(
            "快照版本 {} 不被支持（当前期望 1）。",
            snapshot.version
        ));
    }
    // Write each session file（同 id 直接覆盖）。
    for s in &snapshot.sessions {
        let path = session_path(&s.id)?;
        let body = serde_json::to_string_pretty(s)
            .map_err(|e| format!("Failed to serialize session {}: {e}", s.id))?;
        fs::write(&path, body)
            .map_err(|e| format!("Failed to write session {}: {e}", s.id))?;
    }
    write_index(&snapshot.index)?;
    // prune_orphans = true 时遍历 sessions dir，删 disk 上 *.json 不在 snapshot
    // 里的 session 文件 + index.json 之外的杂项。返回删的条数让前端反馈给用户。
    // 失败的单条 rm 计入 console；不阻塞 import 主流程。
    let mut pruned = 0u32;
    if prune_orphans.unwrap_or(false) {
        let snap_ids: std::collections::HashSet<&str> =
            snapshot.sessions.iter().map(|s| s.id.as_str()).collect();
        let dir = sessions_dir()?;
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                if stem == "index" {
                    continue; // 索引文件刚被 write_index 覆写，不要删
                }
                if !snap_ids.contains(stem) {
                    if fs::remove_file(&path).is_ok() {
                        pruned += 1;
                    }
                }
            }
        }
    }
    Ok(pruned)
}

/// 全量清空 sessions：删 disk 所有 *.json（含 index.json）→ create_session 起一
/// 个新空 session 作 active，保证 PanelChat 总有一个可用会话。返回旧 session
/// 数让前端反馈给用户。
///
/// 仅清 sessions 目录，不动 memory / SOUL.md / butler_history / config.yaml
/// 等其它持久化数据（tooltip 也要说清楚）。
#[tauri::command]
pub fn clear_all_sessions() -> Result<u32, String> {
    let dir = sessions_dir()?;
    let mut deleted = 0u32;
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if fs::remove_file(&path).is_ok() {
                deleted += 1;
            }
        }
    }
    // 起新空 session（reuse create_session，自动写 index.json + active_id）。
    // deleted 计的是旧 session 文件数（含 index.json），create 后新文件不影响
    // 这个数 —— 前端看到的是"清掉了几个"。
    create_session()?;
    Ok(deleted)
}

#[tauri::command]
pub fn delete_session(id: String) -> Result<(), String> {
    // Remove file
    let path = session_path(&id)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Failed to delete session file: {e}"))?;
    }

    // Update index
    let mut index = read_index();
    index.sessions.retain(|m| m.id != id);
    if index.active_id == id {
        index.active_id = index
            .sessions
            .last()
            .map(|m| m.id.clone())
            .unwrap_or_default();
    }
    write_index(&index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session(items: Vec<serde_json::Value>) -> Session {
        Session {
            id: "sid".to_string(),
            title: "测试会话".to_string(),
            created_at: "2026-05-04T10:00:00.000".to_string(),
            updated_at: "2026-05-04T11:00:00.000".to_string(),
            messages: vec![],
            items,
        }
    }

    fn item(role: &str, content: &str) -> serde_json::Value {
        serde_json::json!({ "type": role, "content": content })
    }

    // ---------------- find_match_snippet ----------------

    #[test]
    fn snippet_returns_none_when_keyword_not_found() {
        assert!(find_match_snippet("hello world", "missing", 10).is_none());
    }

    #[test]
    fn snippet_returns_none_for_empty_keyword() {
        assert!(find_match_snippet("hello", "", 10).is_none());
    }

    #[test]
    fn snippet_includes_full_content_when_short() {
        // content 较短 → 不应有前导 / 后缀 `…`
        let (snip, m) = find_match_snippet("hello world", "hello", 10).unwrap();
        assert_eq!(snip, "hello world");
        assert_eq!(m, 0);
    }

    #[test]
    fn snippet_truncates_with_ellipsis_when_long() {
        // 关键字处于中部 → 前后各加 `…`
        let content = format!("{}{}{}", "a".repeat(100), "TARGET", "b".repeat(100));
        let (snip, m) = find_match_snippet(&content, "target", 20).unwrap();
        assert!(snip.starts_with('…'));
        assert!(snip.ends_with('…'));
        // 验证 match_start 偏移正确：snippet 从 `…` 开始，前缀点占 1 char
        // 然后 20 个 'a'（ctx_chars） → 那么 match_start 应该是 1 + 20 = 21
        assert_eq!(m, 21);
        assert_eq!(snip.chars().nth(m).unwrap(), 'T');
    }

    #[test]
    fn snippet_omits_leading_dots_at_start() {
        // 命中在开头时 — 左侧 ctx 用不完，不加 `…`
        let content = "TARGET appears at start of text".to_string();
        let (snip, m) = find_match_snippet(&content, "target", 5).unwrap();
        assert!(!snip.starts_with('…'));
        assert_eq!(m, 0);
    }

    #[test]
    fn snippet_omits_trailing_dots_at_end() {
        let content = "text ends with TARGET".to_string();
        let (snip, _) = find_match_snippet(&content, "target", 5).unwrap();
        assert!(!snip.ends_with('…'));
    }

    #[test]
    fn snippet_handles_chinese_substring_with_correct_char_offset() {
        // 中文 char 偏移检验：每个中文 char 是 3 byte，但接口用 char 计
        let content = "今天天气真好我想出门散步走走";
        let (snip, m) = find_match_snippet(content, "我想出门", 4).unwrap();
        // 命中起始 char 索引 = 6（"今天天气真好" 是 6 个字）
        // 左 ctx=4 → start=2，右 ctx=4 → end=6+4+4=14；snippet 长度受 content 长度限制
        // prefix_dots = (2 > 0) = true, suffix_dots = (14 >= len 14) = false（content 14 字）
        // 实际：content 总 char 数 = 14；命中位置 6；end = min(6+4+4, 14) = 14 → 不加后缀
        assert!(snip.starts_with('…'));
        assert!(!snip.ends_with('…'));
        // m = (6 - 2) + 1 = 5，对应 snippet 第 5 个 char 是 '我'
        assert_eq!(m, 5);
        assert_eq!(snip.chars().nth(m).unwrap(), '我');
    }

    #[test]
    fn snippet_is_case_insensitive_in_search_but_preserves_original_case() {
        // kw_lower 已小写；content 可能是任意大小写。snippet 保留原文 case。
        let (snip, m) = find_match_snippet("Hello World", "hello", 10).unwrap();
        assert!(snip.contains("Hello")); // 原始大小写保留
        assert_eq!(m, 0);
    }

    // ---------------- search_session_items ----------------

    #[test]
    fn session_items_search_skips_non_user_assistant_roles() {
        let session = make_session(vec![
            item("user", "找这个"),
            item("tool", "工具调用 找这个"),
            item("error", "错误 找这个"),
            item("assistant", "也找这个"),
        ]);
        let hits = search_session_items(&session, "找这个", 3, 20);
        assert_eq!(hits.len(), 2, "should match only user + assistant");
        assert_eq!(hits[0].role, "user");
        assert_eq!(hits[1].role, "assistant");
    }

    #[test]
    fn session_items_first_match_per_item_only() {
        // 同一条 item 含两次匹配 → 只输出一条 SearchHit（用户点跳到该 item 即可）
        let session = make_session(vec![item("user", "abc abc abc")]);
        let hits = search_session_items(&session, "abc", 3, 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].match_start, 0);
    }

    #[test]
    fn session_items_carries_session_metadata_into_hit() {
        let session = make_session(vec![item("user", "找我")]);
        let hits = search_session_items(&session, "找我", 2, 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, "sid");
        assert_eq!(hits[0].session_title, "测试会话");
        assert_eq!(hits[0].item_index, 0);
        assert_eq!(hits[0].match_len, 2);
    }

    #[test]
    // ---------------- estimate_tokens ----------------

    #[test]
    fn estimate_tokens_empty_is_zero() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn estimate_tokens_cjk_one_per_char() {
        // 「整理 Downloads」: 2 CJK + 1 space + 9 ASCII non-whitespace
        // → 2 + ceil(9/4) = 2 + 3 = 5
        assert_eq!(estimate_tokens("整理 Downloads"), 5);
    }

    #[test]
    fn estimate_tokens_ascii_quarter() {
        // "hello world" = 10 non-whitespace + 1 space → ceil(10/4) = 3
        assert_eq!(estimate_tokens("hello world"), 3);
    }

    #[test]
    fn estimate_tokens_whitespace_only_zero() {
        assert_eq!(estimate_tokens("   \n\n\t"), 0);
    }

    #[test]
    fn estimate_tokens_pure_cjk() {
        // 11 个汉字（我/是/一/只/可/爱/的/桌/面/宠/物） → 11
        assert_eq!(estimate_tokens("我是一只可爱的桌面宠物"), 11);
    }

    // ---------------- content_value_text ----------------

    #[test]
    fn content_text_extract_string() {
        let v = serde_json::json!("hello");
        assert_eq!(content_value_text(&v), "hello");
    }

    #[test]
    fn content_text_extract_multipart() {
        let v = serde_json::json!([
            { "type": "text", "text": "first" },
            { "type": "image_url", "image_url": { "url": "data:..." } },
            { "type": "text", "text": "second" },
        ]);
        // 两段 text 用 \n 拼接；image_url 不参与
        assert_eq!(content_value_text(&v), "first\nsecond");
    }

    #[test]
    fn content_text_extract_unknown_returns_empty() {
        let v = serde_json::json!({ "weird": "shape" });
        assert_eq!(content_value_text(&v), "");
    }

    #[test]
    fn session_items_no_match_returns_empty() {
        let session = make_session(vec![item("user", "无关内容")]);
        assert!(search_session_items(&session, "找不到", 3, 10).is_empty());
    }
}
