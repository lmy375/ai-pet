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
            serde_json::to_string(&index).unwrap_or_else(|e| {
                format!(r#"{{"error": "serialize failed: {}"}}"#, e)
            })
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
            ctx.log(&format!("memory_search: '{}' -> {} results", keyword, results.len()));
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
                "description": "Create, update, or delete a memory item.\n\n- create: Add a new memory item to a category. Provide title, description, and optionally detail_content (written to a .md file).\n- update: Modify an existing item (matched by category + title). Can update description and/or detail_content.\n- delete: Remove an item (matched by category + title) and its .md file.\n\nCategories: ai_insights, user_profile, todo, butler_tasks, general\n\nUse `butler_tasks` whenever the owner asks you to DO something on their behalf — info gathering (\"每天早上把日历发给我\"), file work (\"周末整理一下 ~/Downloads\"), scheduled reports, or any recurring chore. Description should record what was asked + how often + last execution status. Don't confuse with `todo`: `todo` is reminders the user wants for themselves (\"提醒我 5pm 喝水\"), `butler_tasks` is work the user wants YOU to perform.",
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
                            "enum": ["ai_insights", "user_profile", "todo", "butler_tasks", "general"],
                            "description": "The category"
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

    match memory::memory_edit(action.clone(), category.clone(), title.clone(), description, detail_content) {
        Ok(msg) => {
            ctx.log(&format!("memory_edit: {} '{}' in {}", action, title, category));
            serde_json::json!({ "status": "ok", "message": msg }).to_string()
        }
        Err(e) => format!(r#"{{"error": "{}"}}"#, e),
    }
}
