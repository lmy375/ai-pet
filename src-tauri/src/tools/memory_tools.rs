use crate::commands::memory;
use crate::tools::{Tool, ToolContext};

// ---- memory_list ----

pub struct MemoryListTool;

impl Tool for MemoryListTool {
    fn name(&self) -> &str {
        "memory_list"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "memory_list",
                "description": "List memory items from the memory index. Returns titles, descriptions, and detail file paths grouped by category.\n\nCategories:\n- ai_insights: AI 思考与经验\n- user_profile: 用户习惯\n- todo: 用户让你提醒 ta 的事项（reminders for the user — uses [remind: HH:MM] / [remind: YYYY-MM-DD HH:MM] prefix）\n- butler_tasks: 用户委托给你执行的管家任务（things the OWNER asked YOU to do — info gathering, file writes, scheduled reports, recurring chores）\n- general: 其他\n\nTo read the full detail of a memory item, use the read_file tool with the detail_path (relative to ~/.config/pet/memories/).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "category": {
                            "type": "string",
                            "description": "Filter by category name (ai_insights, user_profile, todo, butler_tasks, general). Omit to list all."
                        }
                    }
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(memory_list_impl(arguments, ctx))
    }
}

async fn memory_list_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let category = args["category"].as_str().map(String::from);

    match memory::memory_list(category) {
        Ok(index) => {
            ctx.log("memory_list: returned index");
            serde_json::to_string(&index)
                .unwrap_or_else(|e| format!(r#"{{"error": "serialize failed: {}"}}"#, e))
        }
        Err(e) => format!(r#"{{"error": "{}"}}"#, e),
    }
}

// ---- memory_search ----

pub struct MemorySearchTool;

impl Tool for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "memory_search",
                "description": "Search memory items by keyword. Searches across titles and descriptions in all categories. Returns matching items with their category name.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "keyword": {
                            "type": "string",
                            "description": "The keyword to search for (case-insensitive)"
                        }
                    },
                    "required": ["keyword"]
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(memory_search_impl(arguments, ctx))
    }
}

async fn memory_search_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let keyword = args["keyword"].as_str().unwrap_or("").to_string();

    if keyword.is_empty() {
        return r#"{"error": "missing 'keyword' parameter"}"#.to_string();
    }

    match memory::memory_search(keyword.clone()) {
        Ok(results) => {
            ctx.log(&format!(
                "memory_search: '{}' -> {} results",
                keyword,
                results.len()
            ));
            // Convert to a nicer JSON array
            let items: Vec<serde_json::Value> = results
                .into_iter()
                .map(|(cat, item)| {
                    serde_json::json!({
                        "category": cat,
                        "title": item.title,
                        "description": item.description,
                        "detail_path": item.detail_path,
                        "created_at": item.created_at,
                        "updated_at": item.updated_at,
                    })
                })
                .collect();
            serde_json::json!({ "results": items }).to_string()
        }
        Err(e) => format!(r#"{{"error": "{}"}}"#, e),
    }
}

// ---- memory_edit ----

pub struct MemoryEditTool;

impl Tool for MemoryEditTool {
    fn name(&self) -> &str {
        "memory_edit"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "memory_edit",
                "description": "Create, update, or delete a **memory** item — long-term facts about the owner, your own thinking notes, generic knowledge worth keeping.\n\n- create: Add a new memory item to a category. Provide title, description, and optionally detail_content (written to a .md file).\n- update: Modify an existing item (matched by category + title). Can update description and/or detail_content.\n- delete: Remove an item (matched by category + title) and its .md file.\n\n**Use this tool ONLY for these categories**:\n  - `user_profile` — stable facts about the owner (habits, preferences, work setup)\n  - `ai_insights` — your own thinking / observations / persona summary / daily_plan / daily_review_<date>\n  - `general` — anything else that doesn't fit a more specific tool\n\n**Business state has dedicated tools — memory_edit will REFUSE these categories**:\n  - butler tasks (work the owner delegates to you) → use `butler_task_edit`\n  - reminders (clock-driven nudges for the owner) → use `todo_edit`\n  - task archive (settled tasks ≥ 30 days) — managed automatically by the consolidate loop; not an LLM-writable surface\n\nThe domain split keeps each store narrowly scoped: `memory` is your knowledge / self-portrait layer, while butler_tasks / todo / task_archive live in their own SQLite tables with per-domain validation. Calling memory_edit with one of those categories now returns an error pointing at the right tool.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["create", "update", "delete"],
                            "description": "The action to perform"
                        },
                        "category": {
                            "type": "string",
                            "enum": ["ai_insights", "user_profile", "general"],
                            "description": "The memory domain. butler_tasks / todo / task_archive are no longer accepted here — use the dedicated tools."
                        },
                        "title": {
                            "type": "string",
                            "description": "Memory title (max 20 chars). For update/delete, used to locate the item."
                        },
                        "description": {
                            "type": "string",
                            "description": "Brief description (max 300 chars). Required for create, optional for update."
                        },
                        "detail_content": {
                            "type": "string",
                            "description": "Full content to write to the detail .md file. Optional."
                        }
                    },
                    "required": ["action", "category", "title"]
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(memory_edit_impl(arguments, ctx))
    }
}

async fn memory_edit_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let action = args["action"].as_str().unwrap_or("").to_string();
    let category = args["category"].as_str().unwrap_or("").to_string();
    let title = args["title"].as_str().unwrap_or("").to_string();
    let description = args["description"].as_str().map(String::from);
    let detail_content = args["detail_content"].as_str().map(String::from);

    if action.is_empty() || category.is_empty() || title.is_empty() {
        return r#"{"error": "missing required parameters: action, category, title"}"#.to_string();
    }

    // Reject migrated business-state domains at the LLM surface. The frontend's
    // Tauri `invoke('memory_edit', ...)` path still hits `memory::memory_edit`
    // directly (PanelMemory / PanelTasks editing); only the LLM tool wrapper
    // refuses these categories so the LLM is forced into the dedicated tools.
    if let Some(redirect) = dedicated_redirect_for(&category) {
        let err = serde_json::json!({
            "error": format!(
                "memory_edit refuses category '{category}'. Use the dedicated tool: {redirect}.",
            ),
            "use_tool": redirect,
        });
        return err.to_string();
    }

    match memory::memory_edit(
        action.clone(),
        category.clone(),
        title.clone(),
        description,
        detail_content,
    ) {
        Ok(msg) => {
            ctx.log(&format!(
                "memory_edit: {} '{}' in {}",
                action, title, category
            ));
            serde_json::json!({ "status": "ok", "message": msg }).to_string()
        }
        Err(e) => format!(r#"{{"error": "{}"}}"#, e),
    }
}

/// Return the dedicated-tool name an LLM should use for a migrated business
/// domain, or `None` for memory-native categories. Pure to keep unit tests
/// trivial; the rejection path in `memory_edit_impl` reuses the same lookup.
pub(crate) fn dedicated_redirect_for(category: &str) -> Option<&'static str> {
    match category {
        "butler_tasks" => Some("butler_task_edit"),
        "todo" => Some("todo_edit"),
        // task_archive has no LLM-writable surface — the consolidate loop is
        // the only legitimate writer. Reject + name it explicitly so the LLM
        // gets a useful pointer rather than a generic "unknown" error.
        "task_archive" => Some("(read-only — managed by the consolidate loop)"),
        _ => None,
    }
}

// ---- butler_task_edit （v11，per-domain dedicated tool）
//
// GOAL：「LLM 通过专用工具读写各域，不再共用 memory_edit」。本 tool 是
// memory_edit 在 butler_tasks 这一域的专用 surface —— LLM 用它做"管家任务
// 委托执行"管理。内部仍走 memory::memory_edit + mirror，所以双写 / read
// 路径 / butler_history 记录全部跟随。

pub struct ButlerTaskEditTool;

impl Tool for ButlerTaskEditTool {
    fn name(&self) -> &str {
        "butler_task_edit"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "butler_task_edit",
                "description": "Manage butler tasks — work the owner has asked YOU to perform on their behalf. Distinct from `todo_edit` (which is reminders the owner wants for themselves).\n\n- create: assign a new task. Use when the owner asks you to DO something — info gathering (\"每天早上把日历发给我\"), file work (\"周末整理一下 ~/Downloads\"), scheduled reports, recurring chores. Description records what was asked + how often + last execution status.\n- update: modify an existing task (matched by title). Use when status changes ([done] / [error: ...] / [result: ...] markers), description tweaks, or progress notes.\n- delete: remove a task you (or the owner) have decided to retire.\n\nSchedule prefix in description so the proactive prompt can flag 到期:\n  - `[every: HH:MM] topic` — daily recurring at local HH:MM\n  - `[once: YYYY-MM-DD HH:MM] topic` — single-fire at absolute moment\n  - `[deadline: YYYY-MM-DD HH:MM] topic` — soft deadline (will nag as time approaches)\nNo prefix = \"do this whenever you judge it right\".\n\nMarkers in description (appended over time as you execute):\n  - `[done]` — finished\n  - `[done] [result: ...]` — finished with one-line产出 summary\n  - `[error: 原因]` — failed, may be retried\n  - `[cancelled: 原因]` — owner cancelled (terminal)\n  - `#tag` — categorization\n  - `[blockedBy: title-a, title-b]` — task dependency: this task should NOT be picked until every listed prerequisite is done / cancelled. The proactive prompt block automatically hides blocked tasks until their dependencies resolve, so use this to express \"先做 A 再做 B\" without creating a fragile schedule. Titles must match exactly (case-sensitive). Missing / typo titles are treated as resolved (no permanent dead-lock).\n  - `[snooze: YYYY-MM-DD HH:MM]` — temporal snooze: hide this task from the proactive prompt until the listed moment (local time, minute precision). After the moment passes, the marker auto-expires and the task reappears — no cleanup needed. Use when the owner says \"先放着，下周再说\" / \"今天先不动\" or when you decide a task isn't ripe yet. To re-snooze, append a new `[snooze: ...]` marker — the parser takes the latest one so you don't have to strip the old marker first.\n\nExample: `[every: 09:00] 把今天的日历汇总写到 ~/today.md`\nExample dependency: `[blockedBy: 调研竞品] 写决策文档` — only surfaces after `调研竞品` flips to done / cancelled.\nExample snooze: `[snooze: 2026-05-20 09:00] 等下个 sprint 再启动` — disappears from the prompt until 5/20 9:00.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["create", "update", "delete"],
                            "description": "The action to perform"
                        },
                        "title": {
                            "type": "string",
                            "description": "Task title (max 20 chars). For update/delete, used to locate the task."
                        },
                        "description": {
                            "type": "string",
                            "description": "Brief task description with schedule prefix + status markers (max 300 chars). Required for create, optional for update."
                        },
                        "detail_content": {
                            "type": "string",
                            "description": "Full progress notes / working log to write to detail .md file. Optional. Use for longer reasoning, partial results, intermediate state."
                        }
                    },
                    "required": ["action", "title"]
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(butler_task_edit_impl(arguments, ctx))
    }
}

async fn butler_task_edit_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let action = args["action"].as_str().unwrap_or("").to_string();
    let title = args["title"].as_str().unwrap_or("").to_string();
    let description = args["description"].as_str().map(String::from);
    let detail_content = args["detail_content"].as_str().map(String::from);

    if action.is_empty() || title.is_empty() {
        return r#"{"error": "missing required parameters: action, title"}"#.to_string();
    }

    let logged = action == "update" || action == "delete";
    let desc_for_log = description.clone().unwrap_or_default();

    match memory::memory_edit(
        action.clone(),
        "butler_tasks".to_string(),
        title.clone(),
        description,
        detail_content,
    ) {
        Ok(msg) => {
            ctx.log(&format!("butler_task_edit: {} '{}'", action, title));
            if logged {
                crate::butler_history::record_event(&action, &title, &desc_for_log).await;
            }
            serde_json::json!({ "status": "ok", "message": msg }).to_string()
        }
        Err(e) => format!(r#"{{"error": "{}"}}"#, e),
    }
}

// ---- todo_edit （v11，per-domain dedicated tool）
//
// 与 butler_task_edit 对偶 —— LLM 用此管理"用户给自己的提醒"。

pub struct TodoEditTool;

impl Tool for TodoEditTool {
    fn name(&self) -> &str {
        "todo_edit"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "todo_edit",
                "description": "Manage reminders the user wants for themselves — clock-driven nudges YOU should surface at the right moment. Distinct from `butler_task_edit` (which is work the user wants YOU to perform).\n\n- create: add a new reminder. Description should carry a `[remind: YYYY-MM-DD HH:MM] topic` prefix so the proactive layer can fire it within the 30-minute window.\n- update: rewrite the description (move the time, edit the topic).\n- delete: drop the reminder.\n\nFormats:\n  - `[remind: 2026-05-14 14:00] 客户视频会议`\n  - `[remind: 18:00] 喝水` — implicit today\n\nUse this when the owner says \"提醒我...\" / \"等下记得...\" / 等关键词。Don't use for owner-delegated work — that's `butler_task_edit`.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["create", "update", "delete"],
                            "description": "The action to perform"
                        },
                        "title": {
                            "type": "string",
                            "description": "Short reminder title (max 20 chars). For update/delete, locates the entry."
                        },
                        "description": {
                            "type": "string",
                            "description": "Reminder body with [remind: ...] prefix + topic. Required for create."
                        },
                        "detail_content": {
                            "type": "string",
                            "description": "Optional long-form notes. Most reminders don't need this."
                        }
                    },
                    "required": ["action", "title"]
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(todo_edit_impl(arguments, ctx))
    }
}

async fn todo_edit_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let action = args["action"].as_str().unwrap_or("").to_string();
    let title = args["title"].as_str().unwrap_or("").to_string();
    let description = args["description"].as_str().map(String::from);
    let detail_content = args["detail_content"].as_str().map(String::from);

    if action.is_empty() || title.is_empty() {
        return r#"{"error": "missing required parameters: action, title"}"#.to_string();
    }

    match memory::memory_edit(
        action.clone(),
        "todo".to_string(),
        title.clone(),
        description,
        detail_content,
    ) {
        Ok(msg) => {
            ctx.log(&format!("todo_edit: {} '{}'", action, title));
            serde_json::json!({ "status": "ok", "message": msg }).to_string()
        }
        Err(e) => format!(r#"{{"error": "{}"}}"#, e),
    }
}

#[cfg(test)]
mod tests {
    use super::dedicated_redirect_for;

    /// Migrated domains must redirect; memory-native categories must not.
    /// This is the contract `memory_edit_impl` relies on — if a future
    /// refactor breaks the table, the rejection path silently regresses.
    #[test]
    fn dedicated_redirect_table() {
        assert_eq!(
            dedicated_redirect_for("butler_tasks"),
            Some("butler_task_edit")
        );
        assert_eq!(dedicated_redirect_for("todo"), Some("todo_edit"));
        assert!(
            dedicated_redirect_for("task_archive").is_some(),
            "task_archive must be refused at the LLM surface"
        );
        // memory-native domains stay routable through memory_edit
        assert_eq!(dedicated_redirect_for("ai_insights"), None);
        assert_eq!(dedicated_redirect_for("user_profile"), None);
        assert_eq!(dedicated_redirect_for("general"), None);
        // unknown category falls through (memory::memory_edit will reject
        // it with its own "Unknown category" error — we don't double-up).
        assert_eq!(dedicated_redirect_for(""), None);
        assert_eq!(dedicated_redirect_for("random"), None);
    }
}
