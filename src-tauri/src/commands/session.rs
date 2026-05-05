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
        });
    }
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
    fn session_items_no_match_returns_empty() {
        let session = make_session(vec![item("user", "无关内容")]);
        assert!(search_session_items(&session, "找不到", 3, 10).is_empty());
    }
}
