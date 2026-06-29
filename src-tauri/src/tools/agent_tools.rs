use crate::commands::chat::{run_agent_loop, ImageCollectingSink};
use crate::commands::prompt;
use crate::commands::shell::{run_or_background, TaskKind};
use crate::tools::{required_str, tool_error, Tool, ToolContext};

/// Maximum sub-agent nesting. 1 means the pet can spawn a sub-agent, but a
/// sub-agent cannot spawn further ones. Enforced both here and by withholding
/// the tool from sub-agent registries (see `ToolRegistry::new`).
const MAX_SUBAGENT_DEPTH: usize = 1;

/// How long a foreground sub-agent runs before auto-converting to a background
/// task (mirrors bash's timeout-then-background behavior).
const SUBAGENT_TIMEOUT_MS: u64 = 120_000;

// ---- spawn_subagent ----

pub struct SpawnSubagentTool;

impl Tool for SpawnSubagentTool {
    fn name(&self) -> &str {
        "spawn_subagent"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "spawn_subagent",
                "description": "Launch an autonomous sub-agent to complete a self-contained task. The sub-agent has the same tools you do (bash, file read/write/edit, MCP) and runs its own multi-step loop until done; its final message is returned to you as the result.\n\nUse this to delegate a focused subtask you want handled in one shot — e.g. \"survey the codebase and summarize how X works\", \"find and fix all call sites of Y\". The sub-agent does NOT see this conversation, so the prompt must be detailed and self-contained: state the goal, the relevant paths/context, and exactly what to return. The sub-agent cannot spawn further sub-agents.\n\nLong runs: if the sub-agent doesn't finish quickly it returns a task_id and keeps running in the background; set run_in_background: true to return immediately. Either way you are notified automatically when it finishes — do NOT poll check_task_status.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "prompt": {
                            "type": "string",
                            "description": "The complete, self-contained task for the sub-agent: the goal, any needed context/paths, and what its final answer should contain."
                        },
                        "description": {
                            "type": "string",
                            "description": "Short description of the task in active voice, 3-6 words (e.g. \"Audit error handling\"). Shown to the user as the sub-agent's purpose."
                        },
                        "run_in_background": {
                            "type": "boolean",
                            "description": "If true, return immediately with a task_id without waiting. You'll be notified when the sub-agent finishes."
                        }
                    },
                    "required": ["prompt"]
                }
            }
        })
    }

    crate::impl_execute!(spawn_subagent_impl);
}

async fn spawn_subagent_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args = super::parse_args(arguments);
    let prompt_text = match required_str(&args, "prompt") {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Defense-in-depth: the tool is already withheld from sub-agent registries,
    // but reject explicitly in case it is ever surfaced at depth.
    if ctx.depth >= MAX_SUBAGENT_DEPTH {
        return tool_error("sub-agents cannot spawn further sub-agents");
    }

    let description = args["description"].as_str().unwrap_or("");
    let run_in_background = args["run_in_background"].as_bool().unwrap_or(false);
    let label = if description.is_empty() {
        prompt_text.lines().next().unwrap_or("").to_string()
    } else {
        description.to_string()
    };
    ctx.log(&format!("spawn_subagent (bg={}): {}", run_in_background, label));

    // Build the sub-agent's own conversation: its task as the user message,
    // fronted by the worker-focused system prompt (not the pet persona).
    let mut conv = vec![serde_json::json!({ "role": "user", "content": prompt_text.clone() })];
    prompt::prepend_subagent_system_messages(&mut conv);

    // Owned copies so the work can outlive this call when backgrounded. Runs
    // silently — the sub-agent's internal tool calls don't stream into the
    // parent chat; only its final text comes back (inline or via notification).
    let config = ctx.config.clone();
    let mcp = ctx.mcp_store.clone();
    let child = ctx.child();
    let work = async move {
        let sink = ImageCollectingSink::new();
        match run_agent_loop(conv, &sink, &config, &mcp, &child).await {
            Ok((text, _conv)) => (Some(0), text),
            Err(e) => (Some(1), tool_error(format!("sub-agent failed: {}", e))),
        }
    };

    run_or_background(ctx, TaskKind::Subagent, label, prompt_text, SUBAGENT_TIMEOUT_MS, run_in_background, work).await
}
