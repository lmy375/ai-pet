use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri::Manager;
use teloxide::dispatching::ShutdownToken;
use teloxide::prelude::*;
use teloxide::types::{BotCommand, ChatAction, Me};
use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;

use crate::commands::chat::{
    inject_deadline_context_layer, inject_focus_context_layer, inject_mood_note,
    inject_persona_layer, run_chat_pipeline, trim_to_context, ChatDonePayload, ChatMessage,
    CollectingSink,
};
use crate::commands::debug::{LogStore, ProcessCountersStore};
use crate::commands::session;
use crate::commands::settings::{get_soul, TelegramConfig};
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::mcp::McpManagerStore;
use crate::mood::read_mood_for_event;
use crate::tools::ToolContext;

/// A running Telegram bot instance.
pub struct TelegramBot {
    shutdown_token: ShutdownToken,
    /// 任务回传 watcher 的句柄。stop() 时一并 abort，避免 watcher 在
    /// dispatcher 关闭后还在用旧 bot 句柄发消息。
    watcher_handle: Option<JoinHandle<()>>,
}

/// Persistent state shared across message handlers.
struct HandlerState {
    /// 允许与 bot 对话的 TG username 白名单（lowercase，无 `@` 前缀）。
    /// 来自 settings.allowed_username，按 `,` 分隔；空 Vec 视为"全开放"
    /// （与之前 String 为空的兼容语义）。
    allowed_usernames: Vec<String>,
    /// Whether the bot's chat pipeline injects the route-A persona layer (Iter 107).
    /// Captured at bot start time from `TelegramConfig.persona_layer_enabled`. Bot
    /// must be restarted for changes to take effect — same lifecycle as bot_token.
    persona_layer_enabled: bool,
    mcp_store: McpManagerStore,
    log_store: LogStore,
    shell_store: ShellStore,
    process_counters: ProcessCountersStore,
    /// Messages for the dedicated Telegram session (kept in memory for fast access).
    session_messages: TokioMutex<Vec<serde_json::Value>>,
    session_id: String,
    /// Used to emit `chat-done` events so the desktop pet's Live2D motion reacts even
    /// when the conversation happened on Telegram.
    app: AppHandle,
    /// `/tasks` 去重缓存：上次 `/tasks` 命令在该 chat 发出的完整 body。命中
    /// 命令时新 body 与缓存相等 → 回简短"无变化"文案。bot 进程内有效，重启
    /// 即清（与"重启 = 全新一次"语义一致）。
    last_tasks_response: TokioMutex<HashMap<i64, String>>,
    /// `/tasks` 显示顺序的 title 列表缓存：让 `/cancel N` `/retry N` 把整数
    /// 当 1-indexed 序号解析。每次 /tasks 触发都更新（不论 body dedup 是否
    /// 命中），保证序号始终对应"用户最近一次看到的列表顺序"。
    last_tasks_titles: TokioMutex<HashMap<i64, Vec<String>>>,
    /// 用户在 settings.telegram.custom_commands 配置过的命令名（已通过
    /// `merged_command_registry` 同款过滤，全 lowercase / 合法字符）。当
    /// 用户在 TG 输 `/{name} ...` 命中本列表 → 不走 command dispatch，
    /// fall through 到 chat pipeline 让 LLM 自由处理。bot 启动时填充，
    /// settings 改后需 reconnect 才生效。
    custom_command_names: Vec<String>,
    /// custom 命令的完整 (name, description) 列表，用于 `/help` 文案末
    /// 尾自动列出 —— 让用户配完忘了输的命令名能从 /help 找回。规则同
    /// `custom_command_names`：合并过滤后的版本。
    custom_command_objects: Vec<crate::commands::settings::TgCustomCommand>,
}

/// 任务回传 watcher 跟踪的最近一轮快照：title → status。读 butler_tasks
/// → 比对差异 → 终态新出现 → 通过 bot 推消息回原 chat。`Mutex` 而非
/// `TokioMutex` 因为只在 watcher 线程独占访问，sync 锁更省。
type TaskSnapshot = HashMap<String, crate::task_queue::TaskStatus>;

const TELEGRAM_SESSION_ID: &str = "telegram-bot";
const TELEGRAM_MSG_LIMIT: usize = 4096;

/// 任务回传 watcher 的轮询间隔。60s 在"用户感知及时性"与"避免空转 IO"
/// 之间的折中。任务通常分钟级别完成，不需要更密。
const WATCHER_INTERVAL_SECS: u64 = 60;

impl TelegramBot {
    pub async fn start(
        config: TelegramConfig,
        mcp_store: McpManagerStore,
        log_store: LogStore,
        shell_store: ShellStore,
        process_counters: ProcessCountersStore,
        app: AppHandle,
        warnings: crate::telegram::warnings::TgStartupWarningStore,
    ) -> Result<Self, String> {
        let bot = Bot::new(&config.bot_token);

        // Verify bot token by calling getMe
        let _me: Me = bot
            .get_me()
            .await
            .map_err(|e| format!("Telegram bot auth failed: {}", e))?;

        // 注册命令清单：让 TG 客户端在用户输 `/` 时弹出补全候选。装饰性
        // API，失败 log 即可不阻断启动 —— 命令本身仍在 parse_tg_command 里
        // 工作，只是用户得记住或翻 /help。
        let cmds: Vec<BotCommand> =
            crate::telegram::commands::merged_command_registry(&config.custom_commands, &config.command_lang)
                .into_iter()
                .map(|(name, desc)| BotCommand::new(name, desc))
                .collect();
        match bot.set_my_commands(cmds).await {
            Ok(_) => eprintln!("Telegram commands registered for autocomplete"),
            Err(e) => {
                let msg = e.to_string();
                eprintln!("set_my_commands failed (non-fatal): {}", msg);
                crate::telegram::warnings::push(&warnings, "set_my_commands", msg);
            }
        }

        // Load or create the dedicated Telegram session
        let (session_id, messages) = load_or_create_session();

        // 提取 custom 命令的 (name, description) 完整列表（已合并/过滤过）—
        // 用 merged_command_registry 拿到全集再剥掉 hardcoded，剩下的就是
        // 合法 custom 条目。单一过滤源避免 drift。同时派生一个 names-only
        // 列表给 handle_message 的 fast-path scan 用。
        let hardcoded_names: std::collections::HashSet<&str> =
            crate::telegram::commands::tg_command_registry()
                .into_iter()
                .map(|(n, _)| n)
                .collect();
        let custom_command_objects: Vec<crate::commands::settings::TgCustomCommand> =
            crate::telegram::commands::merged_command_registry(&config.custom_commands, &config.command_lang)
                .into_iter()
                .filter_map(|(name, description)| {
                    if hardcoded_names.contains(name.as_str()) {
                        None
                    } else {
                        Some(crate::commands::settings::TgCustomCommand {
                            name,
                            description,
                        })
                    }
                })
                .collect();
        let custom_command_names: Vec<String> = custom_command_objects
            .iter()
            .map(|c| c.name.clone())
            .collect();

        let state = Arc::new(HandlerState {
            allowed_usernames: crate::telegram::commands::parse_allowed_usernames(
                &config.allowed_username,
            ),
            persona_layer_enabled: config.persona_layer_enabled,
            mcp_store,
            log_store,
            shell_store,
            process_counters,
            session_messages: TokioMutex::new(messages),
            session_id,
            app,
            last_tasks_response: TokioMutex::new(HashMap::new()),
            last_tasks_titles: TokioMutex::new(HashMap::new()),
            custom_command_names,
            custom_command_objects,
        });

        let handler = Update::filter_message()
            .filter_map(|msg: Message| msg.text().map(|t| t.to_string()))
            .endpoint(handle_message);

        // Bot 内部已是 Arc 共享 client；clone 仅复制句柄，无 IO 成本。
        // watcher_bot 在 dispatcher 之前 clone，保证 dispatcher 拿到原句柄。
        let watcher_bot = bot.clone();
        let mut dispatcher = Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![state])
            .enable_ctrlc_handler()
            .build();

        let shutdown_token = dispatcher.shutdown_token();

        tokio::spawn(async move {
            dispatcher.dispatch().await;
        });

        // 任务回传 watcher：以 60s 为周期扫描 butler_tasks，把通过 TG 派
        // 单的任务的状态翻动通知回原会话。冷启动首轮静默（只填充快照），
        // 避免重启后把所有"已完成"任务再轰炸一遍。
        let watcher_handle = tokio::spawn(async move {
            run_task_watcher(watcher_bot).await;
        });

        Ok(Self {
            shutdown_token,
            watcher_handle: Some(watcher_handle),
        })
    }

    pub fn stop(&self) {
        if let Some(h) = self.watcher_handle.as_ref() {
            h.abort();
        }
        // ShutdownToken::shutdown() returns a future, but we just need to signal
        // the shutdown — spawning it is sufficient.
        let token = self.shutdown_token.clone();
        tokio::spawn(async move {
            token
                .shutdown()
                .expect("Failed to shutdown Telegram bot")
                .await;
        });
    }
}

/// Load the dedicated Telegram session, or create one if it doesn't exist.
fn load_or_create_session() -> (String, Vec<serde_json::Value>) {
    match session::load_session(TELEGRAM_SESSION_ID.to_string()) {
        Ok(s) => (s.id, s.messages),
        Err(_) => {
            let soul = get_soul().unwrap_or_default();
            let system_msg = serde_json::json!({ "role": "system", "content": soul });
            let messages = vec![system_msg];

            let now = chrono::Local::now()
                .format("%Y-%m-%dT%H:%M:%S%.3f")
                .to_string();
            let s = session::Session {
                id: TELEGRAM_SESSION_ID.to_string(),
                title: "Telegram".to_string(),
                created_at: now.clone(),
                updated_at: now,
                messages: messages.clone(),
                items: vec![],
            };
            let _ = session::save_session(s);
            (TELEGRAM_SESSION_ID.to_string(), messages)
        }
    }
}

async fn handle_message(
    bot: Bot,
    msg: Message,
    text: String,
    state: Arc<HandlerState>,
) -> ResponseResult<()> {
    // Check allowed username
    let username = msg
        .from
        .as_ref()
        .and_then(|u| u.username.as_ref())
        .map(|u| u.to_lowercase())
        .unwrap_or_default();

    // 空白名单 = 任何人都允许（默认 / 兼容旧空字符串语义）；非空时必须命中。
    if !state.allowed_usernames.is_empty()
        && !state.allowed_usernames.iter().any(|u| u == &username)
    {
        bot.send_message(
            msg.chat.id,
            "Sorry, you are not authorized to chat with me.",
        )
        .await?;
        return Ok(());
    }

    // 命令分流：以 `/` 开头的消息走命令调度而不是 chat pipeline。直接
    // 复用现有 Tauri 命令（task_cancel / task_retry），共用 decision_log
    // 与 butler_history 路径，与桌面面板上的"取消 / 重试"语义一致。
    //
    // 自定义命令例外：用户在 settings 注册的 custom 命令命中时**不**走
    // dispatch，fall through 到 chat pipeline 让 LLM 把 `/{name} <args>`
    // 当文本看待 + 自由选 tool；不绑定具体 tool 映射（让 LLM 当中介）。
    let is_custom_cmd = text
        .strip_prefix('/')
        .and_then(|rest| rest.split_whitespace().next())
        .map(|head| {
            let lower = head.to_ascii_lowercase();
            state.custom_command_names.iter().any(|n| n == &lower)
        })
        .unwrap_or(false);
    if !is_custom_cmd {
        if let Some(cmd) = crate::telegram::commands::parse_tg_command(&text) {
            handle_tg_command(&bot, msg.chat.id, cmd, &state).await?;
            return Ok(());
        }
    }

    // Send typing indicator
    let _ = bot.send_chat_action(msg.chat.id, ChatAction::Typing).await;

    // Build ChatMessage list from session history + new user message
    let user_msg = serde_json::json!({ "role": "user", "content": text });

    // Snapshot the full session, then let the shared trim/inject helpers prune to the
    // configured context window. Keeps trim semantics identical between desktop and
    // telegram paths.
    let chat_messages: Vec<serde_json::Value> = {
        let mut session_msgs = state.session_messages.lock().await;
        session_msgs.push(user_msg);
        session_msgs.clone()
    };

    // Convert to ChatMessage structs and inject the same mood-context system note that
    // the desktop chat path uses. This keeps the pet's persona behavior consistent
    // regardless of which surface the user is talking through.
    let chat_messages: Vec<ChatMessage> = chat_messages
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    let max_context = AiConfig::from_settings()
        .map(|c| c.max_context_messages)
        .unwrap_or(50);
    let chat_messages = trim_to_context(chat_messages, max_context);
    let chat_messages = inject_mood_note(chat_messages);
    // Iter 107: optionally inject the route-A persona layer (companionship days /
    // persona summary / mood trend) into the Telegram chat path too. Gated on
    // `telegram.persona_layer_enabled` (captured at bot start) so users who prefer
    // terse Telegram chat can opt out without affecting the desktop chat path.
    let chat_messages = if state.persona_layer_enabled {
        inject_persona_layer(chat_messages).await
    } else {
        chat_messages
    };
    // Iter R71: focus-context parity with reactive chat — telegram users
    // can also ask "我今天怎么样" and the AI now has stats to answer
    // coherently. Modality-agnostic data (today/week aggregates) so cross-
    // device makes sense; recent_speech (R9) deliberately *not* injected
    // because those bubbles were desktop-only — telegram user didn't see
    // them and citing would be confusing.
    let chat_messages = inject_focus_context_layer(chat_messages);
    // Iter R79: deadline parity — telegram user can also ask "我有什么
    // deadline" and AI now has the data. butler_tasks is modality-agnostic
    // (lives in user's persistent memory, not surface-bound).
    let chat_messages = inject_deadline_context_layer(chat_messages);

    // Telegram 派单层：告诉 LLM 当前在 TG 通道，应当用 task_create 直接
    // 落盘（而非 propose_task —— TG 没有确认卡 UI）。chat_id 注入到提示
    // 里，让 LLM 调 task_create 时能填正确的 origin。
    let chat_messages = inject_telegram_dispatch_layer(chat_messages, msg.chat.id.0);

    // Run the LLM pipeline
    let reply_text = match AiConfig::from_settings() {
        Ok(config) => {
            let ctx = ToolContext::new(
                state.log_store.clone(),
                state.shell_store.clone(),
                state.process_counters.clone(),
            );
            let sink = CollectingSink::new();
            match run_chat_pipeline(chat_messages, &sink, &config, &state.mcp_store, &ctx).await {
                Ok(text) => {
                    // Emit chat-done with the post-turn mood snapshot so the desktop pet's
                    // Live2D motion reflects state changes even when the user was chatting
                    // via Telegram. Same payload shape as the chat tauri command.
                    let (mood, motion) = read_mood_for_event(&ctx, "Telegram");
                    let payload = ChatDonePayload {
                        mood,
                        motion,
                        timestamp: chrono::Local::now()
                            .format("%Y-%m-%dT%H:%M:%S%.3f")
                            .to_string(),
                    };
                    let _ = state.app.emit("chat-done", payload);
                    text
                }
                Err(e) => format!("Error: {}", e),
            }
        }
        Err(e) => format!("Config error: {}", e),
    };

    // Save assistant message to session
    {
        let assistant_msg = serde_json::json!({ "role": "assistant", "content": reply_text });
        let mut session_msgs = state.session_messages.lock().await;
        session_msgs.push(assistant_msg);

        // Persist to disk
        let now = chrono::Local::now()
            .format("%Y-%m-%dT%H:%M:%S%.3f")
            .to_string();
        let items: Vec<serde_json::Value> = session_msgs
            .iter()
            .filter_map(|m| {
                let role = m["role"].as_str()?;
                let content = m["content"].as_str()?;
                match role {
                    "user" => Some(serde_json::json!({ "type": "user", "content": content })),
                    "assistant" => {
                        Some(serde_json::json!({ "type": "assistant", "content": content }))
                    }
                    _ => None,
                }
            })
            .collect();

        let first_user = items.iter().find(|i| i["type"] == "user");
        let title = first_user
            .and_then(|i| i["content"].as_str())
            .map(|c| {
                let t = c.chars().take(20).collect::<String>();
                if c.len() > 20 {
                    format!("{}...", t)
                } else {
                    t
                }
            })
            .unwrap_or_else(|| "Telegram".to_string());

        let s = session::Session {
            id: state.session_id.clone(),
            title,
            created_at: String::new(), // preserved by backend
            updated_at: now,
            messages: session_msgs.clone(),
            items,
        };
        let _ = session::save_session(s);
    }

    // Send reply (split if exceeds Telegram limit)
    if reply_text.len() <= TELEGRAM_MSG_LIMIT {
        bot.send_message(msg.chat.id, &reply_text).await?;
    } else {
        // 超长 → 切块并给每块加 `(i/n) ` 前缀，避免接收方把第 2、3 块误读为
        // 新一轮发言（特别是 TG 群组多人场景）。
        for chunk in format_split_chunks(&reply_text, TELEGRAM_MSG_LIMIT) {
            bot.send_message(msg.chat.id, chunk).await?;
        }
    }

    Ok(())
}

/// 命令调度。把 TG 用户发的 /cancel / /retry / 未知命令路由到对应的
/// Tauri 命令逻辑，并把结果格式化回 TG。文案 helpers 都是 pure（在
/// `telegram::commands` 里有单测），本函数本身只做"参数缺省 → handler
/// 选择 → bot 回消息"的 IO 编排，错误一律以 `format_command_error` 包
/// 出回给用户而非 panic。
async fn handle_tg_command(
    bot: &Bot,
    chat_id: ChatId,
    cmd: crate::telegram::commands::TgCommand,
    state: &Arc<HandlerState>,
) -> ResponseResult<()> {
    use crate::telegram::commands::{
        format_command_error, format_command_success, format_help_text,
        format_missing_argument, format_task_created_success, format_tasks_no_change,
        format_unknown_command, TgCommand,
    };

    let reply: String = match cmd {
        TgCommand::Cancel { ref title } | TgCommand::Retry { ref title }
            if title.trim().is_empty() =>
        {
            format_missing_argument(cmd.name())
        }
        TgCommand::Task { ref title, .. } if title.trim().is_empty() => {
            format_missing_argument(cmd.name())
        }
        TgCommand::Cancel { title } => {
            // 三层 resolve（自上而下）：
            // 1. 数字编号 → titles[N-1]（最近 /tasks 显示顺序）
            // 2. fuzzy resolve（精确 / 唯一 substring）
            // 3. 0 命中 / 多命中 → 错误反馈带候选列表
            let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                Some(t) => Ok(t),
                None => resolve_tg_task_title(&title),
            };
            match actual {
                Ok(t) => {
                    let decisions = state
                        .app
                        .state::<crate::decision_log::DecisionLogStore>()
                        .inner()
                        .clone();
                    match crate::commands::task::task_cancel_inner(
                        t.clone(),
                        String::new(),
                        decisions,
                    ) {
                        Ok(()) => format_command_success("cancel", &t),
                        Err(e) => format_command_error(&e),
                    }
                }
                Err(msg) => format_command_error(&msg),
            }
        }
        TgCommand::Retry { title } => {
            let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                Some(t) => Ok(t),
                None => resolve_tg_task_title(&title),
            };
            match actual {
                Ok(t) => {
                    let decisions = state
                        .app
                        .state::<crate::decision_log::DecisionLogStore>()
                        .inner()
                        .clone();
                    match crate::commands::task::task_retry_inner(t.clone(), decisions) {
                        Ok(()) => format_command_success("retry", &t),
                        Err(e) => format_command_error(&e),
                    }
                }
                Err(msg) => format_command_error(&msg),
            }
        }
        TgCommand::Task { title, priority } => {
            // 直接落 butler_tasks，不经 LLM。`priority` 由 `parse_task_prefix`
            // 预解析：默认 3（与桌面 PanelTasks 默认一致）/ `!!` 5 / `!!!` 7。
            // 无 due / 无 body —— 想精细派单的用户回桌面调。origin 标记
            // [origin:tg:<chat_id>] 让 watcher 在完成 / 失败时把通知回传到
            // 本会话。
            let title_trimmed = title.trim().to_string();
            let header = crate::task_queue::TaskHeader {
                priority,
                due: None,
                body: String::new(),
            };
            let mut description = crate::task_queue::format_task_description(&header);
            description = crate::task_queue::append_origin_marker(
                &description,
                &crate::task_queue::TaskOrigin::Tg(chat_id.0),
            );
            match crate::commands::memory::memory_edit(
                "create".to_string(),
                "butler_tasks".to_string(),
                title_trimmed.clone(),
                Some(description),
                Some(String::new()),
            ) {
                Ok(_) => format_task_created_success(&title_trimmed, priority),
                Err(e) => format_command_error(&e),
            }
        }
        TgCommand::Tasks => {
            // 去重：与上次 `/tasks` 在同 chat 发出的 body 完全一致 → 简短回
            // "无变化"，避免 TG 历史里堆两份同样列表刷屏。
            // 同时**总是**更新 titles 缓存（不论 body 是否变化），让后续
            // /cancel N / /retry N 用最新的显示顺序。
            let (body, titles) = format_tasks_for_chat(chat_id.0);
            {
                let mut tcache = state.last_tasks_titles.lock().await;
                tcache.insert(chat_id.0, titles);
            }
            let mut cache = state.last_tasks_response.lock().await;
            let key = chat_id.0;
            if cache.get(&key) == Some(&body) {
                format_tasks_no_change()
            } else {
                cache.insert(key, body.clone());
                body
            }
        }
        TgCommand::Help => format_help_text(&state.custom_command_objects),
        TgCommand::Unknown { name } => {
            // 用 levenshtein 在已知命令名里找距 ≤ 2 的最近候选 → 在反馈
            // 首行加 "你是想发 /xxx 吗？"，避免用户来回翻 /help。
            let valid: Vec<&str> = crate::telegram::commands::tg_command_registry()
                .into_iter()
                .map(|(n, _)| n)
                .collect();
            let suggestion = crate::telegram::commands::suggest_command(&name, &valid);
            format_unknown_command(&name, suggestion)
        }
    };
    bot.send_message(chat_id, reply).await?;
    Ok(())
}

/// 读 butler_tasks，过滤出 `[origin:tg:<chat_id>]` 标记匹配的条目，按
/// `compare_for_queue` 排序后调 `format_tasks_list` 返回 TG 文本。
///
/// 不向用户抛"读 memory 失败"等技术错误 —— 失败一律视作"暂无任务"，回
/// 友好提示文案。理由：用户视角下"我列任务"失败是噪音，比起精确告警不
/// 如静默回退（任何持久层故障都会被 panel 端的红点 / 决策日志捕捉）。
/// 把 TG 用户输入的 query（来自 `/cancel <q>` / `/retry <q>`）解析成一个真
/// 实 butler_tasks title。先精确，再 case-insensitive substring；唯一命中
/// → Ok(actual)，0/多 命中 → Err（带候选预览）。Err 串可直接灌进
/// `format_command_error` 包装成⚠️提示。
/// 把 query 当成 1-indexed 序号在 `last_tasks_titles[chat_id]` 里查 title。
/// 非数字 / 0 / 越界 / 缓存空 → None，让 caller fall back 到 fuzzy resolve。
/// async 因为要锁 TokioMutex；纯解析逻辑 `resolve_index_to_title` 在 commands.rs
/// 那边单测覆盖。
async fn try_resolve_by_index(
    query: &str,
    chat_id: i64,
    state: &Arc<HandlerState>,
) -> Option<String> {
    let cache = state.last_tasks_titles.lock().await;
    let titles = cache.get(&chat_id)?;
    crate::telegram::commands::resolve_index_to_title(query, titles)
}

fn resolve_tg_task_title(query: &str) -> Result<String, String> {
    let titles = read_butler_task_titles();
    use crate::telegram::commands::{
        find_task_fuzzy, format_ambiguous_match, format_no_match_with_suggestions,
        suggest_titles, FuzzyMatch,
    };
    match find_task_fuzzy(query, &titles) {
        FuzzyMatch::Exact(t) | FuzzyMatch::Single(t) => Ok(t),
        FuzzyMatch::None => {
            // 0 命中 → 给 char-overlap 最高的 1-2 条做"你是不是想…"建议，
            // 让用户少打一遍长 title。
            let suggestions = suggest_titles(query, &titles, 2);
            Err(format_no_match_with_suggestions(query, &suggestions))
        }
        FuzzyMatch::Ambiguous(list) => Err(format_ambiguous_match(query, &list)),
    }
}

/// 给 fuzzy resolve 用的 title 列表收集器。读 butler_tasks → 取 title 字符串
/// 即可（fuzzy 不关心 description / origin / status）。memory_list 失败 / 类目
/// 缺失视作"没有任务"，让上层走 None 错误路径。
fn read_butler_task_titles() -> Vec<String> {
    let Ok(index) = crate::commands::memory::memory_list(Some("butler_tasks".to_string())) else {
        return Vec::new();
    };
    let Some(cat) = index.categories.get("butler_tasks") else {
        return Vec::new();
    };
    cat.items.iter().map(|i| i.title.clone()).collect()
}

/// 返回 `(body, ordered_titles)`：body 是给 TG 用户看的完整文本，ordered_titles
/// 是同显示顺序的 title vec（给 `/cancel N` / `/retry N` 解析序号用）。
/// 显示顺序遵循 `format_tasks_list` 的 section 排列（Pending → Done →
/// Error → Cancelled，section 内沿用 `compare_for_queue`）。
fn format_tasks_for_chat(chat_id: i64) -> (String, Vec<String>) {
    let Ok(index) = crate::commands::memory::memory_list(Some("butler_tasks".to_string())) else {
        return (crate::telegram::commands::format_tasks_list(&[]), Vec::new());
    };
    let Some(cat) = index.categories.get("butler_tasks") else {
        return (crate::telegram::commands::format_tasks_list(&[]), Vec::new());
    };
    let mut views: Vec<crate::task_queue::TaskView> = cat
        .items
        .iter()
        .filter(|item| {
            matches!(
                crate::task_queue::parse_task_origin(&item.description),
                Some(crate::task_queue::TaskOrigin::Tg(id)) if id == chat_id
            )
        })
        .map(crate::commands::task::build_task_view)
        .collect();
    let now = chrono::Local::now().naive_local();
    views.sort_by(|a, b| crate::task_queue::compare_for_queue(a, b, now));
    let body = crate::telegram::commands::format_tasks_list(&views);
    // 与 format_tasks_list 的 section 顺序一致地拼 ordered_titles
    use crate::task_queue::TaskStatus;
    let mut pending: Vec<String> = Vec::new();
    let mut done: Vec<String> = Vec::new();
    let mut error: Vec<String> = Vec::new();
    let mut cancelled: Vec<String> = Vec::new();
    for v in &views {
        match v.status {
            TaskStatus::Pending => pending.push(v.title.clone()),
            TaskStatus::Done => done.push(v.title.clone()),
            TaskStatus::Error => error.push(v.title.clone()),
            TaskStatus::Cancelled => cancelled.push(v.title.clone()),
        }
    }
    let mut ordered = Vec::with_capacity(views.len());
    ordered.append(&mut pending);
    ordered.append(&mut done);
    ordered.append(&mut error);
    ordered.append(&mut cancelled);
    (body, ordered)
}

/// 注入 Telegram 派单层到 chat messages。系统级 note，告诉 LLM 当前
/// 通道是 TG、应当直接 `task_create` 而非 `propose_task`，并提供 chat_id
/// 让 LLM 填进 origin 参数。pure / 可单测（不依赖任何 IO）。
pub fn inject_telegram_dispatch_layer(
    mut messages: Vec<ChatMessage>,
    chat_id: i64,
) -> Vec<ChatMessage> {
    let note = format!(
        "[Telegram dispatch] 你正在通过 Telegram 与主人对话。\n\
- 如果主人请你做一件具体的、适合放进任务队列的事（「帮我…」「记得…」「这周末…」），**直接调用 `task_create`**（不要用 `propose_task` —— Telegram 没有确认卡 UI）。\n\
- 调用 `task_create` 时务必带上 `origin=\"tg:{}\"`，让任务完成后能把结果发回这条对话。\n\
- 创建后口头简短承接：「好的，加到队列里了，做完会回这里告诉你」。\n\
- 普通闲聊 / 提问 / 抒情不要建任务。",
        chat_id
    );
    let layer = serde_json::from_value(serde_json::json!({
        "role": "system",
        "content": note,
    }))
    .expect("static json must deserialize");
    // 紧跟在 system soul 之后；如果一条 system 都没有就放最前
    let insert_at = messages
        .iter()
        .position(|m| m.role != "system")
        .unwrap_or(messages.len());
    messages.insert(insert_at, layer);
    messages
}

/// 任务回传 watcher 主循环。每 `WATCHER_INTERVAL_SECS` 秒：
/// 1. 读 butler_tasks 全量
/// 2. 过滤出带 `[origin:tg:...]` 标记的条目
/// 3. 与上一轮 snapshot 比对：状态从非终态 → 终态 → 通过 bot 给原 chat 发消息
/// 4. 心跳停滞通知：被宠物动过手却 stagnated 超 settings.proactive
///    .task_heartbeat_minutes 的 pending 任务，发一句"卡 N 分钟了"+ /retry
///    /cancel 命令模板。同 (title, updated_at) 组合只发一次（updated_at
///    变化即算新一轮）。
/// 5. 更新 snapshot + 清理过期 last_heartbeat 条目（任务已删 → 移除）
///
/// 冷启动首轮：只填 snapshot 与 last_heartbeat，**不发任何消息**（避免重启
/// 就把所有"早就 stuck / 早就 done"任务再轰炸一遍）。
async fn run_task_watcher(bot: Bot) {
    let mut snapshot: TaskSnapshot = HashMap::new();
    // 心跳通知去重：title → 上次发心跳通知时该任务的 updated_at。updated_at
    // 变化（LLM 又写了一笔 / 用户改了 priority）→ 视作"任务又活了一下"，下次
    // stuck 时再发。任务被删 → 在 cleanup 步骤里移除条目，避免内存泄漏。
    let mut last_heartbeat: HashMap<String, String> = HashMap::new();
    let mut first_pass = true;
    loop {
        tokio::time::sleep(Duration::from_secs(WATCHER_INTERVAL_SECS)).await;
        let Ok(index) = crate::commands::memory::memory_list(Some("butler_tasks".to_string()))
        else {
            continue;
        };
        let Some(cat) = index.categories.get("butler_tasks") else {
            continue;
        };
        // 心跳阈值动态读 settings；0 = 关。每 tick 重读，让用户改设置后无需重启。
        let heartbeat_threshold: u32 = crate::commands::settings::get_settings()
            .map(|s| s.proactive.task_heartbeat_minutes)
            .unwrap_or(0);
        let now_naive = chrono::Local::now().naive_local();
        let mut next: TaskSnapshot = HashMap::new();
        let mut seen_titles: std::collections::HashSet<String> = std::collections::HashSet::new();
        // 本轮所有 just_finished 事件，按 (chat_id, status) 分组：同组多条
        // 合并成一条 batch 文案，避免连续完成 N 条任务时 N 条独立通知刷屏。
        // HashMap 而非 BTreeMap：TaskStatus 未 derive Ord；几条 group 间发送
        // 顺序对用户体感无差。组内 push 顺序保留 cat.items 出现序（= YAML 写入序）。
        let mut completion_groups: HashMap<
            (i64, crate::task_queue::TaskStatus),
            Vec<(String, Option<String>)>,
        > = HashMap::new();
        for item in &cat.items {
            let Some(crate::task_queue::TaskOrigin::Tg(chat_id)) =
                crate::task_queue::parse_task_origin(&item.description)
            else {
                continue;
            };
            let (status, reason) = crate::task_queue::classify_status(&item.description);
            next.insert(item.title.clone(), status);
            seen_titles.insert(item.title.clone());
            if first_pass {
                continue;
            }
            let prior = snapshot.get(&item.title).copied();
            if just_finished(prior, status) {
                completion_groups
                    .entry((chat_id, status))
                    .or_default()
                    .push((item.title.clone(), reason));
                // 终态 → 后续不再触发心跳；移除 last_heartbeat 条目（防止 cancel
                // 后任务复活成 pending 时不发心跳的死锁）
                last_heartbeat.remove(&item.title);
                continue;
            }
            // 心跳通知（仅 pending；status != Pending 由 is_heartbeat_candidate
            // 内部判定）。同 (title, updated_at) 已通知过则跳过。
            if heartbeat_threshold > 0
                && crate::task_heartbeat::is_heartbeat_candidate(
                    &item.description,
                    &item.created_at,
                    &item.updated_at,
                    now_naive,
                    heartbeat_threshold,
                )
                && last_heartbeat.get(&item.title) != Some(&item.updated_at)
            {
                let body = format_heartbeat_message(&item.title, heartbeat_threshold);
                let _ = bot.send_message(ChatId(chat_id), body).await;
                last_heartbeat.insert(item.title.clone(), item.updated_at.clone());
            }
        }
        // dispatch 本轮 completion 事件：单条走旧文案（防回归），多条走 batch
        // 文案合并。一组失败不阻断其它组（`let _` 显式忽略 send 错误，与既有
        // single-event 路径相同语义）。
        for ((chat_id, status), events) in completion_groups {
            let body = if events.len() == 1 {
                let (title, reason) = &events[0];
                format_completion_message(title, status, reason.as_deref())
            } else {
                format_completion_batch(status, &events)
            };
            let _ = bot.send_message(ChatId(chat_id), body).await;
        }
        // 清理已删任务的 last_heartbeat 条目，避免内存泄漏
        last_heartbeat.retain(|title, _| seen_titles.contains(title));
        snapshot = next;
        first_pass = false;
    }
}

/// pure：判断"这一轮的状态是不是从非终态翻入终态"。
/// - prior 缺失（任务在本轮才出现）+ 当前已是终态 → false（首次出现就是 done
///   的话，意味着是 watcher 错过了变化或者用户在 panel 里手建已完成任务，
///   不主动通知，避免噪音）
/// - prior 是 Pending / Error → 当前是 Done / Cancelled / Error → true
///   - Error → Error 也算"刚结束的一轮"？不算，要求**状态变化**。
fn just_finished(
    prior: Option<crate::task_queue::TaskStatus>,
    now: crate::task_queue::TaskStatus,
) -> bool {
    use crate::task_queue::TaskStatus;
    if !matches!(now, TaskStatus::Done | TaskStatus::Cancelled | TaskStatus::Error) {
        return false;
    }
    match prior {
        // 首次出现就是终态 → 静默（重启 / 漏检场景）
        None => false,
        Some(p) => p != now,
    }
}

/// pure：渲染心跳停滞通知文案。给 TG-origin 任务在 stuck 超阈值时用。文末
/// 附 `/retry` `/cancel` 命令模板，让用户可以一键回操作（与既有 TG 命令矩阵
/// 同源）。
fn format_heartbeat_message(title: &str, minutes: u32) -> String {
    let title = title.trim();
    format!(
        "⏳ 任务「{}」卡了 {} 分钟没动了，要不要我点一下？\n回 /retry {} 让我重试 · /cancel {} 取消",
        title, minutes, title, title
    )
}

/// pure：渲染 TG 通知文案。reason 来自 classify_status 的 Option<String>：
/// done → 不带；error/cancelled → 附原因。
fn format_completion_message(
    title: &str,
    status: crate::task_queue::TaskStatus,
    reason: Option<&str>,
) -> String {
    use crate::task_queue::TaskStatus;
    let title = title.trim();
    match status {
        TaskStatus::Done => format!("✅ 「{}」 已完成", title),
        TaskStatus::Error => match reason.map(str::trim).filter(|s| !s.is_empty()) {
            Some(r) => format!("⚠️ 「{}」 执行失败：{}", title, r),
            None => format!("⚠️ 「{}」 执行失败", title),
        },
        TaskStatus::Cancelled => match reason.map(str::trim).filter(|s| !s.is_empty()) {
            Some(r) => format!("🚫 「{}」 已取消：{}", title, r),
            None => format!("🚫 「{}」 已取消", title),
        },
        // Pending 不该走到这；防御性 fallback
        TaskStatus::Pending => format!("「{}」 状态变更", title),
    }
}

/// pure：把同一 watcher tick 里同状态的多条 just-finished 事件合并成一条
/// TG 文案。让连续完成 N 条任务时不再 N 条独立通知刷屏。
///
/// 与 `format_completion_message` 的边界：
/// - 事件 1 条 → 调用方走旧函数（保留既有单条文案，防回归）
/// - 事件 ≥ 2 条 → 本函数：`{emoji} {状态文案} {N} 条：t1 · t2 · …`
///
/// reason 处理：
/// - done 状态 reason 永远忽略（done 没有 reason 语义）
/// - error / cancelled：每条 title 后括号内附 reason；reason 缺失 / 空白
///   时省略括号
///
/// 分隔符 `·`：中文标题里逗号常见，避免分隔符与内容冲突。
fn format_completion_batch(
    status: crate::task_queue::TaskStatus,
    titles_with_reasons: &[(String, Option<String>)],
) -> String {
    use crate::task_queue::TaskStatus;
    let count = titles_with_reasons.len();
    let parts: Vec<String> = titles_with_reasons
        .iter()
        .map(|(title, reason)| {
            let t = title.trim();
            match status {
                TaskStatus::Done => t.to_string(),
                TaskStatus::Error | TaskStatus::Cancelled => {
                    match reason.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                        Some(r) => format!("{}（{}）", t, r),
                        None => t.to_string(),
                    }
                }
                TaskStatus::Pending => t.to_string(),
            }
        })
        .collect();
    let joined = parts.join(" · ");
    match status {
        TaskStatus::Done => format!("✅ 已完成 {} 条：{}", count, joined),
        TaskStatus::Error => format!("⚠️ 任务失败 {} 条：{}", count, joined),
        TaskStatus::Cancelled => format!("🚫 已取消 {} 条：{}", count, joined),
        TaskStatus::Pending => format!("状态变更 {} 条：{}", count, joined),
    }
}

fn split_message(text: &str, max_len: usize) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let end = std::cmp::min(start + max_len, text.len());
        // Try to split at a newline or space boundary
        let split_at = if end == text.len() {
            end
        } else {
            text[start..end]
                .rfind('\n')
                .or_else(|| text[start..end].rfind(' '))
                .map(|pos| start + pos + 1)
                .unwrap_or(end)
        };
        result.push(&text[start..split_at]);
        start = split_at;
    }
    result
}

/// 给 `(i/n) ` 前缀预留的 byte 预算。覆盖到 N=99 (`(99/99) ` 占 8 byte)
/// 加 4 byte 安全垫。从 `max_len` 里扣这一段，让最终每块 + 前缀 ≤ max_len。
const SPLIT_PREFIX_BUDGET: usize = 12;

/// Pure：切超长消息成多块并加 `(i/n) ` 前缀。
///
/// **调用前提**：text.len() > max_len（单块场景由调用方走快路径直接发原文，
/// 不加前缀）。这样调用方代码里的 1 块 / N 块两条路径泾渭分明，前缀仅对真正
/// 分页的回复出现。
///
/// 切分边界用现有 `split_message` 的"换行 > 空格 > byte"启发式；前缀占用从
/// effective budget 里扣（`max_len - SPLIT_PREFIX_BUDGET`），保证拼前缀后每块
/// 仍 ≤ max_len。
fn format_split_chunks(text: &str, max_len: usize) -> Vec<String> {
    let effective = max_len.saturating_sub(SPLIT_PREFIX_BUDGET).max(1);
    let chunks = split_message(text, effective);
    let n = chunks.len();
    chunks
        .iter()
        .enumerate()
        .map(|(i, c)| format!("({}/{}) {}", i + 1, n, c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_queue::TaskStatus;

    fn msg(role: &str, content: &str) -> ChatMessage {
        serde_json::from_value(serde_json::json!({
            "role": role,
            "content": content,
        }))
        .unwrap()
    }

    // -------- inject_telegram_dispatch_layer --------

    #[test]
    fn dispatch_layer_inserted_after_system_messages() {
        let messages = vec![
            msg("system", "soul"),
            msg("user", "你好"),
            msg("assistant", "你好"),
        ];
        let out = inject_telegram_dispatch_layer(messages, 12345);
        // soul → tg layer → user → assistant
        assert_eq!(out[0].role, "system");
        assert_eq!(out[0].content.as_str().unwrap(), "soul");
        assert_eq!(out[1].role, "system");
        assert!(out[1].content.as_str().unwrap().contains("Telegram dispatch"));
        assert!(out[1].content.as_str().unwrap().contains("tg:12345"));
        assert_eq!(out[2].role, "user");
        assert_eq!(out[3].role, "assistant");
    }

    #[test]
    fn dispatch_layer_negative_chat_id_rendered_as_is() {
        // Telegram 群组 chat_id 是负数
        let out = inject_telegram_dispatch_layer(vec![msg("system", "x")], -1001234567890);
        assert!(out[1].content.as_str().unwrap().contains("tg:-1001234567890"));
    }

    #[test]
    fn dispatch_layer_inserts_at_top_when_no_system_message() {
        let out = inject_telegram_dispatch_layer(vec![msg("user", "你好")], 1);
        assert_eq!(out[0].role, "system");
        assert_eq!(out[1].role, "user");
    }

    // -------- just_finished --------

    #[test]
    fn just_finished_pending_to_done() {
        assert!(just_finished(Some(TaskStatus::Pending), TaskStatus::Done));
        assert!(just_finished(Some(TaskStatus::Pending), TaskStatus::Cancelled));
        assert!(just_finished(Some(TaskStatus::Pending), TaskStatus::Error));
    }

    #[test]
    fn just_finished_ignores_first_appearance() {
        // 任务在本轮才出现就已经是终态 — 静默
        assert!(!just_finished(None, TaskStatus::Done));
        assert!(!just_finished(None, TaskStatus::Cancelled));
    }

    #[test]
    fn just_finished_pending_stays_quiet() {
        // 还没结束 → 不发
        assert!(!just_finished(Some(TaskStatus::Pending), TaskStatus::Pending));
        assert!(!just_finished(None, TaskStatus::Pending));
    }

    #[test]
    fn just_finished_no_repeat_for_same_terminal() {
        // 上一轮 Done → 这一轮还是 Done：不重发
        assert!(!just_finished(Some(TaskStatus::Done), TaskStatus::Done));
        assert!(!just_finished(Some(TaskStatus::Cancelled), TaskStatus::Cancelled));
    }

    #[test]
    fn just_finished_error_to_cancelled_fires() {
        // 状态变化（哪怕都是终态）—— Error → Cancelled 是用户取消了之前
        // 失败的任务，发一次"已取消"通知合理
        assert!(just_finished(Some(TaskStatus::Error), TaskStatus::Cancelled));
    }

    // -------- format_completion_message --------

    #[test]
    fn done_message_uses_check_mark() {
        let s = format_completion_message("整理 Downloads", TaskStatus::Done, None);
        assert!(s.starts_with("✅"));
        assert!(s.contains("整理 Downloads"));
        assert!(s.contains("已完成"));
    }

    #[test]
    fn error_message_includes_reason_when_present() {
        let s = format_completion_message("跑步", TaskStatus::Error, Some("下雨了"));
        assert!(s.starts_with("⚠️"));
        assert!(s.contains("跑步"));
        assert!(s.contains("下雨了"));
    }

    #[test]
    fn error_message_omits_reason_when_blank() {
        let s = format_completion_message("跑步", TaskStatus::Error, Some("   "));
        // 空白原因等同无原因
        assert!(!s.contains("："));
    }

    #[test]
    fn cancelled_message_uses_prohibition_emoji() {
        let s = format_completion_message("跑步", TaskStatus::Cancelled, Some("不做了"));
        assert!(s.starts_with("🚫"));
        assert!(s.contains("不做了"));
    }

    // -------- format_heartbeat_message --------

    #[test]
    fn heartbeat_message_includes_title_minutes_and_command_templates() {
        let s = format_heartbeat_message("整理 Downloads", 30);
        assert!(s.starts_with("⏳"));
        assert!(s.contains("「整理 Downloads」"));
        assert!(s.contains("30 分钟"));
        // 命令模板必须能被 TG 输入栏 tap 进 /retry / /cancel 前缀
        assert!(s.contains("/retry 整理 Downloads"));
        assert!(s.contains("/cancel 整理 Downloads"));
    }

    #[test]
    fn heartbeat_message_trims_title_whitespace() {
        let s = format_heartbeat_message("  跑步  ", 45);
        assert!(s.contains("「跑步」"));
        assert!(!s.contains("「  跑步  」"));
    }

    // -------- format_split_chunks --------

    #[test]
    fn split_chunks_two_parts_have_prefix_and_fit_within_max_len() {
        // ASCII 文本，两块场景：长度 ~ 2x effective budget（6000 < 4096*2 = 8192）。
        let text = "a".repeat(6000);
        let max = 4096;
        let chunks = format_split_chunks(&text, max);
        assert!(chunks.len() >= 2, "expected at least 2 chunks, got {}", chunks.len());
        let n = chunks.len();
        for (i, c) in chunks.iter().enumerate() {
            assert!(
                c.starts_with(&format!("({}/{}) ", i + 1, n)),
                "chunk {} should start with ({}/{}) prefix; got: {:?}",
                i + 1,
                i + 1,
                n,
                c.chars().take(20).collect::<String>(),
            );
            assert!(
                c.len() <= max,
                "chunk {} length {} exceeds max {}",
                i + 1,
                c.len(),
                max,
            );
        }
    }

    #[test]
    fn split_chunks_preserve_content_when_concatenated() {
        // 拼回去（剥前缀）应等于原文，验证 split_message 边界没破坏内容
        let text: String = (0..30)
            .map(|i| format!("line{:02} content here\n", i))
            .collect();
        let chunks = format_split_chunks(&text, 200);
        // 每块剥掉前缀（开头 `(i/n) ` 直到第一个空格之后）
        let body: String = chunks
            .iter()
            .map(|c| {
                let after_close_paren = c.find(") ").map(|i| i + 2).unwrap_or(0);
                c[after_close_paren..].to_string()
            })
            .collect();
        assert_eq!(body, text);
    }

    #[test]
    fn split_chunks_handles_three_part_split() {
        // 验证 N>2 时索引 i 与 n 都正确递进
        let text = "x".repeat(9000);
        let chunks = format_split_chunks(&text, 4096);
        let n = chunks.len();
        assert!(n >= 3, "9000 / ~4084 effective budget should yield ≥3 chunks");
        // 最后一块 prefix 是 (n/n)
        assert!(chunks.last().unwrap().starts_with(&format!("({}/{}) ", n, n)));
    }

    #[test]
    fn split_chunks_min_max_len_does_not_panic() {
        // saturating_sub 保护：max_len 比预算小时 effective 至少为 1，不 panic
        let chunks = format_split_chunks("hello world", 4);
        assert!(!chunks.is_empty());
        // 每块仍 ≤ max_len（短 max_len 下会切很碎，但前缀必出现）
        for c in &chunks {
            assert!(c.starts_with("("));
        }
    }

    // -------- format_completion_batch --------

    fn ev(title: &str, reason: Option<&str>) -> (String, Option<String>) {
        (title.to_string(), reason.map(String::from))
    }

    #[test]
    fn batch_done_lists_count_and_titles_separated_by_middot() {
        let evs = vec![ev("整理 A", None), ev("打扫 B", None), ev("写 C", None)];
        let s = format_completion_batch(TaskStatus::Done, &evs);
        assert!(s.contains("✅"), "should have done emoji: {}", s);
        assert!(s.contains("3 条"), "should mention count: {}", s);
        assert!(s.contains("整理 A"));
        assert!(s.contains("打扫 B"));
        assert!(s.contains("写 C"));
        assert!(s.contains(" · "), "should use middot separator: {}", s);
    }

    #[test]
    fn batch_error_attaches_per_task_reason_in_parens() {
        // error / cancelled 的 reason 各自附加，方便用户当场判断每条的失败原因
        let evs = vec![
            ev("脚本 X", Some("permission denied")),
            ev("脚本 Y", Some("timeout")),
        ];
        let s = format_completion_batch(TaskStatus::Error, &evs);
        assert!(s.contains("⚠️"), "error emoji: {}", s);
        assert!(s.contains("2 条"));
        assert!(s.contains("脚本 X（permission denied）"));
        assert!(s.contains("脚本 Y（timeout）"));
    }

    #[test]
    fn batch_error_omits_paren_when_reason_blank() {
        let evs = vec![ev("脚本 X", None), ev("脚本 Y", Some("   "))];
        let s = format_completion_batch(TaskStatus::Error, &evs);
        // 没有 reason 时不该出现空括号
        assert!(!s.contains("（）"));
        assert!(!s.contains("（   ）"));
        assert!(s.contains("脚本 X"));
        assert!(s.contains("脚本 Y"));
    }

    #[test]
    fn batch_done_ignores_reason_field() {
        // done 没有 reason 语义；即便 caller 误传也不显示
        let evs = vec![ev("A", Some("some leftover")), ev("B", None)];
        let s = format_completion_batch(TaskStatus::Done, &evs);
        assert!(!s.contains("some leftover"), "done must not show reason: {}", s);
        assert!(s.contains("A"));
        assert!(s.contains("B"));
    }

    #[test]
    fn batch_cancelled_shows_block_emoji_and_count() {
        let evs = vec![ev("X", Some("用户取消")), ev("Y", None)];
        let s = format_completion_batch(TaskStatus::Cancelled, &evs);
        assert!(s.contains("🚫"));
        assert!(s.contains("2 条"));
        assert!(s.contains("X（用户取消）"));
        assert!(s.contains("Y"));
    }

    #[test]
    fn batch_two_titles_with_emoji_preserved() {
        // 标题含 emoji / 中文符号原样保留（Telegram 直接渲染）
        let evs = vec![ev("🐱 喂猫", None), ev("买菜！", None)];
        let s = format_completion_batch(TaskStatus::Done, &evs);
        assert!(s.contains("🐱 喂猫"));
        assert!(s.contains("买菜！"));
    }
}
