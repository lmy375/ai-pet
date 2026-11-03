use crate::settings::AppSettings;
use crate::tools::{parse_args, tool_error, Tool, ToolContext, ToolSpec};

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

#[derive(serde::Deserialize)]
struct SwarmUpdateTaskArgs {
    progress: u64,
    summary: String,
}

impl Tool for SwarmUpdateTaskTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "swarm_update_task",
            description: "Update the current task progress. progress is an integer from 0 to 100.",
            schema: serde_json::json!({"type":"object","properties":{"progress":{"type":"integer","minimum":0,"maximum":100},"summary":{"type":"string"}},"required":["progress","summary"]}),
        }
    }
    crate::impl_execute!(swarm_update_task_impl);
}

async fn swarm_update_task_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: SwarmUpdateTaskArgs = match parse_args(arguments) {
        Ok(args) => args,
        Err(e) => return e,
    };
    let Some(session) = ctx.swarm.clone() else {
        return tool_error("swarm unavailable");
    };
    let Some(task_id) = ctx.swarm_task_id.as_deref() else {
        return tool_error("only a worker task can be updated");
    };
    let Some(worker_id) = ctx.swarm_worker_id.as_deref() else {
        return tool_error("worker identity missing");
    };
    let progress = args.progress.min(100) as u8;
    let summary = args.summary.trim().to_string();
    match session
        .update_task(task_id, worker_id, progress, summary)
        .await
    {
        Ok(()) => r#"{"status":"updated"}"#.to_string(),
        Err(e) => tool_error(e),
    }
}

pub struct SwarmCompleteTaskTool;

#[derive(serde::Deserialize)]
struct SwarmCompleteTaskArgs {
    result: String,
    outcome: Option<String>,
}

impl Tool for SwarmCompleteTaskTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "swarm_complete_task",
            description: "Submit the final result for the current task. outcome should be success or failed.",
            schema: serde_json::json!({"type":"object","properties":{"result":{"type":"string"},"outcome":{"type":"string","enum":["success","failed"]}},"required":["result"]}),
        }
    }
    crate::impl_execute!(swarm_complete_task_impl);
}

async fn swarm_complete_task_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: SwarmCompleteTaskArgs = match parse_args(arguments) {
        Ok(args) => args,
        Err(e) => return e,
    };
    let Some(session) = ctx.swarm.clone() else {
        return tool_error("swarm unavailable");
    };
    let Some(task_id) = ctx.swarm_task_id.as_deref() else {
        return tool_error("only a worker task can be completed");
    };
    let Some(worker_id) = ctx.swarm_worker_id.as_deref() else {
        return tool_error("worker identity missing");
    };
    let result = args.result.trim().to_string();
    let outcome = args.outcome.unwrap_or_else(|| "success".to_string());
    if result.is_empty() {
        return tool_error("result is required");
    }
    match session
        .complete_task(task_id, worker_id, result, outcome)
        .await
    {
        Ok(()) => r#"{"status":"completed"}"#.to_string(),
        Err(e) => tool_error(e),
    }
}

pub struct SwarmCreateTaskTool;

#[derive(serde::Deserialize)]
struct SwarmCreateTaskArgs {
    title: String,
    detail: String,
    parent_task_id: Option<String>,
}

impl Tool for SwarmCreateTaskTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "swarm_create_task",
            description: "Create a new pending task for the swarm. Use this when the current task reveals a focused follow-up task.",
            schema: serde_json::json!({"type":"object","properties":{"title":{"type":"string"},"detail":{"type":"string"},"parent_task_id":{"type":"string"}},"required":["title","detail"]}),
        }
    }
    crate::impl_execute!(swarm_create_task_impl);
}

async fn swarm_create_task_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: SwarmCreateTaskArgs = match parse_args(arguments) {
        Ok(args) => args,
        Err(e) => return e,
    };
    let Some(session) = ctx.swarm.clone() else {
        return tool_error("swarm unavailable");
    };
    if args.title.is_empty() {
        return tool_error("missing 'title' parameter");
    }
    if args.detail.is_empty() {
        return tool_error("missing 'detail' parameter");
    }
    match session
        .create_task(args.title, args.detail, args.parent_task_id)
        .await
    {
        Ok(task) => serde_json::json!({"status":"created","task_id":task.id}).to_string(),
        Err(e) => tool_error(e),
    }
}

pub struct SwarmDeleteTaskTool;

#[derive(serde::Deserialize)]
struct SwarmDeleteTaskArgs {
    task_id: String,
}

impl Tool for SwarmDeleteTaskTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "swarm_delete_task",
            description: "Delete a task from the swarm board by its task_id. Use swarm_get_summary to find task ids. Only the Orchestrator can delete tasks.",
            schema: serde_json::json!({"type":"object","properties":{"task_id":{"type":"string"}},"required":["task_id"]}),
        }
    }
    crate::impl_execute!(swarm_delete_task_impl);
}

async fn swarm_delete_task_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: SwarmDeleteTaskArgs = match parse_args(arguments) {
        Ok(args) => args,
        Err(e) => return e,
    };
    let Some(session) = ctx.swarm.clone() else {
        return tool_error("swarm unavailable");
    };
    if ctx.swarm_worker_id.is_some() {
        return tool_error("only the Orchestrator can delete tasks");
    }
    let task_id = args.task_id;
    if task_id.is_empty() {
        return tool_error("missing 'task_id' parameter");
    }
    match session.delete_task(&task_id).await {
        Ok(()) => serde_json::json!({"status":"deleted","task_id":task_id}).to_string(),
        Err(e) => tool_error(e),
    }
}

pub struct SwarmUpdateConfigTool;

#[derive(serde::Deserialize)]
struct SwarmUpdateConfigArgs {
    title: Option<String>,
    main_agent_id: Option<String>,
    pool: Option<Vec<PoolEntry>>,
    shared_prompt: Option<String>,
    orchestrator_prompt: Option<String>,
    worker_prompt: Option<String>,
}

#[derive(serde::Deserialize)]
struct PoolEntry {
    agent_id: String,
    count: u64,
}

impl Tool for SwarmUpdateConfigTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "swarm_update_config",
            description: "Update the current project's settings. Every field is optional; only the fields you send are changed, and calling with no fields returns the current settings. pool replaces the whole Worker roster as [{agent_id, count}]. Changes to pool take effect the next time the project starts; prompt and title changes apply to subsequent turns and tasks.",
            schema: serde_json::json!({"type":"object","properties":{"title":{"type":"string"},"main_agent_id":{"type":"string"},"pool":{"type":"array","items":{"type":"object","properties":{"agent_id":{"type":"string"},"count":{"type":"integer","minimum":0}},"required":["agent_id","count"]}},"shared_prompt":{"type":"string"},"orchestrator_prompt":{"type":"string"},"worker_prompt":{"type":"string"}}}),
        }
    }
    crate::impl_execute!(swarm_update_config_impl);
}

async fn swarm_update_config_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: SwarmUpdateConfigArgs = match parse_args(arguments) {
        Ok(args) => args,
        Err(e) => return e,
    };
    let Some(session) = ctx.swarm.clone() else {
        return tool_error("swarm unavailable");
    };
    if ctx.swarm_worker_id.is_some() {
        return tool_error("only the Orchestrator can update project settings");
    }
    let title = args.title.map(|t| t.trim().to_string());

    let settings = crate::settings::get_settings().ok();
    let main_agent_id = args.main_agent_id;
    if let Some(agent_id) = &main_agent_id {
        if settings.as_ref().and_then(|s| s.agent(agent_id)).is_none() {
            return tool_error(unknown_agent_error(&settings, agent_id));
        }
    }
    let pool = match args.pool {
        None => None,
        Some(entries) => {
            let mut pool = Vec::with_capacity(entries.len());
            for entry in entries {
                let agent_id = entry.agent_id.trim().to_string();
                let count = entry.count.min(u32::MAX as u64) as u32;
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

    let patch = crate::swarm::SwarmConfigPatch {
        title,
        main_agent_id,
        pool,
        shared_prompt: args.shared_prompt,
        orchestrator_prompt: args.orchestrator_prompt,
        worker_prompt: args.worker_prompt,
    };
    match session.update_config(patch).await {
        Ok(config) => serde_json::json!({ "status": "updated", "config": config }).to_string(),
        Err(e) => tool_error(e),
    }
}

pub struct SwarmGetSummaryTool;

#[derive(serde::Deserialize)]
struct SwarmGetSummaryArgs {}

impl Tool for SwarmGetSummaryTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "swarm_get_summary",
            description: "Read the current swarm status and task counts.",
            schema: serde_json::json!({"type":"object","properties":{}}),
        }
    }
    crate::impl_execute!(swarm_get_summary_impl);
}

async fn swarm_get_summary_impl(arguments: &str, ctx: &ToolContext) -> String {
    if let Err(e) = parse_args::<SwarmGetSummaryArgs>(arguments) {
        return e;
    }
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