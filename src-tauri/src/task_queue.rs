//! 任务队列：在现有 `butler_tasks` 内存类目之上叠一层结构化 header
//! `[task pri=N due=YYYY-MM-DDThh:mm]`，让面板能"按优先级 + 截止时间"
//! 给用户排好序的视图，宠物侧的 proactive 循环不变（依旧消费描述文本）。
//!
//! 本模块**只装纯函数**：header 解析、状态判定、排序比较器、TaskView
//! 数据形态。任何 IO（读 memory_list / 写 memory_edit）由
//! `commands/task.rs` 在外层处理。这条边界与 `proactive/morning_briefing.rs`
//! 跟 `proactive.rs` 之间的边界一致 —— 让所有边界条件可单测。
//!
//! 兼容性设计：历史 `butler_tasks` 条目（无 task header，可能带 `[once:]` /
//! `[every:]` / `[done]` / `[error]`）由本模块视作 `priority = 0, due = None,
//! status` 仍按存量约定判定。这样旧条目能与新条目混排在一个面板列表里，
//! 既不改写已存数据也不破坏现有 prompt 行为。

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// 优先级上限。10 档（0..=9）足够日常任务区分"现在做 / 一会儿做 / 闲时做"，
/// 又不会逼用户在 P3 vs P4 上犹豫。前端 UI 用 0-9 数字直输或滑块都可以。
pub const TASK_PRIORITY_MAX: u8 = 9;

/// 解析后的任务 header。`body` 是去掉 header 那段后的纯描述（首尾空白
/// 已 trim）；调用方如果只想要"可读描述"应优先用 `body` 而不是原始
/// description。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskHeader {
    pub priority: u8,
    pub due: Option<NaiveDateTime>,
    pub body: String,
}

/// 任务四态。判定优先级：Cancelled > Error > Done > Pending —— 见
/// `classify_status`。各态的语义：
/// - `Pending`：未结束，可被宠物取走执行
/// - `Done`：宠物自标完成（`[done]`）
/// - `Error`：宠物自标失败（`[error: 原因]`），可重试
/// - `Cancelled`：用户在面板手动取消（`[cancelled: 原因]`），终态、不可重试
///
/// Cancelled 优先于 Error，是为了让用户的"我说不做就不做"压过先前的
/// 错误状态 —— 比如一条任务先 errored，用户决定干脆放弃，按取消之后
/// 它就该是 Cancelled 而不是仍显示 Error。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Done,
    Error,
    Cancelled,
}

/// 序列化给前端的视图。`due` 用 ISO-8601 文本传输（前端 datetime-local
/// 直接使用），避免 JSON number 时间戳带来的时区歧义。
#[derive(Debug, Clone, Serialize)]
pub struct TaskView {
    pub title: String,
    pub body: String,
    pub priority: u8,
    /// 形如 `2026-05-05T18:00`（无时区后缀，本地时区，与输入对称）。
    pub due: Option<String>,
    pub status: TaskStatus,
    /// `[error: ...]` / `[cancelled: ...]` 括号内的简短原因，无时返回 None。
    /// Status 是 Done / Pending 时总是 None；Status 是 Error / Cancelled
    /// 时根据 description 是否带消息文本返回 Some / None。
    /// 字段名保持 `error_message` 是历史兼容 —— 前端现在按"原因消息"
    /// 通用解读即可，不再仅限 error 状态。
    pub error_message: Option<String>,
    /// description 里抽出的 `#tag` 列表（去掉 `#`，首次出现顺序，已去重）。
    /// 给面板渲染 chip 与周报按 tag 聚合用。
    pub tags: Vec<String>,
    /// description 里 `[result: ...]` 标记的内容。LLM 在标 `[done]` 时
    /// 软约定附上一句"做了什么"。Some 时面板已结束行会显式展示"✓ 产物"
    /// 一行；周报"完成清单"也优先用此而非整段描述。
    pub result: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// header 包装格式：`[task pri=N due=YYYY-MM-DDThh:mm]`。
/// - `pri` 必填（缺省视作 0）。
/// - `due` 可选；缺省视作"无截止"。
/// - 任一字段格式错误 → 整体 None，调用方按"无 header 任务"处理。
///
/// 解析容忍：
/// - header 前后允许任意空白；
/// - `pri=` 与 `due=` 顺序无关（`[task due=... pri=...]` 也接受）；
/// - 数字越界（`pri > TASK_PRIORITY_MAX`） → None，避免误显示成"超高优先级"；
/// - 时间无效（如 `due=2026-13-99T25:99`）→ None。
pub fn parse_task_header(description: &str) -> Option<TaskHeader> {
    let trimmed = description.trim_start();
    let rest = trimmed.strip_prefix("[task")?;
    // 找到匹配的 `]`，header 内不允许嵌套方括号。
    let end = rest.find(']')?;
    let inside = &rest[..end];
    let after = rest[end + 1..].trim_start();

    let mut priority: Option<u8> = None;
    let mut due: Option<NaiveDateTime> = None;
    let mut due_seen = false;
    let mut pri_seen = false;

    for token in inside.split_whitespace() {
        if let Some(v) = token.strip_prefix("pri=") {
            if pri_seen {
                return None; // 重复字段视作格式错误
            }
            pri_seen = true;
            let n: u8 = v.parse().ok()?;
            if n > TASK_PRIORITY_MAX {
                return None;
            }
            priority = Some(n);
        } else if let Some(v) = token.strip_prefix("due=") {
            if due_seen {
                return None;
            }
            due_seen = true;
            let dt = NaiveDateTime::parse_from_str(v, "%Y-%m-%dT%H:%M").ok()?;
            due = Some(dt);
        } else {
            // 未知 token —— 严格起见拒绝，免得未来扩展时静默吃掉新字段
            return None;
        }
    }

    Some(TaskHeader {
        priority: priority.unwrap_or(0),
        due,
        body: after.to_string(),
    })
}

/// 把 header 与 body 拼成可写入 `butler_tasks.description` 的字符串。
/// 与 `parse_task_header` 互逆 —— 写入后再 parse 应恢复同样的 TaskHeader。
pub fn format_task_description(header: &TaskHeader) -> String {
    let mut out = String::from("[task pri=");
    out.push_str(&header.priority.to_string());
    if let Some(d) = header.due {
        out.push_str(" due=");
        out.push_str(&d.format("%Y-%m-%dT%H:%M").to_string());
    }
    out.push(']');
    let body = header.body.trim();
    if !body.is_empty() {
        out.push(' ');
        out.push_str(body);
    }
    out
}

/// 状态判定。`description` 是 butler_tasks 的原始 description（可能含
/// header，也可能没有，看历史条目）。
///
/// 顺序：cancelled > error > done > pending。
/// - **cancelled** 优先：用户的"取消"是终态意图，压过此前的 error / done。
/// - **error** 其次：宠物自标失败，用户可点重试。
/// - **done** 再次：宠物自标完成。
/// - **pending** 兜底。
///
/// 这个顺序故意不被排序比较器覆盖 —— 排序比较器另有自己的层级，但
/// status 字段本身的判定单值就在这里定。
pub fn classify_status(description: &str) -> (TaskStatus, Option<String>) {
    if let Some(msg) = extract_cancelled_message(description) {
        return (TaskStatus::Cancelled, msg);
    }
    if let Some(msg) = extract_error_message(description) {
        return (TaskStatus::Error, msg);
    }
    if has_done_marker(description) {
        return (TaskStatus::Done, None);
    }
    (TaskStatus::Pending, None)
}

/// 抽出 `[cancelled: xxx]` 内的 xxx。约定与 `extract_error_message` 同
/// 形 —— `cancelled` 后允许 `:` / `：` / 空格；闭合 `]`。返回结构：
/// - 不存在 cancelled 标记 → None（外层视作非 Cancelled）；
/// - 存在但消息为空（`[cancelled]`）→ `Some(None)`（仍是 Cancelled，
///   只是没有原因 — 用户没填或选择"无理由取消"，下游显示「已取消」即可）；
/// - 存在且消息非空 → `Some(Some(text))`。
fn extract_cancelled_message(description: &str) -> Option<Option<String>> {
    let idx = description.find("[cancelled")?;
    let rest = &description[idx + "[cancelled".len()..];
    let end = rest.find(']')?;
    let inside = &rest[..end];
    let msg = inside
        .trim_start_matches([':', '：', ' '])
        .trim()
        .to_string();
    Some(if msg.is_empty() { None } else { Some(msg) })
}

/// 把 description 里所有 `[error...]` 段剥掉，连带剥掉孤立的 `[done]` /
/// `[done ...]` 标记 —— 重试语义需要让这条任务回到 pending：哪怕 LLM 上轮
/// 误把它标了 done，也得复位。多余空白 collapse 到单空格，首尾 trim。
///
/// 不动 task header（`[task pri=... due=...]`） / cancelled 标记 / 普通
/// 文本 —— 调用方在 status == Error 时才会触发，cancelled 不可达此路径。
pub fn strip_error_markers(description: &str) -> String {
    let cleaned = remove_bracketed_segments(description, &["[error", "[done"]);
    collapse_whitespace(&cleaned)
}

/// 抽 `#xxx` 形式的 tags。词法：`#` 起始 + 一段连续"tag 字符"
/// （ASCII 字母 / 数字 / `_` / `-` / 任意非 ASCII 字符如中文）。
/// 空白、ASCII 标点（除 `_-`）、`#`、`]` 均终止 tag。返回**首次出现顺
/// 序**且**去重**的列表（不带 `#` 前缀）。
///
/// 边界：
/// - `#`后无字符（孤立的 `#`，或后面紧跟空白）→ 跳过。
/// - 同 tag 多次出现 → 只保留首个。
/// - 大小写敏感存储（聚合层若想合并视情况再 lower-case）。
pub fn parse_task_tags(description: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let chars: Vec<char> = description.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '#' {
            i += 1;
            continue;
        }
        // 看上一个字符——若紧贴标识符字符（如 `abc#def`），则不视作新 tag 起始。
        if i > 0 && is_tag_char(chars[i - 1]) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len() && is_tag_char(chars[j]) {
            j += 1;
        }
        if j > i + 1 {
            let tag: String = chars[i + 1..j].iter().collect();
            if seen.insert(tag.clone()) {
                out.push(tag);
            }
        }
        i = j.max(i + 1);
    }
    out
}

fn is_tag_char(c: char) -> bool {
    c == '_' || c == '-' || c.is_ascii_alphanumeric() || (!c.is_ascii() && !c.is_whitespace())
}

/// 单个批量 tag 操作。Add 加该 tag（已有则 noop），Remove 删该 tag（不在则 noop）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagOp {
    Add(String),
    Remove(String),
}

/// pure：解析批量 tag 操作输入字符串，如 `+tag1 -tag2 +工作`。
///
/// 规则：
/// - 空白分隔的 token，每个必须以 `+` 或 `-` 开头
/// - 前缀后的剩余作为 tag 名；空 → Err（用户输了孤立的 `+` / `-`）
/// - 重复 op（`+a +a`）→ 去重保留首次
/// - 互斥冲突（`+a -a` 同输入）→ Err，让用户重输（不引入"先加后删 = 净删"
///   的潜规则）
/// - 整个输入空 / 全空白 → Err（避免无操作误触发）
pub fn parse_tag_ops(input: &str) -> Result<Vec<TagOp>, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("没有指定任何 tag 操作".to_string());
    }
    let mut ops: Vec<TagOp> = Vec::new();
    let mut adds: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut removes: std::collections::HashSet<String> = std::collections::HashSet::new();
    for token in trimmed.split_whitespace() {
        let (sign, name) = if let Some(rest) = token.strip_prefix('+') {
            ('+', rest)
        } else if let Some(rest) = token.strip_prefix('-') {
            ('-', rest)
        } else {
            return Err(format!("token `{}` 缺少 +/- 前缀", token));
        };
        if name.is_empty() {
            return Err(format!("token `{}` 没有 tag 名", token));
        }
        // 校验 tag 名只含合法字符（与 parse_task_tags 边界一致）
        if !name.chars().all(is_tag_char) {
            return Err(format!("tag 名 `{}` 含非法字符（应为字母 / 数字 / 中文 / `_` / `-`）", name));
        }
        let key = name.to_string();
        match sign {
            '+' => {
                if removes.contains(&key) {
                    return Err(format!("tag `{}` 同次既 + 又 -，请二选一", key));
                }
                if adds.insert(key.clone()) {
                    ops.push(TagOp::Add(key));
                }
            }
            '-' => {
                if adds.contains(&key) {
                    return Err(format!("tag `{}` 同次既 + 又 -，请二选一", key));
                }
                if removes.insert(key.clone()) {
                    ops.push(TagOp::Remove(key));
                }
            }
            _ => unreachable!(),
        }
    }
    Ok(ops)
}

/// pure：把 `ops` 应用到 description，返回新 description。
///
/// - Add：当前 tag 集合不含 → 追加 ` #tag` 到末尾；含 → noop
/// - Remove：扫所有 `#tag` token（以 `parse_task_tags` 同款边界）→ 删它
///   + 紧邻前导空格（避免出现孤立 `  ` 双空格）
///
/// 不动 description 里的其它 markers（`[task pri=...]` / `[origin:...]`
/// 等）。多 op 顺序应用，互斥冲突已被 parse_tag_ops 拒绝。
pub fn apply_tag_ops(description: &str, ops: &[TagOp]) -> String {
    let mut s = description.to_string();
    for op in ops {
        match op {
            TagOp::Add(name) => {
                let existing = parse_task_tags(&s);
                if existing.iter().any(|t| t == name) {
                    continue;
                }
                if !s.is_empty() && !s.ends_with(char::is_whitespace) {
                    s.push(' ');
                }
                s.push('#');
                s.push_str(name);
            }
            TagOp::Remove(name) => {
                s = remove_tag_token(&s, name);
            }
        }
    }
    collapse_whitespace(&s)
}

/// 内部 helper：从 description 字符串里删除所有形如 ` #name` 或行首
/// `#name` 的 token，name 比较与 parse_task_tags 一致（前后字符必须不是
/// is_tag_char，避免误伤 `#tags-with-name` 等）。
fn remove_tag_token(s: &str, name: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let target: Vec<char> = name.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // 寻找一个 `#name` 边界匹配点
        if chars[i] == '#'
            && (i == 0 || !is_tag_char(chars[i - 1]))
            && i + 1 + target.len() <= chars.len()
            && chars[i + 1..i + 1 + target.len()] == target[..]
            && (i + 1 + target.len() == chars.len()
                || !is_tag_char(chars[i + 1 + target.len()]))
        {
            // 删它本身 + 紧邻前导空白（让 collapse_whitespace 后没双空格）
            while let Some(&last) = out.last() {
                if last.is_whitespace() {
                    out.pop();
                } else {
                    break;
                }
            }
            i += 1 + target.len();
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out.into_iter().collect()
}

/// 抽首个 `[result: 文本]` 内的文本。`result` 后允许 `:` / `：`（中文冒
/// 号）/ 空格。trim 后空字符串视为无产物（None）。多个 `[result:]` 段
/// 时只取首个 —— 单写者协议；多个一定是脏数据，按"最早一条"取。
pub fn parse_task_result(description: &str) -> Option<String> {
    let idx = description.find("[result")?;
    let rest = &description[idx + "[result".len()..];
    let end = rest.find(']')?;
    let inside = &rest[..end];
    let trimmed = inside.trim_start_matches([':', '：', ' ']).trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// 把 description 里的 `[result:]` 段剥掉。给面板 body 显示用 —— 产物
/// 已经单独在 TaskView.result 字段展示，没必要在 body 里重复出现。
pub fn strip_result_marker(description: &str) -> String {
    let cleaned = remove_bracketed_segments(description, &["[result"]);
    collapse_whitespace(&cleaned)
}

/// 任务来源标签。目前只有 Telegram 一种，但加成 enum 留扩展位 ——
/// 未来 webhook / 其它入口可以加新的 variant 而不破坏 description 协议。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskOrigin {
    Tg(i64),
}

/// 解析 description 里的 `[origin:tg:<chat_id>]` 标记。仅支持 telegram
/// 来源；解析失败 / 标记缺失返回 None。chat_id 是 i64（teloxide 给的就
/// 是 i64，包含负数 group id）。
pub fn parse_task_origin(description: &str) -> Option<TaskOrigin> {
    let idx = description.find("[origin:tg:")?;
    let rest = &description[idx + "[origin:tg:".len()..];
    let end = rest.find(']')?;
    let inner = &rest[..end];
    inner.trim().parse::<i64>().ok().map(TaskOrigin::Tg)
}

/// 在 description 末尾追加 origin 标记。已存在 origin 则原样返回（不
/// 重复追加，避免反复 update 把标记叠成一坨）。
pub fn append_origin_marker(description: &str, origin: &TaskOrigin) -> String {
    if parse_task_origin(description).is_some() {
        return description.to_string();
    }
    let marker = match origin {
        TaskOrigin::Tg(id) => format!("[origin:tg:{}]", id),
    };
    let trimmed = description.trim_end();
    if trimmed.is_empty() {
        marker
    } else {
        format!("{} {}", trimmed, marker)
    }
}

/// 把 description 里的 origin 标记剥掉（给面板 body 显示用）。其它
/// 标记 / 文本保留。空白合并由 `collapse_whitespace` 收尾。多个 origin
/// 段也都会被剥掉（防御 — 单写者协议保证只有一个）。
pub fn strip_origin_marker(description: &str) -> String {
    let cleaned = remove_bracketed_segments(description, &["[origin:"]);
    collapse_whitespace(&cleaned)
}

/// 在 description 末尾追加 `[cancelled: <reason>]`（reason 为空时写
/// `[cancelled]`）。不重写已有内容 —— 保留 task header / 旧的 done /
/// error 痕迹（虽然 cancelled 优先级最高，但保留事实痕迹便于调试 / 周
/// 报回看）。重复调用会追加多个 cancelled 段，但 classify_status 取第
/// 一个出现的，所以语义稳定（最早一次取消生效）。
pub fn append_cancelled_marker(description: &str, reason: &str) -> String {
    let trimmed = description.trim_end();
    let reason_trimmed = reason.trim();
    let marker = if reason_trimmed.is_empty() {
        "[cancelled]".to_string()
    } else {
        format!("[cancelled: {}]", reason_trimmed)
    };
    if trimmed.is_empty() {
        marker
    } else {
        format!("{} {}", trimmed, marker)
    }
}

/// 单遍扫 input：每个位置检查是否以 `prefixes` 中任一开头；命中且能找
/// 到 `]` → 整段（包含 `]`）跳过；否则原样复制一个字符。结果里所有
/// 命中的 bracketed 段都被剥掉，未命中的方括号段（如 `[task pri=...]`）
/// 原样保留。空白合并交给 `collapse_whitespace`。
fn remove_bracketed_segments(input: &str, prefixes: &[&str]) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &input[i..];
        let mut matched: Option<usize> = None;
        for p in prefixes {
            if rest.starts_with(p) {
                if let Some(close) = rest.find(']') {
                    matched = Some(close + 1);
                    break;
                }
            }
        }
        match matched {
            Some(skip) => i += skip,
            None => {
                // 走 char_indices 安全边界：UTF-8 多字节字符不能按 byte 步进
                let ch_end = next_char_boundary(input, i);
                out.push_str(&input[i..ch_end]);
                i = ch_end;
            }
        }
    }
    out
}

fn next_char_boundary(input: &str, i: usize) -> usize {
    let bytes = input.as_bytes();
    let mut j = i + 1;
    while j < bytes.len() && !input.is_char_boundary(j) {
        j += 1;
    }
    j
}

/// 把多空白合并成单空格，首尾 trim。CR / LF 也算空白。
fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_space = true;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

/// 抽出 `[error: xxx]` 内的 xxx。约定：`error` 后可跟 `:`、`：`（中文冒
/// 号）或空格；闭合 `]`。匹配不到 → None；找到了但 token 内为空 → 返回
/// `Some(("".to_string()))`，让 `classify_status` 仍把它判为 Error 状态
/// （宠物显式标了错就是错，详细原因用户得自己去看 detail.md）。
fn extract_error_message(description: &str) -> Option<Option<String>> {
    let idx = description.find("[error")?;
    let rest = &description[idx + "[error".len()..];
    let end = rest.find(']')?;
    let inside = &rest[..end];
    let msg = inside
        .trim_start_matches([':', '：', ' '])
        .trim()
        .to_string();
    Some(if msg.is_empty() { None } else { Some(msg) })
}

/// `[done]` 检测。要求是独立 token —— 不接受 description 中混进的 "done"
/// 单词。约定写法：`[done]` 紧贴上下文空白或文本起止。
fn has_done_marker(description: &str) -> bool {
    // 简单正则替代：扫到 "[done" 后看后一字符是不是 ']'。
    let mut i = 0;
    let bytes = description.as_bytes();
    while i + 5 < bytes.len() {
        if &bytes[i..i + 5] == b"[done" {
            // 容忍 [done] 与 [done...] 两种写法以保护未来扩展（如
            // `[done at=...]`），但拒绝 `[done...` 没闭合的情况。
            let after = &description[i + 5..];
            if after.starts_with(']') || after.starts_with(' ') {
                if after.contains(']') {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

/// 排序比较器。语义：返回 `Less` 表示 a 应排在 b 前面（"宠物更优先做"）。
///
/// 层级：
/// 1. 状态：Error > Pending > Done > Cancelled。
///    - Error / Pending 在活动段（队列里仍可推进）；
///    - Done / Cancelled 在结束段（默认面板里被「显示已结束」开关隐藏）；
///    - Done 优于 Cancelled —— "已完成"代表正向产出，"已取消"是放弃，
///      "显示已结束"打开时让正向结果先冒上来更符合用户回看的直觉。
/// 2. 过期紧迫度：a / b 中"已过期"的排前；都过期时越久前的越前。
/// 3. 优先级降序：pri 大的排前。
/// 4. 截止时间升序：早到期的排前；无 due 视为 +∞。
/// 5. 创建时间升序：稳定 tie-break，老任务优先（避免被新任务永久饿死）。
pub fn compare_for_queue(a: &TaskView, b: &TaskView, now: NaiveDateTime) -> Ordering {
    // 1. 状态
    let s_a = status_rank(a.status);
    let s_b = status_rank(b.status);
    if s_a != s_b {
        return s_a.cmp(&s_b);
    }

    // 2. 过期紧迫度
    let due_a = parse_due(&a.due);
    let due_b = parse_due(&b.due);
    let overdue_a = due_a.map(|d| d <= now).unwrap_or(false);
    let overdue_b = due_b.map(|d| d <= now).unwrap_or(false);
    match (overdue_a, overdue_b) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        (true, true) => {
            // 过期越久越前 —— due 越小越前。
            return due_a.cmp(&due_b);
        }
        _ => {}
    }

    // 3. 优先级降序
    if a.priority != b.priority {
        return b.priority.cmp(&a.priority);
    }

    // 4. due 升序（无 due → +∞）
    match (due_a, due_b) {
        (Some(x), Some(y)) if x != y => return x.cmp(&y),
        (Some(_), None) => return Ordering::Less,
        (None, Some(_)) => return Ordering::Greater,
        _ => {}
    }

    // 5. created_at 升序
    a.created_at.cmp(&b.created_at)
}

fn status_rank(s: TaskStatus) -> u8 {
    match s {
        TaskStatus::Error => 0,
        TaskStatus::Pending => 1,
        TaskStatus::Done => 2,
        TaskStatus::Cancelled => 3,
    }
}

fn parse_due(s: &Option<String>) -> Option<NaiveDateTime> {
    let raw = s.as_deref()?;
    NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M").ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dt(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, 0)
            .unwrap()
    }

    fn view(
        title: &str,
        priority: u8,
        due: Option<&str>,
        status: TaskStatus,
        created_at: &str,
    ) -> TaskView {
        TaskView {
            title: title.to_string(),
            body: String::new(),
            priority,
            due: due.map(String::from),
            status,
            error_message: None,
            tags: Vec::new(),
            result: None,
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
        }
    }

    // ---------------- parse_task_header ----------------

    #[test]
    fn parses_full_header() {
        let h = parse_task_header("[task pri=3 due=2026-05-05T18:00] 整理 Downloads")
            .expect("should parse");
        assert_eq!(h.priority, 3);
        assert_eq!(h.due, Some(dt(2026, 5, 5, 18, 0)));
        assert_eq!(h.body, "整理 Downloads");
    }

    #[test]
    fn parses_pri_only_header() {
        let h = parse_task_header("[task pri=1] 喝水").expect("should parse");
        assert_eq!(h.priority, 1);
        assert_eq!(h.due, None);
        assert_eq!(h.body, "喝水");
    }

    #[test]
    fn accepts_field_order_swap() {
        // due 在 pri 前面也接受 — 容忍写法
        let h = parse_task_header("[task due=2026-05-05T09:00 pri=2] 早会").expect("should parse");
        assert_eq!(h.priority, 2);
        assert_eq!(h.due, Some(dt(2026, 5, 5, 9, 0)));
    }

    #[test]
    fn returns_none_for_missing_brackets() {
        assert!(parse_task_header("task pri=1 没有方括号").is_none());
        assert!(parse_task_header("[task pri=1 没闭合").is_none());
    }

    #[test]
    fn returns_none_for_priority_out_of_range() {
        // 10 超出 0..=9 — 拒绝而不是 saturating
        assert!(parse_task_header("[task pri=10] x").is_none());
        assert!(parse_task_header("[task pri=255] x").is_none());
    }

    #[test]
    fn returns_none_for_invalid_due() {
        assert!(parse_task_header("[task pri=1 due=not-a-date] x").is_none());
        assert!(parse_task_header("[task pri=1 due=2026-13-99T25:99] x").is_none());
    }

    #[test]
    fn returns_none_for_unknown_token() {
        // 严格：未知字段视作格式错误，避免未来扩展时静默忽略
        assert!(parse_task_header("[task pri=1 status=done] x").is_none());
    }

    #[test]
    fn returns_none_for_duplicate_field() {
        assert!(parse_task_header("[task pri=1 pri=2] x").is_none());
        assert!(parse_task_header("[task due=2026-05-05T09:00 due=2026-05-05T10:00 pri=1] x").is_none());
    }

    #[test]
    fn returns_none_for_non_task_brackets() {
        // [once:...] / [every:...] / [done] 都不是 task header — 别误命中
        assert!(parse_task_header("[once: 2026-05-05T18:00] x").is_none());
        assert!(parse_task_header("[every: 09:00] x").is_none());
        assert!(parse_task_header("[done] x").is_none());
    }

    #[test]
    fn body_is_trimmed_and_can_be_empty() {
        let h = parse_task_header("[task pri=0]").expect("empty body still valid");
        assert_eq!(h.body, "");
        let h = parse_task_header("[task pri=0]    ").unwrap();
        assert_eq!(h.body, "");
    }

    // ---------------- format_task_description ----------------

    #[test]
    fn format_round_trips_with_parse() {
        let h = TaskHeader {
            priority: 5,
            due: Some(dt(2026, 6, 1, 10, 30)),
            body: "测试".to_string(),
        };
        let s = format_task_description(&h);
        let parsed = parse_task_header(&s).unwrap();
        assert_eq!(parsed, h);
    }

    #[test]
    fn format_omits_due_when_none() {
        let h = TaskHeader {
            priority: 0,
            due: None,
            body: "x".to_string(),
        };
        let s = format_task_description(&h);
        assert_eq!(s, "[task pri=0] x");
        assert!(!s.contains("due="));
    }

    // ---------------- classify_status ----------------

    #[test]
    fn classify_pending_when_no_markers() {
        let (s, m) = classify_status("[task pri=1] 整理文件");
        assert_eq!(s, TaskStatus::Pending);
        assert!(m.is_none());
    }

    #[test]
    fn classify_done_for_done_marker() {
        let (s, _) = classify_status("[task pri=1] 整理 [done]");
        assert_eq!(s, TaskStatus::Done);
    }

    #[test]
    fn classify_error_with_message() {
        let (s, m) = classify_status("[task pri=1] [error: 文件不存在] 复查");
        assert_eq!(s, TaskStatus::Error);
        assert_eq!(m.as_deref(), Some("文件不存在"));
    }

    #[test]
    fn classify_error_takes_precedence_over_done() {
        // 即使描述里也有 [done]，error 仍然优先 — 出错状态不该被 done 掩盖
        let (s, m) = classify_status("整理 [done] [error: 没权限]");
        assert_eq!(s, TaskStatus::Error);
        assert_eq!(m.as_deref(), Some("没权限"));
    }

    #[test]
    fn classify_cancelled_with_reason() {
        let (s, m) = classify_status("[task pri=1] x [cancelled: 不再需要]");
        assert_eq!(s, TaskStatus::Cancelled);
        assert_eq!(m.as_deref(), Some("不再需要"));
    }

    #[test]
    fn classify_cancelled_without_reason() {
        // [cancelled] 无副文案：仍判 Cancelled，但 reason = None
        let (s, m) = classify_status("[task pri=1] x [cancelled]");
        assert_eq!(s, TaskStatus::Cancelled);
        assert!(m.is_none());
    }

    #[test]
    fn classify_cancelled_takes_precedence_over_error() {
        // 用户的"取消"是终态，覆盖此前的失败状态
        let (s, m) = classify_status("整理 [error: 路径找不到] [cancelled: 不做了]");
        assert_eq!(s, TaskStatus::Cancelled);
        assert_eq!(m.as_deref(), Some("不做了"));
    }

    #[test]
    fn classify_cancelled_takes_precedence_over_done() {
        // 极少见：done + cancelled 共存。语义上"我说取消就取消"，覆盖 done
        let (s, _) = classify_status("整理 [done] [cancelled]");
        assert_eq!(s, TaskStatus::Cancelled);
    }

    #[test]
    fn classify_error_supports_chinese_colon() {
        let (s, m) = classify_status("[error：路径找不到]");
        assert_eq!(s, TaskStatus::Error);
        assert_eq!(m.as_deref(), Some("路径找不到"));
    }

    #[test]
    fn done_marker_must_be_token_not_substring() {
        // "我用 done 这个词描述任务" 不该被误判
        let (s, _) = classify_status("我用 done 形容这个任务");
        assert_eq!(s, TaskStatus::Pending);
    }

    // ---------------- strip_error_markers ----------------

    #[test]
    fn strip_clears_error_segment_and_keeps_header() {
        let cleaned = strip_error_markers("[task pri=2 due=2026-05-05T18:00] 整理 [error: 没权限] 复查");
        // task header 不动；error 段被剥；多余空白合并
        assert_eq!(
            cleaned,
            "[task pri=2 due=2026-05-05T18:00] 整理 复查"
        );
    }

    #[test]
    fn strip_clears_done_alongside_error() {
        // 重试时即便 LLM 误把它标了 done，也得复位
        let cleaned = strip_error_markers("[task pri=1] 整理 [done] [error: 文件不存在]");
        assert_eq!(cleaned, "[task pri=1] 整理");
    }

    #[test]
    fn strip_is_idempotent_on_clean_pending() {
        // 已是干净 pending 的 description 应保持不变（除空白合并）
        let cleaned = strip_error_markers("[task pri=1] 整理 Downloads");
        assert_eq!(cleaned, "[task pri=1] 整理 Downloads");
    }

    #[test]
    fn strip_handles_multiple_error_segments() {
        let cleaned = strip_error_markers("[error: 第一次失败] 进度 [error: 第二次失败]");
        assert_eq!(cleaned, "进度");
    }

    // ---------------- append_cancelled_marker ----------------

    #[test]
    fn append_cancelled_with_reason_round_trips() {
        let appended = append_cancelled_marker("[task pri=1] 整理", "不需要了");
        assert_eq!(appended, "[task pri=1] 整理 [cancelled: 不需要了]");
        let (s, m) = classify_status(&appended);
        assert_eq!(s, TaskStatus::Cancelled);
        assert_eq!(m.as_deref(), Some("不需要了"));
    }

    #[test]
    fn append_cancelled_without_reason_uses_bare_marker() {
        let appended = append_cancelled_marker("[task pri=1] 整理", "  ");
        assert_eq!(appended, "[task pri=1] 整理 [cancelled]");
        let (s, m) = classify_status(&appended);
        assert_eq!(s, TaskStatus::Cancelled);
        assert!(m.is_none());
    }

    #[test]
    fn append_cancelled_to_empty_description() {
        let appended = append_cancelled_marker("", "x");
        assert_eq!(appended, "[cancelled: x]");
    }

    // ---------------- task origin ----------------

    #[test]
    fn parse_origin_extracts_telegram_chat_id() {
        let desc = "[task pri=1] 整理 [origin:tg:123456789]";
        assert_eq!(parse_task_origin(desc), Some(TaskOrigin::Tg(123456789)));
    }

    #[test]
    fn parse_origin_handles_negative_group_id() {
        // 群组 chat_id 在 teloxide 里是负数
        let desc = "[origin:tg:-1001234567890]";
        assert_eq!(parse_task_origin(desc), Some(TaskOrigin::Tg(-1001234567890)));
    }

    #[test]
    fn parse_origin_returns_none_when_absent() {
        assert_eq!(parse_task_origin("[task pri=1] 整理"), None);
    }

    #[test]
    fn parse_origin_returns_none_for_malformed_id() {
        assert_eq!(parse_task_origin("[origin:tg:not-a-number]"), None);
        assert_eq!(parse_task_origin("[origin:tg:]"), None);
    }

    #[test]
    fn append_origin_round_trips_with_parse() {
        let appended = append_origin_marker("[task pri=2] 跑步", &TaskOrigin::Tg(987654));
        assert_eq!(appended, "[task pri=2] 跑步 [origin:tg:987654]");
        assert_eq!(parse_task_origin(&appended), Some(TaskOrigin::Tg(987654)));
    }

    #[test]
    fn append_origin_idempotent_when_already_tagged() {
        // 反复 append 不会叠加多个 origin 段
        let once = append_origin_marker("[task pri=1] x", &TaskOrigin::Tg(42));
        let twice = append_origin_marker(&once, &TaskOrigin::Tg(42));
        let thrice = append_origin_marker(&twice, &TaskOrigin::Tg(42));
        assert_eq!(twice, once);
        assert_eq!(thrice, once);
    }

    #[test]
    fn append_origin_does_not_replace_existing_with_different_id() {
        // 已有 origin → 即便 id 不同也不替换 —— 防御性，避免后续误
        // 调用 swap origin（创建路径只调一次）
        let existing = "[task pri=1] x [origin:tg:1]";
        let attempted = append_origin_marker(existing, &TaskOrigin::Tg(2));
        assert_eq!(attempted, existing);
    }

    #[test]
    fn strip_origin_removes_marker_and_preserves_rest() {
        let desc = "[task pri=2] 整理 Downloads [origin:tg:999]";
        assert_eq!(strip_origin_marker(desc), "[task pri=2] 整理 Downloads");
    }

    #[test]
    fn strip_origin_is_noop_when_absent() {
        let desc = "[task pri=1] 整理 [error: 文件不存在]";
        assert_eq!(strip_origin_marker(desc), desc);
    }

    #[test]
    fn strip_origin_removes_multiple_markers() {
        // 防御性：理论上只该有一个，但脏数据 / 多次写入可能产生多个
        let desc = "x [origin:tg:1] y [origin:tg:2] z";
        assert_eq!(strip_origin_marker(desc), "x y z");
    }

    // ---------------- parse_task_tags ----------------

    #[test]
    fn parse_tags_extracts_ascii_and_chinese() {
        let tags = parse_task_tags("[task pri=2] 整理 Downloads #organize #文件整理 #weekly");
        assert_eq!(tags, vec!["organize", "文件整理", "weekly"]);
    }

    #[test]
    fn parse_tags_dedup_preserves_first_order() {
        let tags = parse_task_tags("#a #b #a #c #b");
        assert_eq!(tags, vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_tags_handles_underscore_and_dash() {
        let tags = parse_task_tags("#tech-debt #user_profile");
        assert_eq!(tags, vec!["tech-debt", "user_profile"]);
    }

    #[test]
    fn parse_tags_skips_lone_hash() {
        // 孤立的 # 或 # 后跟空白都不算 tag
        assert!(parse_task_tags("just # symbol").is_empty());
        assert!(parse_task_tags("hello # world").is_empty());
    }

    #[test]
    fn parse_tags_skips_hash_in_middle_of_word() {
        // "abc#def" 不视作 tag — # 紧贴标识符字符（如 PR 编号 #42 在英文
        // 句中）会引发误命中；要求 # 前不是 tag 字符
        assert_eq!(parse_task_tags("see PR#42 in #weekly notes"), vec!["weekly"]);
    }

    #[test]
    fn parse_tags_terminates_at_punctuation_and_brackets() {
        let tags = parse_task_tags("#a, #b. #c! #d ] #e");
        assert_eq!(tags, vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn parse_tags_returns_empty_for_no_tags() {
        assert!(parse_task_tags("[task pri=1] no tags here").is_empty());
        assert!(parse_task_tags("").is_empty());
    }

    // ---------------- parse_tag_ops ----------------

    #[test]
    fn parse_tag_ops_basic_add_remove() {
        let ops = parse_tag_ops("+a -b +工作").unwrap();
        assert_eq!(
            ops,
            vec![
                TagOp::Add("a".into()),
                TagOp::Remove("b".into()),
                TagOp::Add("工作".into()),
            ]
        );
    }

    #[test]
    fn parse_tag_ops_dedupes_repeated_op() {
        let ops = parse_tag_ops("+a +a -b -b").unwrap();
        assert_eq!(
            ops,
            vec![TagOp::Add("a".into()), TagOp::Remove("b".into())]
        );
    }

    #[test]
    fn parse_tag_ops_rejects_conflicting_signs() {
        assert!(parse_tag_ops("+a -a").is_err());
        assert!(parse_tag_ops("-x +x").is_err());
    }

    #[test]
    fn parse_tag_ops_rejects_missing_prefix_or_name() {
        assert!(parse_tag_ops("a").is_err()); // 缺前缀
        assert!(parse_tag_ops("+").is_err()); // 缺名
        assert!(parse_tag_ops("-").is_err());
    }

    #[test]
    fn parse_tag_ops_rejects_empty_input() {
        assert!(parse_tag_ops("").is_err());
        assert!(parse_tag_ops("   ").is_err());
    }

    #[test]
    fn parse_tag_ops_rejects_illegal_chars_in_name() {
        // 空格 / 标点等非 tag 字符
        assert!(parse_tag_ops("+a,b").is_err());
        assert!(parse_tag_ops("+a!b").is_err());
    }

    // ---------------- apply_tag_ops ----------------

    #[test]
    fn apply_tag_ops_add_appends_when_absent() {
        let out = apply_tag_ops("[task pri=2] 整理", &[TagOp::Add("organize".into())]);
        assert_eq!(out, "[task pri=2] 整理 #organize");
    }

    #[test]
    fn apply_tag_ops_add_noop_when_already_present() {
        let out = apply_tag_ops(
            "[task pri=2] 整理 #organize",
            &[TagOp::Add("organize".into())],
        );
        assert_eq!(out, "[task pri=2] 整理 #organize");
    }

    #[test]
    fn apply_tag_ops_remove_strips_token_and_leading_space() {
        let out = apply_tag_ops(
            "[task pri=1] 跑步 #weekly #fitness",
            &[TagOp::Remove("weekly".into())],
        );
        // 不该出现双空格
        assert!(!out.contains("  "));
        assert!(!parse_task_tags(&out).iter().any(|t| t == "weekly"));
        assert!(parse_task_tags(&out).iter().any(|t| t == "fitness"));
    }

    #[test]
    fn apply_tag_ops_remove_nonexistent_is_noop() {
        let out = apply_tag_ops("[task pri=1] x #a", &[TagOp::Remove("nonexistent".into())]);
        assert_eq!(parse_task_tags(&out), vec!["a"]);
    }

    #[test]
    fn apply_tag_ops_does_not_strip_substring_match() {
        // remove "tag" 不该误删 #tagged
        let out = apply_tag_ops(
            "[task pri=1] x #tag #tagged",
            &[TagOp::Remove("tag".into())],
        );
        assert_eq!(parse_task_tags(&out), vec!["tagged"]);
    }

    #[test]
    fn apply_tag_ops_chains_multiple_ops() {
        let out = apply_tag_ops(
            "[task pri=1] x #a #b",
            &[
                TagOp::Remove("a".into()),
                TagOp::Add("c".into()),
                TagOp::Add("b".into()), // 已存在 → noop
            ],
        );
        assert_eq!(parse_task_tags(&out), vec!["b", "c"]);
    }

    // ---------------- parse_task_result ----------------

    #[test]
    fn parse_result_extracts_text_after_colon() {
        let r = parse_task_result("[task pri=1] 整理 [done] [result: 把 38 个文件归档到 ~/Archive/]");
        assert_eq!(r.as_deref(), Some("把 38 个文件归档到 ~/Archive/"));
    }

    #[test]
    fn parse_result_supports_chinese_colon() {
        let r = parse_task_result("[result：完成]");
        assert_eq!(r.as_deref(), Some("完成"));
    }

    #[test]
    fn parse_result_returns_none_when_absent() {
        assert!(parse_task_result("[task pri=1] 整理 [done]").is_none());
    }

    #[test]
    fn parse_result_returns_none_when_empty() {
        // [result:] 空内容视作无产物 — 给 LLM 留容错空间
        assert!(parse_task_result("[result:]").is_none());
        assert!(parse_task_result("[result: ]").is_none());
        assert!(parse_task_result("[result:    ]").is_none());
    }

    #[test]
    fn parse_result_takes_first_when_multiple() {
        // 脏数据兜底：取首个，不合并
        let r = parse_task_result("[result: 一] [result: 二]");
        assert_eq!(r.as_deref(), Some("一"));
    }

    // ---------------- strip_result_marker ----------------

    #[test]
    fn strip_result_removes_marker_and_keeps_rest() {
        let desc = "[task pri=2] 整理 [done] [result: 完成]";
        assert_eq!(strip_result_marker(desc), "[task pri=2] 整理 [done]");
    }

    #[test]
    fn strip_result_is_noop_when_absent() {
        let desc = "[task pri=1] 整理 #organize";
        assert_eq!(strip_result_marker(desc), desc);
    }

    // ---------------- compare_for_queue ----------------

    #[test]
    fn cancelled_sorts_after_done() {
        // 结束段内 done 优于 cancelled — 用户开「显示已结束」时希望先看到完成的
        let now = dt(2026, 5, 4, 12, 0);
        let done = view("d", 9, None, TaskStatus::Done, "2026-05-01T00:00");
        let cancelled = view("c", 9, None, TaskStatus::Cancelled, "2026-05-01T00:00");
        assert_eq!(compare_for_queue(&done, &cancelled, now), Ordering::Less);
    }

    #[test]
    fn cancelled_sorts_after_pending_even_with_overdue() {
        // cancelled 是终态，永远不与活动段争位 —— 即便它"过期"也排到最后
        let now = dt(2026, 5, 4, 12, 0);
        let cancelled_overdue = view(
            "c",
            9,
            Some("2026-05-03T08:00"),
            TaskStatus::Cancelled,
            "2026-05-01T00:00",
        );
        let pending_no_due = view("p", 0, None, TaskStatus::Pending, "2026-05-01T00:00");
        assert_eq!(
            compare_for_queue(&pending_no_due, &cancelled_overdue, now),
            Ordering::Less
        );
    }

    #[test]
    fn error_outranks_pending_outranks_done() {
        let now = dt(2026, 5, 4, 12, 0);
        let err = view("e", 0, None, TaskStatus::Error, "2026-05-01T00:00");
        let pen = view("p", 9, None, TaskStatus::Pending, "2026-05-01T00:00");
        let done = view("d", 9, None, TaskStatus::Done, "2026-05-01T00:00");
        assert_eq!(compare_for_queue(&err, &pen, now), Ordering::Less);
        assert_eq!(compare_for_queue(&pen, &done, now), Ordering::Less);
        // 哪怕 pri 与 due 都更友好，done 也不能挤到 pending 前
        let done_high = view(
            "d",
            9,
            Some("2026-05-04T11:00"),
            TaskStatus::Done,
            "2026-05-01T00:00",
        );
        assert_eq!(compare_for_queue(&pen, &done_high, now), Ordering::Less);
    }

    #[test]
    fn overdue_pending_outranks_future_pending_even_with_lower_priority() {
        let now = dt(2026, 5, 4, 12, 0);
        let overdue_low = view(
            "overdue-low",
            1,
            Some("2026-05-04T08:00"),
            TaskStatus::Pending,
            "2026-05-01T00:00",
        );
        let future_high = view(
            "future-hi",
            9,
            Some("2026-05-05T08:00"),
            TaskStatus::Pending,
            "2026-05-01T00:00",
        );
        assert_eq!(
            compare_for_queue(&overdue_low, &future_high, now),
            Ordering::Less
        );
    }

    #[test]
    fn among_overdue_earlier_due_first() {
        let now = dt(2026, 5, 4, 12, 0);
        let a = view(
            "a",
            0,
            Some("2026-05-03T10:00"),
            TaskStatus::Pending,
            "2026-05-02T00:00",
        );
        let b = view(
            "b",
            0,
            Some("2026-05-04T10:00"),
            TaskStatus::Pending,
            "2026-05-02T00:00",
        );
        // a 过期更久 → 排前
        assert_eq!(compare_for_queue(&a, &b, now), Ordering::Less);
    }

    #[test]
    fn higher_priority_wins_among_non_overdue_pending() {
        let now = dt(2026, 5, 4, 12, 0);
        let a = view("a", 5, None, TaskStatus::Pending, "2026-05-01T00:00");
        let b = view("b", 1, None, TaskStatus::Pending, "2026-05-01T00:00");
        assert_eq!(compare_for_queue(&a, &b, now), Ordering::Less);
    }

    #[test]
    fn earlier_due_wins_when_priority_tied_and_not_overdue() {
        let now = dt(2026, 5, 4, 12, 0);
        let a = view(
            "a",
            3,
            Some("2026-05-05T08:00"),
            TaskStatus::Pending,
            "2026-05-01T00:00",
        );
        let b = view(
            "b",
            3,
            Some("2026-05-06T08:00"),
            TaskStatus::Pending,
            "2026-05-01T00:00",
        );
        assert_eq!(compare_for_queue(&a, &b, now), Ordering::Less);
    }

    #[test]
    fn task_with_due_outranks_dueless_at_same_priority() {
        let now = dt(2026, 5, 4, 12, 0);
        let a = view(
            "a",
            2,
            Some("2026-05-10T08:00"),
            TaskStatus::Pending,
            "2026-05-01T00:00",
        );
        let b = view("b", 2, None, TaskStatus::Pending, "2026-05-01T00:00");
        assert_eq!(compare_for_queue(&a, &b, now), Ordering::Less);
    }

    #[test]
    fn created_at_breaks_remaining_ties() {
        let now = dt(2026, 5, 4, 12, 0);
        let older = view("old", 3, None, TaskStatus::Pending, "2026-05-01T00:00");
        let newer = view("new", 3, None, TaskStatus::Pending, "2026-05-03T00:00");
        // 同 pri + 同 due（None） + 同 status → 老任务优先（避免饿死）
        assert_eq!(compare_for_queue(&older, &newer, now), Ordering::Less);
    }
}
