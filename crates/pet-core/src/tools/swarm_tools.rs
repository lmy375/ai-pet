use crate::tools::{required_str, tool_error, Tool, ToolContext};

pub struct SwarmClaimTaskTool;
impl Tool for SwarmClaimTaskTool {
    fn name(&self) -> &str {
        "swarm_claim_task"
    }
    fn definition(&self) -> serde_json::Value {
        serde_json::json!({"type":"function","function":{"name":"swarm_claim_task","description":"Inspect the task already exclusively claimed for this worker. The worker runtime claims tasks before starting your loop.","parameters":{"type":"object","properties":{}}}})
    }
    crate::impl_execute!(swarm_claim_task_impl);
}
async fn swarm_claim_task_impl(_arguments: &str, ctx: &ToolContext) -> String {
    let Some(session) = ctx.swarm.clone() else {
        return tool_error("swarm unavailable");
    };
    let Some(task_id) = ctx.swarm_task_id.as_deref() else {
        return tool_error("only a swarm worker can claim tasks");
    };
    let Some(worker_id) = ctx.swarm_worker_id.as_deref() else {
        return tool_error("worker identity missing");
    };
    let state = session.state.lock().await;
    let Some(task) = state
        .tasks
        .iter()
        .find(|task| task.id == task_id && task.claimed_by.as_deref() == Some(worker_id))
    else {
        return tool_error("this worker has no claimed task");
    };
    serde_json::json!({"status":"claimed","task":task}).to_string()
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
