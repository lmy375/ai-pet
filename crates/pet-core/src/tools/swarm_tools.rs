use crate::settings::AppSettings;
use crate::tools::{required_str, tool_error, Tool, ToolContext};

/// An invalid agent reference must tell the model which ids it may use, or it
/// has no way to recover from the error.
fn unknown_agent_error(settings: &Option<AppSettings>, agent_id: &str) -> String {
    let available: Vec<&str> = settings
        .as_ref()
        .map(|s| s.agents.iter().map(|a| a.id.as_str()).collect())
        .unwrap_or_default();
    let available = if available.is_empty() {
        "(none)".to_string()
    } else {
        available.join(", ")
    };
    format!("unknown agent_id {agent_id}; available agents: {available}")
}

pub struct SwarmUpdateTaskTool;
impl Tool for SwarmUpdateTaskTool {
    fn name(&self) -> &str {
        "swarm_update_task"
    }
    fn definition(&self) -> serde_json::Value {
        serde_json::json!({"type":"function","function":{"name":"swarm_update_task","description":"Update the current task progress. progress is an integer from 0 to 100.","parameters":{"type":"object","properties":{"progress":{"type":"integer","minimum":0,"maximum":100},"summary":{"type":"string"}},"required":["progress","summary"]}}})
    }
    crate::impl_execute!(swarm_update_task_impl);
}
async fn swarm_update_task_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args = super::parse_args(arguments);
    let Some(session) = ctx.swarm.clone() else {
        return tool_error("swarm unavailable");
    };
    let Some(task_id) = ctx.swarm_task_id.as_deref() else {
        return tool_error("only a worker task can be updated");
    };
    let Some(worker_id) = ctx.swarm_worker_id.as_deref() else {
        return tool_error("worker identity missing");
    };
    let progress = args["progress"].as_u64().unwrap_or(0).min(100) as u8;
    let summary = args["summary"].as_str().unwrap_or("").trim().to_string();
    let state = session.state.lock().await;
    let task = state.tasks.iter().find(|t| t.id == task_id).cloned();
    drop(state);
    let Some(task) = task else {
        return tool_error("task not found");
    };
    if task.claimed_by.as_deref() != Some(worker_id) {
        return tool_error("task lock is owned by another worker");
    }
    // The session's public task mutators are reached through the runtime in the
    // worker loop. A worker tool only needs a small, lock-checked update here.
    let mut state = session.state.lock().await;
    let task = state
        .tasks
        .iter_mut()
        .find(|t| t.id == task_id)
        .expect("task checked above");
    task.progress = progress;
    task.progress_summary = summary;
    task.updated_at = crate::common::iso_now();
    let snapshot = state.clone();
    drop(state);
    // Persisting through the session helper keeps tool updates visible even
    // when no Tauri command is in flight.
    crate::swarm::persist_session(&session, snapshot).await;
    r#"{"status":"updated"}"#.to_string()
}

pub struct SwarmCompleteTaskTool;
impl Tool for SwarmCompleteTaskTool {
    fn name(&self) -> &str {
        "swarm_complete_task"
    }
    fn definition(&self) -> serde_json::Value {
        serde_json::json!({"type":"function","function":{"name":"swarm_complete_task","description":"Submit the final result for the current task. outcome should be success or failed.","parameters":{"type":"object","properties":{"result":{"type":"string"},"outcome":{"type":"string","enum":["success","failed"]}},"required":["result"]}}})
    }
    crate::impl_execute!(swarm_complete_task_impl);
}
async fn swarm_complete_task_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args = super::parse_args(arguments);
    let Some(session) = ctx.swarm.clone() else {
        return tool_error("swarm unavailable");
    };
    let Some(task_id) = ctx.swarm_task_id.as_deref() else {
        return tool_error("only a worker task can be completed");
    };
    let Some(worker_id) = ctx.swarm_worker_id.as_deref() else {
        return tool_error("worker identity missing");
    };
    let result = args["result"].as_str().unwrap_or("").trim().to_string();
    let outcome = args["outcome"].as_str().unwrap_or("success").to_string();
    if result.is_empty() {
        return tool_error("result is required");
    }
    let mut state = session.state.lock().await;
    let Some(task) = state.tasks.iter_mut().find(|t| t.id == task_id) else {
        return tool_error("task not found");
    };
    if task.claimed_by.as_deref() != Some(worker_id) {
        return tool_error("task lock is owned by another worker");
    }
    task.status = crate::swarm::SwarmTaskStatus::Completed;
    task.progress = 100;
    task.result = Some(result);
    task.outcome = Some(outcome);
    task.updated_at = crate::common::iso_now();
    let snapshot = state.clone();
    drop(state);
    crate::swarm::persist_session(&session, snapshot).await;
    r#"{"status":"completed"}"#.to_string()
}

pub struct SwarmCreateTaskTool;
impl Tool for SwarmCreateTaskTool {
    fn name(&self) -> &str {
        "swarm_create_task"
    }
    fn definition(&self) -> serde_json::Value {
        serde_json::json!({"type":"function","function":{"name":"swarm_create_task","description":"Create a new pending task for the swarm. Use this when the current task reveals a focused follow-up task.","parameters":{"type":"object","properties":{"title":{"type":"string"},"detail":{"type":"string"},"parent_task_id":{"type":"string"}},"required":["title","detail"]}}})
    }
    crate::impl_execute!(swarm_create_task_impl);
}
async fn swarm_create_task_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args = super::parse_args(arguments);
    let Some(session) = ctx.swarm.clone() else {
        return tool_error("swarm unavailable");
    };
    let title = match required_str(&args, "title") {
        Ok(v) => v.to_string(),
        Err(e) => return e,
    };
    let detail = match required_str(&args, "detail") {
        Ok(v) => v.to_string(),
        Err(e) => return e,
    };
    let parent = args["parent_task_id"].as_str().map(str::to_string);
    let mut state = session.state.lock().await;
    if state.tasks.len() >= 10_000 {
        return tool_error("task limit reached");
    }
    let task = crate::swarm::SwarmTask {
        id: uuid::Uuid::new_v4().to_string(),
        title: title.trim().to_string(),
        detail: detail.trim().to_string(),
        status: crate::swarm::SwarmTaskStatus::Pending,
        progress: 0,
        progress_summary: String::new(),
        claimed_by: None,
        parent_task_id: parent,
        result: None,
        outcome: None,
        attempt: 0,
        created_at: crate::common::iso_now(),
        updated_at: crate::common::iso_now(),
    };
    if task.title.is_empty() || task.detail.is_empty() {
        return tool_error("title and detail are required");
    }
    state.tasks.push(task.clone());
    let snapshot = state.clone();
    drop(state);
    crate::swarm::persist_session(&session, snapshot).await;
    serde_json::json!({"status":"created","task_id":task.id}).to_string()
}

pub struct SwarmDeleteTaskTool;
impl Tool for SwarmDeleteTaskTool {
    fn name(&self) -> &str {
        "swarm_delete_task"
    }
    fn definition(&self) -> serde_json::Value {
        serde_json::json!({"type":"function","function":{"name":"swarm_delete_task","description":"Delete a task from the swarm board by its task_id. Use swarm_get_summary to find task ids. Only the Orchestrator can delete tasks.","parameters":{"type":"object","properties":{"task_id":{"type":"string"}},"required":["task_id"]}}})
    }
    crate::impl_execute!(swarm_delete_task_impl);
}
async fn swarm_delete_task_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args = super::parse_args(arguments);
    let Some(session) = ctx.swarm.clone() else {
        return tool_error("swarm unavailable");
    };
    if ctx.swarm_worker_id.is_some() {
        return tool_error("only the Orchestrator can delete tasks");
    }
    let task_id = match required_str(&args, "task_id") {
        Ok(v) => v.to_string(),
        Err(e) => return e,
    };
    let mut state = session.state.lock().await;
    let Some(index) = state.tasks.iter().position(|task| task.id == task_id) else {
        return tool_error("task not found");
    };
    state.tasks.remove(index);
    state.updated_at = crate::common::iso_now();
    let snapshot = state.clone();
    drop(state);
    crate::swarm::persist_session(&session, snapshot).await;
    serde_json::json!({"status":"deleted","task_id":task_id}).to_string()
}

pub struct SwarmUpdateConfigTool;
impl Tool for SwarmUpdateConfigTool {
    fn name(&self) -> &str {
        "swarm_update_config"
    }
    fn definition(&self) -> serde_json::Value {
        serde_json::json!({"type":"function","function":{"name":"swarm_update_config","description":"Update the current project's settings. Every field is optional; only the fields you send are changed, and calling with no fields returns the current settings. pool replaces the whole Worker roster as [{agent_id, count}]. Changes to pool take effect the next time the project starts; prompt and title changes apply to subsequent turns and tasks.","parameters":{"type":"object","properties":{"title":{"type":"string"},"main_agent_id":{"type":"string"},"pool":{"type":"array","items":{"type":"object","properties":{"agent_id":{"type":"string"},"count":{"type":"integer","minimum":0}},"required":["agent_id","count"]}},"shared_prompt":{"type":"string"},"orchestrator_prompt":{"type":"string"},"worker_prompt":{"type":"string"}}}}})
    }
    crate::impl_execute!(swarm_update_config_impl);
}
async fn swarm_update_config_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args = super::parse_args(arguments);
    let Some(session) = ctx.swarm.clone() else {
        return tool_error("swarm unavailable");
    };
    if ctx.swarm_worker_id.is_some() {
        return tool_error("only the Orchestrator can update project settings");
    }
    let title = args["title"].as_str().map(|t| t.trim().to_string());
    if let Some(title) = &title {
        if title.is_empty() {
            return tool_error("title cannot be empty");
        }
    }
    let settings = crate::settings::get_settings().ok();
    let main_agent_id = args["main_agent_id"].as_str().map(str::to_string);
    if let Some(agent_id) = &main_agent_id {
        if settings.as_ref().and_then(|s| s.agent(agent_id)).is_none() {
            return tool_error(unknown_agent_error(&settings, agent_id));
        }
    }
    let pool = match args.get("pool").filter(|value| !value.is_null()) {
        None => None,
        Some(value) => {
            let Some(items) = value.as_array() else {
                return tool_error("pool must be an array of {agent_id, count}");
            };
            let mut pool = Vec::with_capacity(items.len());
            for item in items {
                let agent_id = item["agent_id"].as_str().unwrap_or("").trim().to_string();
                let Some(count) = item["count"].as_u64() else {
                    return tool_error("pool entries need an integer count");
                };
                let count = count.min(u32::MAX as u64) as u32;
                if agent_id.is_empty() {
                    return tool_error("pool entries need a non-empty agent_id");
                }
                if settings.as_ref().and_then(|s| s.agent(&agent_id)).is_none() {
                    return tool_error(unknown_agent_error(&settings, &agent_id));
                }
                if count > 0 {
                    pool.push(crate::swarm::WorkerProfile { agent_id, count });
                }
            }
            Some(pool)
        }
    };
    let shared_prompt = args["shared_prompt"].as_str().map(str::to_string);
    let orchestrator_prompt = args["orchestrator_prompt"].as_str().map(str::to_string);
    let worker_prompt = args["worker_prompt"].as_str().map(str::to_string);

    let mut state = session.state.lock().await;
    if let Some(title) = title {
        state.title = title;
    }
    if let Some(agent_id) = main_agent_id {
        state.main_agent_id = agent_id;
    }
    if let Some(pool) = pool {
        state.pool = pool;
    }
    if let Some(prompt) = shared_prompt {
        state.shared_prompt = prompt;
    }
    if let Some(prompt) = orchestrator_prompt {
        state.orchestrator_prompt = prompt;
    }
    if let Some(prompt) = worker_prompt {
        state.worker_prompt = prompt;
    }
    state.updated_at = crate::common::iso_now();
    let snapshot = state.clone();
    drop(state);
    let config = serde_json::json!({
        "title": &snapshot.title,
        "main_agent_id": &snapshot.main_agent_id,
        "pool": &snapshot.pool,
        "shared_prompt": &snapshot.shared_prompt,
        "orchestrator_prompt": &snapshot.orchestrator_prompt,
        "worker_prompt": &snapshot.worker_prompt,
    });
    crate::swarm::persist_session(&session, snapshot).await;
    serde_json::json!({ "status": "updated", "config": config }).to_string()
}

pub struct SwarmGetSummaryTool;
impl Tool for SwarmGetSummaryTool {
    fn name(&self) -> &str {
        "swarm_get_summary"
    }
    fn definition(&self) -> serde_json::Value {
        serde_json::json!({"type":"function","function":{"name":"swarm_get_summary","description":"Read the current swarm status and task counts.","parameters":{"type":"object","properties":{}}}})
    }
    crate::impl_execute!(swarm_get_summary_impl);
}
async fn swarm_get_summary_impl(_arguments: &str, ctx: &ToolContext) -> String {
    let Some(session) = ctx.swarm.clone() else {
        return tool_error("swarm unavailable");
    };
    let state = session.state.lock().await;
    let pending = state
        .tasks
        .iter()
        .filter(|t| t.status == crate::swarm::SwarmTaskStatus::Pending)
        .count();
    let running = state
        .tasks
        .iter()
        .filter(|t| t.status == crate::swarm::SwarmTaskStatus::Running)
        .count();
    let completed = state
        .tasks
        .iter()
        .filter(|t| t.status == crate::swarm::SwarmTaskStatus::Completed)
        .count();
    let tasks: Vec<_> = state
        .tasks
        .iter()
        .filter(|task| task.status != crate::swarm::SwarmTaskStatus::Completed || task.result.is_some())
        .map(|task| {
            serde_json::json!({
                "id": task.id,
                "title": task.title,
                "status": task.status,
                "progress": task.progress,
                "result": task.result.as_deref().map(|result| result.chars().take(2000).collect::<String>()),
            })
        })
        .collect();
    serde_json::json!({"status":state.status,"pending":pending,"running":running,"completed":completed,"total":state.tasks.len(),"tasks":tasks}).to_string()
}
