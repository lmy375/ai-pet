pub fn format_command_success(kind: &str, title: &str) -> String {
    let title = title.trim();
    match kind {
        "cancel" => format!(
            "🚫 已取消「{}」\n如需恢复发 /retry {}",
            title, title
        ),
        "retry" => format!(
            "🔄 已重置「{}」回 pending，下一轮宠物会重新尝试\n如需取消发 /cancel {}",
            title, title
        ),
        "done" => format!(
            "✓ 已标 done「{}」\n想加 result 摘要请回桌面板「✓ 标 done」按钮（TG 暂只支持空 result 路径）",
            title
        ),
        _ => format!("✅ 「{}」 已处理", title),
    }
}

/// 命令失败反馈文案。err 是底层 task_cancel / task_retry 返回的字符串。
pub fn format_command_error(err: &str) -> String {
    format!("⚠️ 操作失败：{}", err)
}

/// `/task <title>` 成功反馈。强调"已入队 + 实际 P 档 + 怎么调细节"，让用户
/// 一眼知道这条命令做了什么、想精细化怎么走。/cancel 的 hint 用来自洽
/// "误派 → 一键撤"。
pub fn format_task_created_success(title: &str, priority: u8) -> String {
    let t = title.trim();
    format!(
        "✅ 已加到队列「{}」(P{})\n用 /tasks 查看，/cancel {} 撤回；想调截止时间请回桌面板",
        t, priority, t
    )
}

/// pure：byte-level Levenshtein 编辑距离。命令名都是 ascii lowercase，
/// 不需 unicode-aware。标准 DP 但只保留两行 → O(min(a,b)) 空间；命令名
/// 都 ≤ 8 字符性能不是事，主要保证代码清晰。
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1) // deletion
                .min(curr[j - 1] + 1) // insertion
                .min(prev[j - 1] + cost); // substitution
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// 距离阈值：≤ 2 的 typo 才提示。3+ 通常已是另一个意思而非笔误。
pub const SUGGEST_MAX_DISTANCE: usize = 2;

/// pure：从 valid 里找与 unknown 编辑距离 ≤ `SUGGEST_MAX_DISTANCE` 的
/// 最近命令；返回 Some 仅当唯一最近。距离相同时取 valid 数组首个（让
/// 调用方按"高频在前"自然控制歧义解析顺序）。
///
/// `unknown` 空 / `valid` 空 → None。
pub fn suggest_command<'a>(unknown: &str, valid: &[&'a str]) -> Option<&'a str> {
    if unknown.is_empty() || valid.is_empty() {
        return None;
    }
    let mut best: Option<(usize, &'a str)> = None;
    for &name in valid {
        let d = levenshtein(unknown, name);
        if d > SUGGEST_MAX_DISTANCE {
            continue;
        }
        match best {
            None => best = Some((d, name)),
            Some((bd, _)) if d < bd => best = Some((d, name)),
            // 距离相同保留首个（valid 顺序优先）
            _ => {}
        }
    }
    best.map(|(_, n)| n)
}

/// 未知命令的回复。指向 `/help` 让用户获得完整列表，避免在多处重复列举命令矩阵。
/// `suggestion` 非空时把建议放第一行 —— TG 客户端通知预览常只显首行，
/// "你是想发 /xx 吗？" 比 "未知命令" 更有用。
pub fn format_unknown_command(name: &str, suggestion: Option<&str>) -> String {
    match suggestion {
        Some(sug) => format!(
            "你是不是想发 /{} 吗？\n未知命令 /{}。输入 /help 查看可用命令。",
            sug, name,
        ),
        None => format!("未知命令 /{}。输入 /help 查看可用命令。", name),
    }
}

/// 命令缺参数的回复（如 /cancel 后面什么都没有）。
pub fn format_missing_argument(name: &str) -> String {
    format!("用法：/{} <任务标题>", name)
}

/// `/cancel` `/retry` 的 fuzzy title resolution 结果。优先级：Exact >
/// Single > Ambiguous > None。多命中保留全部候选（IO 层做 head-N 截断）。
#[derive(Debug, PartialEq, Eq)]
pub enum FuzzyMatch {
    Exact(String),
    Single(String),
    None,
    Ambiguous(Vec<String>),
}

const FUZZY_AMBIGUOUS_PREVIEW_MAX: usize = 5;

/// pure：在 titles 里找与 query 匹配的 task title。trim query 与每条 title。
/// 1. 先找 trim 后字面相等的 title → Exact
/// 2. 否则 case-insensitive substring：collect 全部命中 →
///    - 0 → None
///    - 1 → Single
///    - >1 → Ambiguous（保留全部候选，IO 层 format 时再截断到 N 条预览）
/// 空 query 一律 None（避免空字符串 substring 命中所有 title）。
pub fn find_task_fuzzy(query: &str, titles: &[String]) -> FuzzyMatch {
    let q = query.trim();
    if q.is_empty() {
        return FuzzyMatch::None;
    }
    if let Some(t) = titles.iter().find(|t| t.trim() == q) {
        return FuzzyMatch::Exact(t.clone());
    }
    let q_lower = q.to_lowercase();
    let matches: Vec<String> = titles
        .iter()
        .filter(|t| t.to_lowercase().contains(&q_lower))
        .cloned()
        .collect();
    match matches.len() {
        0 => FuzzyMatch::None,
        1 => FuzzyMatch::Single(matches.into_iter().next().unwrap()),
        _ => FuzzyMatch::Ambiguous(matches),
    }
}

/// pure：把 1-indexed 整数 query 解析为 titles 中对应位置的 title。让
/// `/cancel 1` `/retry 2` 等价于"上次 /tasks 输出第 N 条"，避免键入长 title。
/// query trim 后非纯数字 / 数字 0 / 越界 → None，让 caller fall back 到 fuzzy
/// resolve；非 None 时返回 owned String 直接给 cancel/retry inner 用。
pub fn resolve_index_to_title(query: &str, titles: &[String]) -> Option<String> {
    let n: usize = query.trim().parse().ok()?;
    if n == 0 {
        return None;
    }
    titles.get(n - 1).cloned()
}

/// pure：给 `/cancel` `/retry` 0 命中时的"你是不是想…"建议排名。基于 query
/// 与各 title 的字符重合度（HashSet 交集大小）排序，取 top N。0 重合的 title
/// 过滤掉避免给完全不相关的建议。
///
/// char-overlap vs Levenshtein：前者 cover 90% 实战 typo / 漏字 / 顺序错
/// case，且不会让 "整理" → "学习"（短串距离小）的反直觉建议出现。
pub fn suggest_titles(query: &str, titles: &[String], n: usize) -> Vec<String> {
    let q = query.trim().to_lowercase();
    if q.is_empty() || n == 0 {
        return Vec::new();
    }
    let q_chars: std::collections::HashSet<char> = q.chars().collect();
    let mut scored: Vec<(String, usize)> = titles
        .iter()
        .map(|t| {
            let t_chars: std::collections::HashSet<char> = t.to_lowercase().chars().collect();
            let common = q_chars.intersection(&t_chars).count();
            (t.clone(), common)
        })
        .filter(|(_, score)| *score > 0)
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored.into_iter().take(n).map(|(t, _)| t).collect()
}

/// pure：0 命中反馈文案。suggestions 非空时附"你是不是想…"列表；空时回简短
/// "找不到任务「query」"。bullet 文案要让用户能直接复制 / 修改其中一条 title
/// 重发命令。
pub fn format_no_match_with_suggestions(query: &str, suggestions: &[String]) -> String {
    let q = query.trim();
    if suggestions.is_empty() {
        return format!("找不到任务「{}」", q);
    }
    let bullets: Vec<String> = suggestions
        .iter()
        .map(|t| format!("• {}", t.trim()))
        .collect();
    format!(
        "找不到任务「{}」。你是不是想：\n{}",
        q,
        bullets.join("\n")
    )
}

/// pure：渲染"多个任务都包含 query"反馈。最多 `FUZZY_AMBIGUOUS_PREVIEW_MAX`
/// 条 bullet；超出截断 + "…等 N 条" 提示，避免长列表刷屏。
pub fn format_ambiguous_match(query: &str, candidates: &[String]) -> String {
    let total = candidates.len();
    let preview: Vec<String> = candidates
        .iter()
        .take(FUZZY_AMBIGUOUS_PREVIEW_MAX)
        .map(|t| format!("• {}", t.trim()))
        .collect();
    let mut out = format!("「{}」匹配多个任务：\n{}", query.trim(), preview.join("\n"));
    if total > FUZZY_AMBIGUOUS_PREVIEW_MAX {
        out.push_str(&format!(
            "\n…等 {} 条；请用更精确的标题再试。",
            total
        ));
    } else {
        out.push_str("\n请用更精确的标题再试。");
    }
    out
}
