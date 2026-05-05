# TG /cancel /retry 0 命中带 "你是不是想..." 建议 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG `/cancel` 反馈带最近匹配建议：fuzzy resolve 0 命中时给个 "你是不是想 X / Y"（substring 距离最近的 1-2 个 title）让用户少打几下。

## 目标

`find_task_fuzzy` 的 None 分支当前只回 "找不到任务「X」"。如果用户输入有
typo / 多打了字 / 顺序错乱（"整理D" vs "整理 Downloads"），substring 失败之
后没有任何指引；用户得回 `/help` 或 `/tasks` 重看清单再来一次。

本轮加一个 char-overlap 启发式排名：找与 query 字符集**重合最多**的 1-2 个
title，作为 "你是不是想..." 提示拼回错误反馈。

## 非目标

- 不做 Levenshtein / fuzzy-find 库 —— char-overlap 已能 cover 90% typo /
  顺序错 / 漏字 case，引入编辑距离库会让 "整理"匹配 "学习"（短串距离都小）的
  反直觉 case 出现。
- 不做拼音匹配 —— 用户在 TG 里手键中文 title 不会用拼音输入再输回中文，
  实际场景少。
- 不写 README —— TG 命令体验补强。

## 设计

### Pure ranker

`telegram/commands.rs` 加：

```rust
/// 给 `/cancel` `/retry` 0 命中时的"你是不是想..."建议。返回 query 与
/// 各 title 的字符重合度排序后的 top N（默认 2）；过滤掉 0 重合的 title
/// 避免给完全不相关的建议。pure / 测试好写。
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

/// 0 命中反馈：suggestions 非空时附"你是不是想..."列表；空时回简短"找不到
/// 任务「query」"。文案要让用户能直接复制 / 修改其中一条 title 重新发命令。
pub fn format_no_match_with_suggestions(query: &str, suggestions: &[String]) -> String {
    let q = query.trim();
    if suggestions.is_empty() {
        return format!("找不到任务「{}」", q);
    }
    let bullets: Vec<String> = suggestions.iter().map(|t| format!("• {}", t.trim())).collect();
    format!(
        "找不到任务「{}」。你是不是想：\n{}",
        q,
        bullets.join("\n")
    )
}
```

### bot.rs 接线

`resolve_tg_task_title` 的 None 分支改为：

```rust
FuzzyMatch::None => {
    let suggestions = suggest_titles(query, &titles, 2);
    Err(format_no_match_with_suggestions(query, &suggestions))
}
```

### 测试

- `suggest_titles` 各边界（空 query / n=0 / 0 overlap 全过滤 / 排序按 score
  desc / take n）
- `format_no_match_with_suggestions` 空建议 vs 非空 fallback 文案

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `suggest_titles` + `format_no_match_with_suggestions` + 单测 |
| **M2** | bot.rs `resolve_tg_task_title` None 分支接入 |
| **M3** | cargo test + cleanup |

## 复用清单

- 既有 `find_task_fuzzy` / `FuzzyMatch::None` 触发路径
- 既有 `format_command_error` 包装

## 进度日志

- 2026-05-05 36:00 — 创建本文档；准备 M1。
- 2026-05-05 36:15 — 完成实现：
  - **M1**：`telegram/commands.rs` 加 `suggest_titles(query, titles, n)` pure 函数（HashSet 字符交集 score → desc 排序 → 过滤 0 重合 → take n）+ `format_no_match_with_suggestions(query, suggestions)` 文案 helper（空建议 fallback "找不到任务「X」"）。7 条新增单测覆盖空 query / n=0 / 0 overlap 过滤 / 排序 desc / 中文 / fallback / 完整文案。
  - **M2**：`bot.rs::resolve_tg_task_title` 的 None 分支改为 `suggest_titles(query, &titles, 2)` + `format_no_match_with_suggestions`；其它分支不变。
  - **M3**：`cargo test --lib` 905/905（+7）通过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— TG 命令体验补强。
  - **设计取舍**：char-overlap 而非 Levenshtein —— 实战 typo / 漏字 / 顺序错由 char-overlap cover 90%，且不让 "整理" → "学习"（短串距离小）的反直觉建议出现。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；纯函数 7 条单测覆盖。
