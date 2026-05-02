//! Periodic memory consolidation.
//!
//! Spawns a long-period background loop that occasionally asks the LLM to look over the
//! pet's memory index and clean it up: merge duplicates, summarize stale entries, drop
//! trivial ones. The model uses the existing `memory_edit` tool to perform changes — Rust
//! never modifies memories directly. Default disabled and gated behind a minimum item count
//! so we don't spend tokens consolidating an empty index.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::commands::chat::{run_chat_pipeline, ChatDonePayload, ChatMessage, CollectingSink};
use crate::commands::debug::{write_log, LogStore};
use crate::commands::memory;
use crate::commands::settings::get_settings;
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::mcp::McpManagerStore;
use crate::mood::{read_current_mood, read_mood_for_event};
use crate::tools::ToolContext;

/// Spawn the memory consolidation loop. Reads settings each tick so the user can toggle
/// at runtime. Sleeps for `interval_hours` between attempts.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Delay first run a bit so app startup doesn't include LLM calls.
        tokio::time::sleep(Duration::from_secs(120)).await;

        loop {
            let settings = match get_settings() {
                Ok(s) => s,
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                    continue;
                }
            };

            let cfg = &settings.memory_consolidate;
            let interval_secs = cfg.interval_hours.max(1) * 3600;

            if !cfg.enabled {
                tokio::time::sleep(Duration::from_secs(interval_secs)).await;
                continue;
            }

            let log_store = app.state::<LogStore>().inner().clone();
            let total = total_memory_items();

            if total < cfg.min_total_items {
                write_log(
                    &log_store.0,
                    &format!(
                        "Consolidate: skip — only {} items (min {})",
                        total, cfg.min_total_items
                    ),
                );
            } else if let Err(e) = run_consolidation(&app, total).await {
                eprintln!("Consolidate turn failed: {}", e);
            }

            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
        }
    });
}

/// Build the consolidation prompt, run it through the chat pipeline so the LLM can call
/// `memory_edit`, and log a before/after item count.
async fn run_consolidation(app: &AppHandle, total_before: usize) -> Result<(), String> {
    let config = AiConfig::from_settings()?;
    let mcp_store = app.state::<McpManagerStore>().inner().clone();
    let log_store = app.state::<LogStore>().inner().clone();
    let shell_store = app.state::<ShellStore>().inner().clone();
    let ctx = ToolContext::new(log_store.clone(), shell_store);

    let index = memory::memory_list(None).map_err(|e| format!("memory_list failed: {e}"))?;
    let index_json = serde_json::to_string_pretty(&index)
        .map_err(|e| format!("serialize index: {e}"))?;

    let prompt = format!(
        "[系统提示·记忆整理]\n\n\
作为 AI 桌面宠物，你正在做后台记忆维护——这次没有用户互动，只是回顾一下你存的记忆。\n\n\
当前记忆索引（共 {total} 条）：\n\n```yaml\n{index}\n```\n\n\
请扫一遍这些条目，判断：\n\
1. **重复/同主题**：把内容相近的合并成一条更精炼的——保留信息量大的，用 `memory_edit update` 更新；用 `memory_edit delete` 删掉冗余的。\n\
2. **过期/失效**：明显过时（已完成的 todo、不再相关的临时上下文），用 `memory_edit delete`。\n\
3. **太琐碎**：完全没有保留价值的（例如随口一句话被记下），删除。\n\
4. **可以补充细节**：如果某条记忆 description 太短、可以扩展但需要查更多上下文，可以用 `memory_edit update` 加入更完整的 detail_content。\n\n\
**特殊保护**：`ai_insights/current_mood` 是宠物当前的心情状态，绝对不要删除——可以适当 update 让 description 更准确，但务必保留这条记录、且 description 必须以 `[motion: Tap|Flick|Flick3|Idle] 心情文字` 开头格式。\n\n\
原则：**保守**。如果不确定一条记忆是否还有价值，就保留。**不要为了整理而整理**——如果索引看起来已经清爽，就什么都不做并输出 `<noop>`。\n\n\
工作完成后，简短总结你做了什么（合并了几条 / 删了几条 / 没改动）。不需要客气，只要事实。",
        total = total_before,
        index = index_json,
    );

    let messages: Vec<ChatMessage> = vec![
        serde_json::from_value(serde_json::json!({
            "role": "system",
            "content": "你是一个记忆整理助理。可以并应当使用 memory_edit 工具直接修改记忆。",
        })).unwrap(),
        serde_json::from_value(serde_json::json!({
            "role": "user",
            "content": prompt,
        })).unwrap(),
    ];

    let mood_before = read_current_mood();

    let sink = CollectingSink::new();
    let summary = run_chat_pipeline(messages, &sink, &config, &mcp_store, &ctx).await?;

    let total_after = total_memory_items();
    write_log(
        &log_store.0,
        &format!(
            "Consolidate: done — {} -> {} items. Summary: {}",
            total_before,
            total_after,
            summary.trim().chars().take(200).collect::<String>()
        ),
    );

    // Re-read mood for the post-consolidation snapshot. If consolidation merged or refined
    // the mood entry, we want the desktop pet's Live2D motion to reflect it.
    let (mood, motion) = read_mood_for_event(&log_store, "Consolidate");
    if mood_before.is_some() && mood.is_none() {
        // Despite the explicit prompt protection, the LLM removed the mood entry. Worth
        // surfacing — repeated occurrences mean the protection text needs hardening.
        write_log(
            &log_store.0,
            "Consolidate: WARNING — current_mood entry was removed despite protection rule",
        );
    }
    let payload = ChatDonePayload {
        mood,
        motion,
        timestamp: chrono::Local::now()
            .format("%Y-%m-%dT%H:%M:%S%.3f")
            .to_string(),
    };
    let _ = app.emit("chat-done", payload);

    Ok(())
}

/// Sum item counts across all memory categories.
fn total_memory_items() -> usize {
    match memory::memory_list(None) {
        Ok(idx) => idx.categories.values().map(|c| c.items.len()).sum(),
        Err(_) => 0,
    }
}
