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
    inject_deadline_context_layer, inject_mood_note, inject_persona_layer, run_chat_pipeline,
    trim_to_context, ChatDonePayload, ChatMessage, CollectingSink,
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
        TgCommand::Cancel { ref title }
        | TgCommand::Retry { ref title }
        | TgCommand::Done { ref title }
        | TgCommand::Snooze { ref title, .. }
        | TgCommand::Unsnooze { ref title }
        | TgCommand::Pin { ref title }
        | TgCommand::Unpin { ref title }
        | TgCommand::Silent { ref title }
        | TgCommand::Unsilent { ref title }
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
        TgCommand::Done { title } => {
            // Resolve 与 /cancel /retry 同三层（数字 index → fuzzy → 错误）。
            // task_mark_done_inner 传 None result —— TG 单行命令不收 result
            // 摘要；想加 result 走桌面板。已 done / cancelled 状态会被后端
            // 拒绝（与桌面同策略）。
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
                    match crate::commands::task::task_mark_done_inner(
                        t.clone(),
                        None,
                        decisions,
                    ) {
                        Ok(()) => format_command_success("done", &t),
                        Err(e) => format_command_error(&e),
                    }
                }
                Err(msg) => format_command_error(&msg),
            }
        }
        TgCommand::Snooze { title, token } => {
            // Resolve 与 /done /cancel /retry 同三层。token 空时默认 30m；
            // token 非空但解析失败（比如 "/snooze title 99y"）→ 报错而非
            // 沉默落 default，让用户知道 typo。
            // 先校验 token 再 resolve title —— 让 invalid-token 错误比
            // task-not-found 优先级高（用户先解决 typo 再考虑 title 是否对）。
            let spec_result: Result<crate::telegram::commands::SnoozeSpec, String> =
                if token.is_empty() {
                    Ok(crate::telegram::commands::SnoozeSpec::Minutes(30))
                } else {
                    crate::telegram::commands::parse_snooze_token(&token).ok_or_else(|| {
                        format!(
                            "未知 preset「{}」 — 支持 30m / 2h / tonight / tomorrow / monday，或省略走默认 30m",
                            token,
                        )
                    })
                };
            match spec_result {
                Err(msg) => format_command_error(&msg),
                Ok(spec) => {
                    let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                        Some(t) => Ok(t),
                        None => resolve_tg_task_title(&title),
                    };
                    match actual {
                        Ok(t) => {
                            let now = chrono::Local::now().naive_local();
                            let until = crate::telegram::commands::compute_snooze_until(
                                spec, now,
                            );
                            let until_str =
                                until.format("%Y-%m-%d %H:%M").to_string();
                            match crate::commands::task::task_set_snooze(
                                t.clone(),
                                Some(until_str.clone()),
                            ) {
                                Ok(()) => format!(
                                    "💤 已暂停「{}」至 {}\n如需解除发 /unsnooze {}",
                                    t, until_str, t
                                ),
                                Err(e) => format_command_error(&e),
                            }
                        }
                        Err(msg) => format_command_error(&msg),
                    }
                }
            }
        }
        TgCommand::Unsnooze { title } => {
            let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                Some(t) => Ok(t),
                None => resolve_tg_task_title(&title),
            };
            match actual {
                Ok(t) => match crate::commands::task::task_set_snooze(t.clone(), None) {
                    Ok(()) => format!("☀️ 已解除「{}」 的暂停", t),
                    Err(e) => format_command_error(&e),
                },
                Err(msg) => format_command_error(&msg),
            }
        }
        TgCommand::Pin { title } => {
            // Resolve 三层与 /done /snooze 同。task_set_pinned 是 strip-before-
            // write 幂等：已 pinned 时 owner 再 /pin 不会让 description 累积
            // 冗余 marker。提示文案附 /unpin 反向命令。
            let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                Some(t) => Ok(t),
                None => resolve_tg_task_title(&title),
            };
            match actual {
                Ok(t) => match crate::commands::task::task_set_pinned(t.clone(), true) {
                    Ok(()) => format!("📌 已钉住「{}」\n如需解除发 /unpin {}", t, t),
                    Err(e) => format_command_error(&e),
                },
                Err(msg) => format_command_error(&msg),
            }
        }
        TgCommand::Unpin { title } => {
            let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                Some(t) => Ok(t),
                None => resolve_tg_task_title(&title),
            };
            match actual {
                Ok(t) => match crate::commands::task::task_set_pinned(t.clone(), false) {
                    Ok(()) => format!("📌 已取消钉住「{}」", t),
                    Err(e) => format_command_error(&e),
                },
                Err(msg) => format_command_error(&msg),
            }
        }
        TgCommand::Silent { title } => {
            // 与 /pin 同模板：三层 resolve + atomic task_set_silent + 反向命令
            // 提示。silent 让 LLM proactive cycle 不主动 pick；面板 / 手动触发
            // 不受影响。
            let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                Some(t) => Ok(t),
                None => resolve_tg_task_title(&title),
            };
            match actual {
                Ok(t) => match crate::commands::task::task_set_silent(t.clone(), true) {
                    Ok(()) => format!(
                        "🔇 已标 silent「{}」\nLLM 不再主动选；如需恢复发 /unsilent {}",
                        t, t
                    ),
                    Err(e) => format_command_error(&e),
                },
                Err(msg) => format_command_error(&msg),
            }
        }
        TgCommand::Unsilent { title } => {
            let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                Some(t) => Ok(t),
                None => resolve_tg_task_title(&title),
            };
            match actual {
                Ok(t) => match crate::commands::task::task_set_silent(t.clone(), false) {
                    Ok(()) => format!("🔇 已解除 silent「{}」", t),
                    Err(e) => format_command_error(&e),
                },
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
        TgCommand::Stats => {
            // 与 /tasks 共用 read path（memory_list → 过滤 Tg(chat_id) → build_task_view），
            // 但不去重、不缓存：用户连发 /stats 就是想看"现在到底什么样"。
            let views = read_tg_chat_task_views(chat_id.0);
            let now = chrono::Local::now().naive_local();
            crate::telegram::commands::format_stats_reply(&views, now, now.date())
        }
        TgCommand::Buckets => {
            // 与 /stats 同 read path + filter active；formatter 内部 priority
            // 分桶 + 一行式 dump。
            let views = read_tg_chat_task_views(chat_id.0);
            crate::telegram::commands::format_buckets_reply(&views)
        }
        TgCommand::Pinned => {
            // 与 /tasks 共用 read path + 同 chat 过滤；再加 t.pinned 子集过滤。
            // 不缓存（与 /stats 同思路）：用户连发就是想"看现在到底什么样"。
            let views: Vec<crate::task_queue::TaskView> = read_tg_chat_task_views(chat_id.0)
                .into_iter()
                .filter(|v| v.pinned)
                .collect();
            crate::telegram::commands::format_pinned_tasks_list(&views)
        }
        TgCommand::PinnedDue => {
            // read 路径与 /pinned / /silenced 同；formatter 内部 filter
            // active + pinned + due.is_some() + sort by due asc。本 handler
            // 只过 chat-scope，formatter 做剩余过滤让单测稳定。
            let views = read_tg_chat_task_views(chat_id.0);
            crate::telegram::commands::format_pinned_due_reply(&views)
        }
        TgCommand::Silenced => {
            // 与 /pinned 同模板：read_tg_chat_task_views + raw_description 含
            // [silent] marker 过滤。format_silenced_tasks_list 分状态分组渲染。
            let views: Vec<crate::task_queue::TaskView> = read_tg_chat_task_views(chat_id.0)
                .into_iter()
                .filter(|v| {
                    crate::task_queue::parse_silent(&v.raw_description)
                })
                .collect();
            crate::telegram::commands::format_silenced_tasks_list(&views)
        }
        TgCommand::Markers => {
            // /markers 同时考虑 pinned + silent 两 markers，所以 caller 把
            // 全 union 也都传进 format helper 内部再分两段渲染。即 caller
            // 不过滤；read 路径就 chat-scoped。
            let views: Vec<crate::task_queue::TaskView> =
                read_tg_chat_task_views(chat_id.0)
                    .into_iter()
                    .filter(|v| {
                        v.pinned
                            || crate::task_queue::parse_silent(&v.raw_description)
                    })
                    .collect();
            crate::telegram::commands::format_markers_list(&views)
        }
        TgCommand::Tags => {
            // /tags：统计本 chat 派单的 #tag 矩阵。read path 同 /markers /
            // /today 等：read_tg_chat_task_views 已 chat-scoped。formatter
            // 内部聚合 + 排序 + cap + 兜底。
            let views = read_tg_chat_task_views(chat_id.0);
            crate::telegram::commands::format_tags_reply(&views)
        }
        TgCommand::Mood => {
            // 心情是宠物全局状态（与 MoodWidget 同 mood state 文件），不分 chat
            // 过滤。read 失败 / 未写过 → format 函数兜底友好提示。
            let parsed = crate::mood::read_current_mood_parsed();
            crate::telegram::commands::format_mood_reply(parsed)
        }
        TgCommand::Whoami => {
            // 与桌面 chat `/whoami` 对偶：并发收集 5 个 IPC 源，每个独立兜底
            // （某一源缺失不挡整段渲染）。这些都是廉价同步 read（< 1ms），
            // 不开 async / spawn。
            let user_name = crate::commands::settings::get_user_name();
            let companionship_days =
                Some(crate::companionship::companionship_days().await);
            let mood = crate::mood::read_current_mood_parsed();
            let persona_summary = crate::commands::memory::read_ai_insights_item(
                "persona_summary",
            )
            .map(|i| i.description)
            .unwrap_or_default();
            let top_tools_raw = crate::tool_call_history::get_top_tools_used();
            let top_tools: Vec<(String, u64)> = top_tools_raw
                .into_iter()
                .map(|s| (s.name, s.count))
                .collect();
            crate::telegram::commands::format_whoami_reply(
                &user_name,
                companionship_days,
                mood,
                &persona_summary,
                &top_tools,
            )
        }
        TgCommand::Streak => {
            // 完成节奏 audit：reuse read_tg_chat_task_views（已 chat-scoped）。
            // formatter 内部 connect compute_done_streak / count_done_in_
            // window pure helpers + today 注入。
            let views = read_tg_chat_task_views(chat_id.0);
            let today = chrono::Local::now().date_naive();
            crate::telegram::commands::format_streak_reply(&views, today)
        }
        TgCommand::Yesterday => {
            // 昨日 done 视图：reuse read_tg_chat_task_views（已 chat-scoped）。
            // formatter 内部从 today 算 yesterday 边界并过滤 + sort。
            let views = read_tg_chat_task_views(chat_id.0);
            let today = chrono::Local::now().date_naive();
            crate::telegram::commands::format_yesterday_reply(&views, today)
        }
        TgCommand::TodayDone => {
            // 今日 done 视图（与 Yesterday 同模板但 scope 是今日）：reuse
            // read_tg_chat_task_views + chrono::Local today；formatter 内部
            // updated_at 前缀匹配 today_str + sort。
            let views = read_tg_chat_task_views(chat_id.0);
            let today = chrono::Local::now().date_naive();
            crate::telegram::commands::format_today_done_reply(&views, today)
        }
        TgCommand::TouchedToday => {
            // 与 TodayDone 同 read + today_str 路径；formatter 不限 status
            // 仅按 updated_at 命中今日过滤 — 「我今天动过哪些」audit。
            let views = read_tg_chat_task_views(chat_id.0);
            let today = chrono::Local::now().date_naive();
            crate::telegram::commands::format_touched_today_reply(&views, today)
        }
        TgCommand::EditTitle { title, new_title } => {
            // resolve 同 /done / /cancel / /show 三层。命中后调
            // memory_rename(butler_tasks, old, new)，新 title trim 后空由
            // backend 拒（return Err "new title must not be empty"）—
            // formatter 透显 err；同名拒 / 找不到拒同样透显。重名冲突时
            // backend 会拒（"Title already exists ..."），与 /dup 的
            // unique-filename 自动加 _N 行为不同（rename 是"显式新名"，
            // 自动改名违反 owner intent）。
            if title.trim().is_empty() || new_title.trim().is_empty() {
                format_missing_argument("edit_title")
            } else {
                let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title),
                };
                match actual {
                    Ok(src) => {
                        let rename_result = crate::commands::memory::memory_rename(
                            "butler_tasks".to_string(),
                            src.clone(),
                            new_title.clone(),
                        );
                        match rename_result {
                            Ok(_) => crate::telegram::commands::format_edit_title_reply(
                                &src,
                                &new_title,
                            ),
                            Err(e) => format_command_error(&e),
                        }
                    }
                    Err(msg) => format_command_error(&msg),
                }
            }
        }
        TgCommand::Quick { text } => {
            // 与 /task 同 backend (memory_edit("create", "butler_tasks")) 但
            // priority 始终 P3、reply 极短。空 text 走 formatter usage hint。
            let trimmed = text.trim();
            if trimmed.is_empty() {
                crate::telegram::commands::format_quick_reply(&text, Ok(()))
            } else {
                let header = crate::task_queue::TaskHeader {
                    priority: 3,
                    due: None,
                    body: String::new(),
                };
                let mut description =
                    crate::task_queue::format_task_description(&header);
                description = crate::task_queue::append_origin_marker(
                    &description,
                    &crate::task_queue::TaskOrigin::Tg(chat_id.0),
                );
                match crate::commands::memory::memory_edit(
                    "create".to_string(),
                    "butler_tasks".to_string(),
                    trimmed.to_string(),
                    Some(description),
                    Some(String::new()),
                ) {
                    Ok(_) => crate::telegram::commands::format_quick_reply(
                        &text,
                        Ok(()),
                    ),
                    Err(e) => crate::telegram::commands::format_quick_reply(
                        &text,
                        Err(&e),
                    ),
                }
            }
        }
        TgCommand::Sleep => {
            // 一键 8h mute：复用 set_mute_minutes 同后端（与 /mute 等价）。
            // format_sleep_reply 走专属温和文案。SLEEP_MUTE_MINUTES = 480。
            let minutes = crate::telegram::commands::SLEEP_MUTE_MINUTES;
            let _ = crate::proactive::set_mute_minutes(minutes);
            let until_local =
                Some(chrono::Local::now() + chrono::Duration::minutes(minutes));
            crate::telegram::commands::format_sleep_reply(until_local)
        }
        TgCommand::Random => {
            // 随机抽 active 任务：system time nanos 当 seed 拿非确定性。
            // formatter 走 `seed % candidates.len()` 索引选一条。read path
            // 同 /last / /tasks（已 chat-scoped）。
            let views = read_tg_chat_task_views(chat_id.0);
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as usize)
                .unwrap_or(0);
            crate::telegram::commands::format_random_reply(&views, seed)
        }
        TgCommand::Last => {
            // 闪查最近创建：reuse read_tg_chat_task_views（已 chat-scoped）。
            // formatter 内部 max_by created_at + 截 raw 预览。本地 now 注入
            // 让 "N 前" 文案稳定（避免运行机时间影响测试）。
            let views = read_tg_chat_task_views(chat_id.0);
            let now = chrono::Local::now().naive_local();
            crate::telegram::commands::format_last_reply(&views, now)
        }
        TgCommand::Now => {
            // 快速状态 check：复用 mood + companionship 两条 read 路径。time
            // 用 chrono::Local::now() 拿本地时间 + tz；with_timezone 转 DateTime
            // <FixedOffset> 给 pure formatter 注入便于单测稳定（FixedOffset
            // 不依赖运行机 Local::now 时间）。
            let now_local = chrono::Local::now();
            let now_fixed = now_local.with_timezone(now_local.offset());
            let companionship_days =
                Some(crate::companionship::companionship_days().await);
            let mood = crate::mood::read_current_mood_parsed();
            let mood_text = mood.as_ref().map(|(t, _)| t.as_str());
            crate::telegram::commands::format_now_reply(
                now_fixed,
                companionship_days,
                mood_text,
            )
        }
        TgCommand::LastSpeech => {
            // 最近一条主动开口：调既有 recent_speeches_with_meta(1) 拿
            // entry（ts + text）+ now 锚点交给 pure formatter。空 history
            // → entry=None，formatter 走兜底。
            let entries =
                crate::speech_history::recent_speeches_with_meta(1).await;
            let entry_opt = entries
                .first()
                .map(|e| (e.ts.as_str(), e.text.as_str()));
            let now = chrono::Local::now();
            crate::telegram::commands::format_last_speech_reply(entry_opt, now)
        }
        TgCommand::ShowSpeech { n } => {
            // 最近 N 条主动开口：调既有 recent_speeches_with_meta(n) +
            // 转 (ts, text) tuple vec 给 pure formatter。empty / IO 失败
            // 由 formatter 内部走兜底。clamp 已在 parser 完成 (1..=20)。
            let entries =
                crate::speech_history::recent_speeches_with_meta(n as usize)
                    .await;
            let tuples: Vec<(String, String)> = entries
                .into_iter()
                .map(|e| (e.ts, e.text))
                .collect();
            crate::telegram::commands::format_show_speech_reply(&tuples)
        }
        TgCommand::Today => {
            // 今日叙事视图：reuse 与 /tasks /stats 同一 read path；本地 today
            // 日期注入。views 已按 origin==Tg(chat_id) 过滤，与其它命令一致。
            let views = read_tg_chat_task_views(chat_id.0);
            let today = chrono::Local::now().date_naive();
            crate::telegram::commands::format_today_reply(&views, today)
        }
        TgCommand::Here => {
            // owner 视角信号 dump：transient_note + mute + feedback band。
            // 与 /aware 对偶 — pet 视角看的 vs owner 输入侧。
            let (tn_text, tn_until) = crate::proactive::get_transient_note();
            let now_local = chrono::Local::now();
            let transient = if tn_text.is_empty() {
                None
            } else {
                let mins = chrono::DateTime::parse_from_str(
                    &tn_until,
                    "%Y-%m-%dT%H:%M:%S%:z",
                )
                .ok()
                .map(|until| {
                    let diff = until - now_local.with_timezone(until.offset());
                    diff.num_minutes()
                })
                .unwrap_or(0);
                Some((tn_text.as_str(), mins))
            };

            // mute_remaining_seconds → Option<i64>；ceil 到分钟。
            let mute_remaining_minutes =
                crate::proactive::mute_remaining_seconds().map(|secs| {
                    ((secs + 59) / 60).max(1) // round up，clamp 最小 1
                });

            // feedback band — recent_feedback(20) 作 R7 cooldown 同窗口
            // （feedback_history 默认 cap），分类返 (&'static str, f64)
            let entries = crate::feedback_history::recent_feedback(20).await;
            let (band, _factor) =
                crate::feedback_history::classify_feedback_band(&entries);

            crate::telegram::commands::format_here_reply(
                transient,
                mute_remaining_minutes,
                band,
            )
        }
        TgCommand::Aware => {
            // pet 自述当前感知 snapshot：transient_note + active tasks + mood
            // + 时间 + 陪伴。所有 read 路径都已就绪 — 复用既有 API：
            // - proactive::get_transient_note() → (text, until_iso)
            // - memory_list("butler_tasks") → 数非 [done] item
            // - mood::read_current_mood_parsed() → (text, motion)
            // - companionship::companionship_days() → u64
            let now_local = chrono::Local::now();
            let now_fixed = now_local.with_timezone(now_local.offset());
            let companionship_days =
                Some(crate::companionship::companionship_days().await);
            let mood = crate::mood::read_current_mood_parsed();
            let mood_text = mood.as_ref().map(|(t, _)| t.as_str());

            let (tn_text, tn_until) = crate::proactive::get_transient_note();
            let transient = if tn_text.is_empty() {
                None
            } else {
                let mins = chrono::DateTime::parse_from_str(
                    &tn_until,
                    "%Y-%m-%dT%H:%M:%S%:z",
                )
                .ok()
                .map(|until| {
                    let diff = until - now_local.with_timezone(until.offset());
                    diff.num_minutes()
                })
                .unwrap_or(0);
                Some((tn_text.as_str(), mins))
            };

            let active_count = match crate::commands::memory::memory_list(
                Some("butler_tasks".to_string()),
            ) {
                Ok(index) => index
                    .categories
                    .get("butler_tasks")
                    .map(|cat| {
                        cat.items
                            .iter()
                            .filter(|it| !it.description.contains("[done]"))
                            .count()
                    })
                    .unwrap_or(0),
                Err(_) => 0,
            };

            crate::telegram::commands::format_aware_reply(
                transient,
                active_count,
                mood_text,
                now_fixed,
                companionship_days,
            )
        }
        TgCommand::Due { preset, raw_arg } => {
            // 与 /today 同 read path，本地 today 注入；formatter 内部按 preset
            // 算 [start, end] 日期范围。preset == None 时 formatter 走 usage
            // hint 回显 raw_arg。
            let views = read_tg_chat_task_views(chat_id.0);
            let today = chrono::Local::now().date_naive();
            crate::telegram::commands::format_due_reply(&views, preset, &raw_arg, today)
        }
        TgCommand::Recent { n } => {
            // 最近完成清单：reuse 同 read path（已 origin==Tg(chat_id) 过滤），
            // formatter 内部按 updated_at 倒序 + n cap。clamp 已在 parser 完
            // 成 (1..=20)。
            let views = read_tg_chat_task_views(chat_id.0);
            crate::telegram::commands::format_recent_reply(&views, n)
        }
        TgCommand::OldestN { n } => {
            // 最老 pending 清单：reuse 同 read path；formatter 内部 filter
            // pending + sort created_at asc + n cap。clamp 已在 parser 完成
            // (1..=20)。inject chrono::Local::now() 让 age label 用本机时区
            // 算「N 天前」。
            let views = read_tg_chat_task_views(chat_id.0);
            let now = chrono::Local::now().fixed_offset();
            crate::telegram::commands::format_oldest_n_reply(&views, n, now)
        }
        TgCommand::ActiveRecent { n } => {
            // 最新创建 active 清单：reuse 同 read path；formatter 内部 filter
            // pending + error + sort created_at desc + n cap。clamp 已在
            // parser 完成 (1..=20)。inject chrono::Local::now() 让 age label
            // 用本机时区算「N 天前」（与 /oldest_n 同 now-injection 模板）。
            let views = read_tg_chat_task_views(chat_id.0);
            let now = chrono::Local::now().fixed_offset();
            crate::telegram::commands::format_active_recent_reply(&views, n, now)
        }
        TgCommand::Find { keyword } => {
            // keyword 搜本 chat 派单。reuse 同 read path；空 keyword 由
            // formatter 内部走 missing-argument 模板。
            let views = read_tg_chat_task_views(chat_id.0);
            crate::telegram::commands::format_find_reply(&views, &keyword)
        }
        TgCommand::FindInDetail { keyword } => {
            // 搜每条 task 的 detail.md 内容 — handler 负责 IO（读所有
            // detail.md 文件），formatter 仅做字符串拼装。空 keyword 由
            // formatter 内部走 usage hint。
            let kw = keyword.trim().to_string();
            if kw.is_empty() {
                crate::telegram::commands::format_find_in_detail_reply(
                    &[],
                    &keyword,
                )
            } else {
                let views = read_tg_chat_task_views(chat_id.0);
                let mut hits: Vec<
                    crate::telegram::commands::FindInDetailHit,
                > = Vec::new();
                // 排序：active 在前 — pending → error → done → cancelled
                use crate::task_queue::TaskStatus;
                let status_rank = |s: &TaskStatus| match s {
                    TaskStatus::Pending => 0u8,
                    TaskStatus::Error => 1,
                    TaskStatus::Done => 2,
                    TaskStatus::Cancelled => 3,
                };
                let mut sorted: Vec<&crate::task_queue::TaskView> =
                    views.iter().collect();
                sorted.sort_by_key(|v| status_rank(&v.status));
                for v in sorted.iter() {
                    if v.detail_path.is_empty() {
                        continue;
                    }
                    let content =
                        match crate::commands::memory::memory_read_detail_full(
                            v.detail_path.clone(),
                        ) {
                            Ok(s) => s,
                            Err(_) => continue,
                        };
                    if content.is_empty() {
                        continue;
                    }
                    if let Some(snippet) =
                        crate::telegram::commands::extract_find_in_detail_snippet(
                            &content, &kw,
                        )
                    {
                        hits.push(crate::telegram::commands::FindInDetailHit {
                            title: v.title.as_str(),
                            status: v.status,
                            snippet,
                        });
                    }
                }
                crate::telegram::commands::format_find_in_detail_reply(
                    &hits, &keyword,
                )
            }
        }
        TgCommand::FindSpeech { keyword } => {
            // 搜 speech_history.log — handler 读全文 + 逐行 case-insensitive
            // 子串过滤 + 抽 ts + snippet。空 keyword 由 formatter 走 usage
            // hint。每行格式 "<RFC3339 ts> <text>"；reverse 让最新 hit 在
            // 前（owner 通常关心近期 utterance）。
            let kw = keyword.trim().to_string();
            if kw.is_empty() {
                crate::telegram::commands::format_find_speech_reply(
                    &[],
                    &keyword,
                )
            } else {
                let content =
                    crate::speech_history::read_history_content().await;
                let kw_lower = kw.to_lowercase();
                let mut hits: Vec<(String, String)> = Vec::new();
                for line in content.lines().rev() {
                    if line.is_empty() {
                        continue;
                    }
                    // 行格式：`<ts> <text>` — strip_timestamp 提供 text；
                    // ts 部分手动取首段
                    let Some((ts_str, _)) = line.split_once(' ') else {
                        continue;
                    };
                    let text =
                        crate::speech_history::strip_timestamp(line);
                    if !text.to_lowercase().contains(&kw_lower) {
                        continue;
                    }
                    // ts → 本地 MM-DD HH:MM
                    let ts_label =
                        chrono::DateTime::parse_from_rfc3339(ts_str)
                            .map(|t| {
                                t.with_timezone(&chrono::Local)
                                    .format("%m-%d %H:%M")
                                    .to_string()
                            })
                            .unwrap_or_else(|_| ts_str.to_string());
                    let Some(snippet) =
                        crate::telegram::commands::extract_find_in_detail_snippet(
                            text, &kw,
                        )
                    else {
                        continue;
                    };
                    hits.push((ts_label, snippet));
                }
                crate::telegram::commands::format_find_speech_reply(
                    &hits, &keyword,
                )
            }
        }
        TgCommand::Tag { name } => {
            // 按 #tag exact 等值匹配。reuse 同 read path；空 name 由
            // formatter 内部走 usage hint。
            let views = read_tg_chat_task_views(chat_id.0);
            crate::telegram::commands::format_tag_reply(&views, &name)
        }
        TgCommand::TagsFor { title } => {
            // 单条 task 的 #tags 清单：3 层 title resolve + formatter 直
            // 接读 target_view.tags Vec<String>。
            if title.trim().is_empty() {
                format_missing_argument("tags_for")
            } else {
                let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title),
                };
                match actual {
                    Ok(t) => {
                        let views = read_tg_chat_task_views(chat_id.0);
                        crate::telegram::commands::format_tags_for_reply(&views, &t)
                    }
                    Err(msg) => format_command_error(&msg),
                }
            }
        }
        TgCommand::Touch { title } => {
            // 刷 updated_at 不改内容 — 让老 task 重新冒头 proactive 选
            // 单。3 层 title resolve + 调 task_touch_inner（与 task_
            // skip_once 共享 backend helper 但 decision_log 标 TaskTouch
            // 区分）。done / cancelled 拒由 backend 内部 status check。
            if title.trim().is_empty() {
                crate::telegram::commands::format_touch_reply(&title, Ok(()))
            } else {
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
                        let save = crate::commands::task::task_touch_inner(
                            t.clone(),
                            decisions,
                        );
                        crate::telegram::commands::format_touch_reply(
                            &t,
                            save.as_ref().map(|_| ()).map_err(|e| e.as_str()),
                        )
                    }
                    Err(msg) => format_command_error(&msg),
                }
            }
        }
        TgCommand::Blocked => {
            // 被 blockedBy 锁住的 active task 清单。reuse 同 read path；
            // formatter 内部把 chat-scoped views 当 active 集 + 交集计算
            // unresolved blockers。
            let views = read_tg_chat_task_views(chat_id.0);
            crate::telegram::commands::format_blocked_reply(&views)
        }
        TgCommand::Forks { title } => {
            // 反向 audit：解锁 title 会松开哪些 active task。title resolve
            // 三层（数字 index → fuzzy → 错误候选）与 /show / /timeline 同
            // 源；resolved title 传给 pure formatter 在 chat-scoped views
            // 里扫 blocked_by 引用方。空 title 走 missing-arg hint。
            if title.trim().is_empty() {
                format_missing_argument("forks")
            } else {
                let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title),
                };
                match actual {
                    Ok(t) => {
                        let views = read_tg_chat_task_views(chat_id.0);
                        crate::telegram::commands::format_forks_reply(&views, &t)
                    }
                    Err(msg) => format_command_error(&msg),
                }
            }
        }
        TgCommand::BlockedBy { title } => {
            // 单条 audit：title 在等谁。3 层 title resolve 与 /forks 同源。
            // formatter 读 target view 的 blocked_by + filter 仍 active 的
            // blocker 集合。
            if title.trim().is_empty() {
                format_missing_argument("blocked_by")
            } else {
                let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title),
                };
                match actual {
                    Ok(t) => {
                        let views = read_tg_chat_task_views(chat_id.0);
                        crate::telegram::commands::format_blocked_by_reply(&views, &t)
                    }
                    Err(msg) => format_command_error(&msg),
                }
            }
        }
        TgCommand::Snoozed => {
            // 当前 [snooze: ...] 中的 task 清单。read 路径与 /pinned /
            // /silenced 同；TaskView.snoozed_until 已由 build_task_view
            // 按 active-only（now < until）填充，所以本地 filter 即可。
            let views: Vec<crate::task_queue::TaskView> =
                read_tg_chat_task_views(chat_id.0)
                    .into_iter()
                    .filter(|v| v.snoozed_until.is_some())
                    .collect();
            let now = chrono::Local::now().naive_local();
            crate::telegram::commands::format_snoozed_reply(&views, now)
        }
        TgCommand::Mute { minutes } => {
            // 复用 proactive::set_mute_minutes 同后端 — 与桌面 PanelDebug
            // "⚙️ mute" / pet ctx menu 同入口。set_mute_minutes(minutes > 0)
            // 内部 hook 也会 record_mute_engaged 让"🔕 今日 mute"chip 计数同步。
            let _ = crate::proactive::set_mute_minutes(minutes);
            let until_local = if minutes > 0 {
                Some(chrono::Local::now() + chrono::Duration::minutes(minutes))
            } else {
                None
            };
            crate::telegram::commands::format_mute_reply(minutes, until_local)
        }
        TgCommand::SleepUntil { raw } => {
            // 解析 HH:MM → 算"到 target 还剩多少分钟"→ 复用 set_mute_minutes。
            // target ≤ now → 落到明日同时刻（owner 凌晨 1 点说"到 8 点"视为
            // 今早 8:00 反直觉，所以这里只是把今日 8:00 已过的情况推到明日）。
            // 实际上 chrono.with_time(...) 拿"今日 8:00" 时若已过 now，则 +1d
            // 让 target 永远 > now。
            let parsed =
                crate::telegram::commands::parse_sleep_until_time(&raw);
            match parsed {
                None => crate::telegram::commands::format_sleep_until_reply(
                    &raw, None, 0, None, false,
                ),
                Some((h, m)) => {
                    use chrono::{Datelike, Local, TimeZone};
                    let now = Local::now();
                    let today_target = Local
                        .with_ymd_and_hms(
                            now.year(),
                            now.month(),
                            now.day(),
                            h as u32,
                            m as u32,
                            0,
                        )
                        .single();
                    let target = match today_target {
                        Some(t) if t > now => t,
                        Some(t) => t + chrono::Duration::days(1),
                        None => {
                            // 极端 DST 边界 — fallback to "now + 1h" 兜底，
                            // 让命令至少不无响应
                            now + chrono::Duration::hours(1)
                        }
                    };
                    let crosses_midnight = today_target
                        .map(|t| t <= now)
                        .unwrap_or(false);
                    let minutes =
                        (target - now).num_minutes().clamp(1, 10080);
                    let _ = crate::proactive::set_mute_minutes(minutes);
                    let until_local = Some(target);
                    crate::telegram::commands::format_sleep_until_reply(
                        &raw,
                        Some((h, m)),
                        minutes,
                        until_local,
                        crosses_midnight,
                    )
                }
            }
        }
        TgCommand::SnoozeUntil { title, time } => {
            // 与 /sleep_until 同跨日规则：HH:MM 解析为今日同时刻，已
            // 过则 +1d 落明日。time=None → 失败兜底 formatter usage。
            // 成功路径 task_set_snooze("YYYY-MM-DD HH:MM" 字符串)。
            if title.trim().is_empty() || time.is_none() {
                crate::telegram::commands::format_snooze_until_reply(
                    &title,
                    time,
                    None,
                    false,
                    Ok(()),
                )
            } else {
                use chrono::{Datelike, Local, TimeZone};
                let (h, m) = time.unwrap();
                let now = Local::now();
                let today_target = Local
                    .with_ymd_and_hms(
                        now.year(),
                        now.month(),
                        now.day(),
                        h as u32,
                        m as u32,
                        0,
                    )
                    .single();
                let target = match today_target {
                    Some(t) if t > now => t,
                    Some(t) => t + chrono::Duration::days(1),
                    None => now + chrono::Duration::hours(1),
                };
                let crosses_midnight = today_target
                    .map(|t| t <= now)
                    .unwrap_or(false);
                let until_str = target.format("%Y-%m-%d %H:%M").to_string();
                // title resolve 三层（数字 index → fuzzy → exact）：
                // 与 /snooze / /done / /cancel 同模板
                let resolved =
                    match try_resolve_by_index(&title, chat_id.0, state)
                        .await
                    {
                        Some(t) => Ok(t),
                        None => resolve_tg_task_title(&title),
                    };
                let save_ok = match resolved {
                    Ok(t) => crate::commands::task::task_set_snooze(
                        t.clone(),
                        Some(until_str.clone()),
                    )
                    .map_err(|e| e.to_string()),
                    Err(e) => Err(e),
                };
                crate::telegram::commands::format_snooze_until_reply(
                    &title,
                    Some((h, m)),
                    Some(target),
                    crosses_midnight,
                    save_ok,
                )
            }
        }
        TgCommand::Digest { n } => {
            // 最近 done 任务标题 + result 摘要清单。reuse 同 read path（与
            // /recent / /tasks / /today 同源），formatter 内部 sort updated_at
            // desc + 截 N。clamp 已在 parser 完成 (1..=20)。
            let views = read_tg_chat_task_views(chat_id.0);
            crate::telegram::commands::format_digest_reply(&views, n)
        }
        TgCommand::Show { title } => {
            // resolve 同 /done /cancel 三层（数字 index → fuzzy → 错误候选）。
            // 命中后调 task_get_detail 拿 raw_description + detail_md；查 view
            // 拿 status 给 formatter 选 emoji。task_get_detail 内部 detail.md
            // 读失败 → 空串（formatter 自动省略 detail 段）。
            if title.trim().is_empty() {
                format_missing_argument("show")
            } else {
                let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title),
                };
                match actual {
                    Ok(t) => {
                        let views = read_tg_chat_task_views(chat_id.0);
                        let status = views
                            .iter()
                            .find(|v| v.title == t)
                            .map(|v| v.status)
                            .unwrap_or(crate::task_queue::TaskStatus::Pending);
                        match crate::commands::task::task_get_detail(t.clone()).await {
                            Ok(detail) => {
                                crate::telegram::commands::format_show_reply(
                                    &detail.title,
                                    &detail.raw_description,
                                    &detail.detail_md,
                                    status,
                                )
                            }
                            Err(e) => format_command_error(&e),
                        }
                    }
                    Err(msg) => format_command_error(&msg),
                }
            }
        }
        TgCommand::Peek { title } => {
            // resolve 同 /show 三层（数字 index → fuzzy → 错误候选）。命中后查
            // view 拿 raw_description + status 给 formatter — 不读 detail.md
            // （紧凑视图不需要，省一次 IO）。
            if title.trim().is_empty() {
                format_missing_argument("peek")
            } else {
                let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title),
                };
                match actual {
                    Ok(t) => {
                        let views = read_tg_chat_task_views(chat_id.0);
                        match views.iter().find(|v| v.title == t) {
                            Some(v) => crate::telegram::commands::format_peek_reply(
                                &v.title,
                                &v.raw_description,
                                v.status,
                            ),
                            None => format_command_error(&format!("找不到 task「{}」", t)),
                        }
                    }
                    Err(msg) => format_command_error(&msg),
                }
            }
        }
        TgCommand::Dup { title } => {
            // resolve 同 /show /peek 三层。命中后取 view 拿 raw_description，
            // parse_task_header 抽 priority / due / body，strip_for_dup 去除
            // 终态 markers，task_create 写新 task。
            //
            // 标题冲突由 memory_edit 内置 unique-filename 兜底（自动加 _N
            // 后缀） — 不在前端去重。
            //
            // due 透传 ISO 字符串：原 view 的 due 是 NaiveDateTime，
            // task_create 接 `YYYY-MM-DDThh:mm` 字符串，本地 format 即可。
            if title.trim().is_empty() {
                format_missing_argument("dup")
            } else {
                let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title),
                };
                match actual {
                    Ok(src) => {
                        let views = read_tg_chat_task_views(chat_id.0);
                        let view_opt = views.iter().find(|v| v.title == src).cloned();
                        match view_opt {
                            None => format_command_error(&format!("找不到 task「{}」", src)),
                            Some(view) => {
                                // 解析源 raw_description：拿 priority / due / body
                                let parsed = crate::task_queue::parse_task_header(
                                    &view.raw_description,
                                );
                                let (priority, due, body_raw) = match parsed {
                                    Some(h) => (h.priority, h.due, h.body),
                                    // 无 header 兜底 — 整段当 body，P3 默认
                                    None => (3, None, view.raw_description.clone()),
                                };
                                let cleaned_body =
                                    crate::task_queue::strip_for_dup(&body_raw);
                                let new_title = format!("{} (副本)", src);
                                let due_iso =
                                    due.map(|d| d.format("%Y-%m-%dT%H:%M").to_string());
                                let create_result = crate::commands::task::task_create(
                                    crate::commands::task::TaskCreateArgs {
                                        title: new_title.clone(),
                                        body: cleaned_body,
                                        priority,
                                        due: due_iso,
                                    },
                                );
                                match create_result {
                                    Ok(actual_new_title) => {
                                        crate::telegram::commands::format_dup_reply(
                                            &src,
                                            &actual_new_title,
                                        )
                                    }
                                    Err(e) => format_command_error(&e),
                                }
                            }
                        }
                    }
                    Err(msg) => format_command_error(&msg),
                }
            }
        }
        TgCommand::Snippets => {
            // 与 /pinned / /silenced 同模板：read_tg_chat_task_views + 含
            // [snippet] / [snippet: <label>] marker filter。formatter 内部
            // 渲染（含空集兜底）。
            let views: Vec<crate::task_queue::TaskView> = read_tg_chat_task_views(chat_id.0)
                .into_iter()
                .filter(|v| {
                    crate::telegram::commands::parse_snippet_marker(&v.raw_description)
                        .is_some()
                })
                .collect();
            crate::telegram::commands::format_snippets_reply(&views)
        }
        TgCommand::RecentEvents { title, n } => {
            // 与 Timeline 同 resolve + 同底层 entries 路径；区别仅在 formatter
            // 取末尾 N 而非前 30。共享 compute_timeline_entries 让两个命令
            // 行为 / 去重逻辑天然一致。
            if title.trim().is_empty() {
                format_missing_argument("recent_events")
            } else {
                let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title),
                };
                match actual {
                    Ok(t) => match crate::commands::task::task_get_detail(t.clone()).await {
                        Ok(detail) => {
                            let raw_events: Vec<(String, String, String)> = detail
                                .history
                                .iter()
                                .map(|e| (e.timestamp.clone(), e.action.clone(), e.snippet.clone()))
                                .collect();
                            let entries =
                                crate::telegram::commands::compute_timeline_entries(&raw_events);
                            crate::telegram::commands::format_recent_events_reply(
                                &detail.title,
                                &entries,
                                raw_events.len(),
                                n,
                            )
                        }
                        Err(e) => format_command_error(&e),
                    },
                    Err(msg) => format_command_error(&msg),
                }
            }
        }
        TgCommand::Timeline { title } => {
            // 与 Show 同 resolve 三层。命中后调 task_get_detail 拿 history（已
            // newest-first 排好），扫 markers 算 entries（旧→新 + 去重无变化
            // update）→ formatter 拼回复。task_get_detail 内部 history 读失
            // 败时返空 vec（NotFound / 路径解析失败已兜底），formatter 会渲
            // "无 history 记录"友好文案。
            if title.trim().is_empty() {
                format_missing_argument("timeline")
            } else {
                let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title),
                };
                match actual {
                    Ok(t) => match crate::commands::task::task_get_detail(t.clone()).await {
                        Ok(detail) => {
                            let raw_events: Vec<(String, String, String)> = detail
                                .history
                                .iter()
                                .map(|e| (e.timestamp.clone(), e.action.clone(), e.snippet.clone()))
                                .collect();
                            let entries =
                                crate::telegram::commands::compute_timeline_entries(&raw_events);
                            crate::telegram::commands::format_timeline_reply(
                                &detail.title,
                                &entries,
                                raw_events.len(),
                            )
                        }
                        Err(e) => format_command_error(&e),
                    },
                    Err(msg) => format_command_error(&msg),
                }
            }
        }
        TgCommand::Demote { title } => {
            // priority -1 (clamp 0)。与 Promote arm 对偶。已 P0 short-circuit
            // no-op friendly reply 不调 backend。
            if title.trim().is_empty() {
                format_missing_argument("demote")
            } else {
                let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title),
                };
                match actual {
                    Ok(t) => {
                        let views = read_tg_chat_task_views(chat_id.0);
                        let old = views
                            .iter()
                            .find(|v| v.title == t)
                            .map(|v| v.priority);
                        match old {
                            Some(0) => crate::telegram::commands::format_demote_reply(
                                &t, Some(0), Ok(()),
                            ),
                            Some(o) => {
                                let new_pri = o.saturating_sub(1);
                                match crate::commands::task::task_set_priority(
                                    t.clone(),
                                    new_pri,
                                ) {
                                    Ok(()) => crate::telegram::commands::format_demote_reply(
                                        &t,
                                        Some(o),
                                        Ok(()),
                                    ),
                                    Err(e) => crate::telegram::commands::format_demote_reply(
                                        &t,
                                        Some(o),
                                        Err(&e),
                                    ),
                                }
                            }
                            None => crate::telegram::commands::format_demote_reply(
                                &t, None, Ok(()),
                            ),
                        }
                    }
                    Err(msg) => format_command_error(&msg),
                }
            }
        }
        TgCommand::Promote { title } => {
            // priority +1 (clamp 9)。三层 resolve title → 查 view 拿 current
            // priority → 算 new = old + 1（clamp）→ 调 task_set_priority。
            // 已是 P9 时 short-circuit 直接走 formatter no-op 文案。
            if title.trim().is_empty() {
                format_missing_argument("promote")
            } else {
                let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title),
                };
                match actual {
                    Ok(t) => {
                        // 拉 current priority — 从 chat views 找；找不到时 None
                        // 让 formatter 走 fallback 简短文案。
                        let views = read_tg_chat_task_views(chat_id.0);
                        let old = views
                            .iter()
                            .find(|v| v.title == t)
                            .map(|v| v.priority);
                        match old {
                            Some(9) => {
                                // 已 P9 — no-op 友好 reply 不调 backend
                                crate::telegram::commands::format_promote_reply(
                                    &t, Some(9), Ok(()),
                                )
                            }
                            Some(o) => {
                                let new_pri = o.saturating_add(1).min(9);
                                match crate::commands::task::task_set_priority(
                                    t.clone(),
                                    new_pri,
                                ) {
                                    Ok(()) => crate::telegram::commands::format_promote_reply(
                                        &t,
                                        Some(o),
                                        Ok(()),
                                    ),
                                    Err(e) => crate::telegram::commands::format_promote_reply(
                                        &t,
                                        Some(o),
                                        Err(&e),
                                    ),
                                }
                            }
                            None => {
                                // 查不到 priority — view miss 极少；不阻断主路径
                                crate::telegram::commands::format_promote_reply(
                                    &t, None, Ok(()),
                                )
                            }
                        }
                    }
                    Err(msg) => format_command_error(&msg),
                }
            }
        }
        TgCommand::PromoteAllP7 { confirmed } => {
            // 紧急 sprint：扫本 chat 派单的 active task（pending / error）
            // + filter priority < 7（已 ≥ P7 跳过避免无意义写）→ confirm
            // 时逐条 task_set_priority(t, old+1) clamp 7。失败计入 err 不
            // 阻断后续。formatter 走 pure path 返计数 reply。
            let views = read_tg_chat_task_views(chat_id.0);
            let candidates: Vec<(String, u8)> = views
                .iter()
                .filter(|v| matches!(
                    v.status,
                    crate::task_queue::TaskStatus::Pending
                        | crate::task_queue::TaskStatus::Error
                ))
                .filter(|v| v.priority < 7)
                .map(|v| (v.title.clone(), v.priority))
                .collect();
            let total = candidates.len() as u32;
            if !confirmed {
                crate::telegram::commands::format_promote_all_p7_reply(
                    false, total, 0, 0,
                )
            } else {
                let mut ok = 0u32;
                let mut err = 0u32;
                for (title, old) in &candidates {
                    let new_pri = (*old).saturating_add(1).min(7);
                    match crate::commands::task::task_set_priority(
                        title.clone(),
                        new_pri,
                    ) {
                        Ok(()) => ok += 1,
                        Err(_) => err += 1,
                    }
                }
                crate::telegram::commands::format_promote_all_p7_reply(
                    true, total, ok, err,
                )
            }
        }
        TgCommand::TouchAllP7 { confirmed } => {
            // 与 /promote_all_p7 对偶：扫 active task（pending / error）
            // + filter priority >= 7（已 P7+ 的批量唤醒），逐条调
            // task_touch_inner rewrite description → memory_edit 自动
            // stamp updated_at。失败计入 err 不阻断。
            let views = read_tg_chat_task_views(chat_id.0);
            let candidates: Vec<String> = views
                .iter()
                .filter(|v| matches!(
                    v.status,
                    crate::task_queue::TaskStatus::Pending
                        | crate::task_queue::TaskStatus::Error
                ))
                .filter(|v| v.priority >= 7)
                .map(|v| v.title.clone())
                .collect();
            let total = candidates.len() as u32;
            if !confirmed {
                crate::telegram::commands::format_touch_all_p7_reply(
                    false, total, 0, 0,
                )
            } else {
                let decisions = state
                    .app
                    .state::<crate::decision_log::DecisionLogStore>()
                    .inner()
                    .clone();
                let mut ok = 0u32;
                let mut err = 0u32;
                for title in &candidates {
                    match crate::commands::task::task_touch_inner(
                        title.clone(),
                        decisions.clone(),
                    ) {
                        Ok(()) => ok += 1,
                        Err(_) => err += 1,
                    }
                }
                crate::telegram::commands::format_touch_all_p7_reply(
                    true, total, ok, err,
                )
            }
        }
        TgCommand::PinAllP7 { confirmed } => {
            // P7+ 批量族第三条：扫 active task（pending / error）+ filter
            // priority >= 7 + 未 [pinned]（已 pinned 跳过避免无意义写）→
            // confirm 时逐条 task_set_pinned(title, true)。失败计入 err
            // 不阻断。
            let views = read_tg_chat_task_views(chat_id.0);
            let candidates: Vec<String> = views
                .iter()
                .filter(|v| matches!(
                    v.status,
                    crate::task_queue::TaskStatus::Pending
                        | crate::task_queue::TaskStatus::Error
                ))
                .filter(|v| v.priority >= 7)
                .filter(|v| !v.pinned)
                .map(|v| v.title.clone())
                .collect();
            let total = candidates.len() as u32;
            if !confirmed {
                crate::telegram::commands::format_pin_all_p7_reply(
                    false, total, 0, 0,
                )
            } else {
                let mut ok = 0u32;
                let mut err = 0u32;
                for title in &candidates {
                    match crate::commands::task::task_set_pinned(
                        title.clone(),
                        true,
                    ) {
                        Ok(()) => ok += 1,
                        Err(_) => err += 1,
                    }
                }
                crate::telegram::commands::format_pin_all_p7_reply(
                    true, total, ok, err,
                )
            }
        }
        TgCommand::ConsolidateNow { confirmed } => {
            // TG 端手动触发 consolidate sweep — 与桌面 PanelMemory「立即
            // 整理」/ PanelDebug「🧹 force consolidate」同后端
            // trigger_consolidate(app)。confirm 模板防误触；confirmed=true
            // 时 await sweep + 把 Result<String, String> 交给 formatter。
            if !confirmed {
                crate::telegram::commands::format_consolidate_now_reply(
                    false, None,
                )
            } else {
                let app = state.app.clone();
                let result =
                    crate::consolidate::trigger_consolidate(app).await;
                crate::telegram::commands::format_consolidate_now_reply(
                    true,
                    Some(result),
                )
            }
        }
        TgCommand::CancelAllError { confirmed } => {
            // 扫本 chat 派单中的 error 任务（按 Tg(chat_id) origin 过滤）
            let views = read_tg_chat_task_views(chat_id.0);
            let error_titles: Vec<String> = views
                .iter()
                .filter(|v| matches!(v.status, crate::task_queue::TaskStatus::Error))
                .map(|v| v.title.clone())
                .collect();
            let total = error_titles.len() as u32;
            if !confirmed {
                crate::telegram::commands::format_cancel_all_error_reply(
                    false, total, 0, 0,
                )
            } else {
                // 逐条 cancel；失败计入 err 不阻断后续。
                let decisions = state
                    .app
                    .state::<crate::decision_log::DecisionLogStore>()
                    .inner()
                    .clone();
                let mut ok = 0u32;
                let mut err = 0u32;
                for title in &error_titles {
                    match crate::commands::task::task_cancel_inner(
                        title.clone(),
                        String::new(),
                        decisions.clone(),
                    ) {
                        Ok(()) => ok += 1,
                        Err(_) => err += 1,
                    }
                }
                crate::telegram::commands::format_cancel_all_error_reply(
                    true, total, ok, err,
                )
            }
        }
        TgCommand::Feedback { text } => {
            // owner 主动反馈：写 feedback_history.log（FeedbackKind::Comment）。
            // 空 text 由 formatter 走 usage hint。
            let trimmed = text.trim();
            if trimmed.is_empty() {
                crate::telegram::commands::format_feedback_reply(&text)
            } else {
                // record_event best-effort (写盘失败也不阻塞 reply)
                crate::feedback_history::record_event(
                    crate::feedback_history::FeedbackKind::Comment,
                    trimmed,
                )
                .await;
                crate::telegram::commands::format_feedback_reply(&text)
            }
        }
        TgCommand::RecentChats { n } => {
            // 读 active session → 过滤 user/assistant items → 取最后 N
            // → truncate excerpt → 调 formatter。session 不存在 / 空时
            // formatter 走 bootstrap hint。
            let idx = crate::commands::session::list_sessions();
            if idx.active_id.is_empty() {
                crate::telegram::commands::format_recent_chats_reply(
                    &[],
                    "",
                    "",
                    n,
                    0,
                )
            } else {
                match crate::commands::session::load_session(idx.active_id.clone()) {
                    Ok(session) => {
                        let cap = crate::telegram::commands::RECENT_CHATS_EXCERPT_CHARS;
                        let mut all: Vec<(String, String)> = session
                            .items
                            .iter()
                            .filter_map(|item| {
                                let obj = item.as_object()?;
                                let t = obj.get("type")?.as_str()?;
                                if t != "user" && t != "assistant" {
                                    return None;
                                }
                                let content =
                                    obj.get("content")?.as_str()?.to_string();
                                let flat = content.replace(['\n', '\r'], " ");
                                let trimmed =
                                    flat.split_whitespace().collect::<Vec<_>>().join(" ");
                                let chars: Vec<char> = trimmed.chars().collect();
                                let excerpt = if chars.len() > cap {
                                    let head: String = chars.iter().take(cap).collect();
                                    format!("{}…", head)
                                } else {
                                    trimmed
                                };
                                Some((t.to_string(), excerpt))
                            })
                            .collect();
                        let total = all.len();
                        let want = (n as usize).max(1);
                        if all.len() > want {
                            let drop = all.len() - want;
                            all.drain(0..drop);
                        }
                        crate::telegram::commands::format_recent_chats_reply(
                            &all,
                            &session.title,
                            &session.updated_at,
                            n,
                            total,
                        )
                    }
                    Err(_) => crate::telegram::commands::format_recent_chats_reply(
                        &[], "", "", n, 0,
                    ),
                }
            }
        }
        TgCommand::Alarms { n } => {
            // 读 todo memory items → 过滤含 [remind: ...] 协议条目 →
            // 收集 (target, topic, title) → 按 target 升序排（最近 fire
            // 在前）→ 传 formatter（cap N + 渲染剩余分钟/逾期）。
            let items = crate::db::todos_as_memory_items();
            let mut rows: Vec<(
                crate::proactive::ReminderTarget,
                String,
                String,
            )> = items
                .iter()
                .filter_map(|item| {
                    crate::proactive::parse_reminder_prefix(&item.description)
                        .map(|(target, topic)| (target, topic, item.title.clone()))
                })
                .collect();
            let now = chrono::Local::now().naive_local();
            // sort 按 absolute target 升序。TodayHour 视作"今日 HH:MM"
            // 落到日期 — 与 formatter 同语义。
            rows.sort_by_key(|(t, _, _)| match t {
                crate::proactive::ReminderTarget::Absolute(dt) => *dt,
                crate::proactive::ReminderTarget::TodayHour(h, m) => now
                    .date()
                    .and_hms_opt(*h as u32, *m as u32, 0)
                    .unwrap_or(now),
            });
            crate::telegram::commands::format_alarms_reply(&rows, now, n)
        }
        TgCommand::SilentAll { minutes } => {
            // minutes == 0 → 仅 release active 窗口（与 /mute 0 同协议）。
            // minutes > 0 → arm 新窗口：先 release prior（如果有），再扫
            // butler_tasks pending + !silent candidates 应用 markers + spawn timer。
            if minutes == 0 {
                let released = crate::telegram::bulk_silent::release_active();
                crate::telegram::commands::format_silent_all_reply(
                    0,
                    released.map(|v| v.len()).unwrap_or(0),
                    0,
                    None,
                )
            } else {
                // 先记录 release count（如果 arm 前有 prior 窗口）
                let prior_count = crate::telegram::bulk_silent::snapshot()
                    .map(|s| s.titles.len())
                    .unwrap_or(0);
                // 扫 butler_tasks pending && !silent 候选
                let candidates: Vec<String> = match crate::commands::memory::memory_list(
                    Some("butler_tasks".to_string()),
                ) {
                    Ok(index) => index
                        .categories
                        .get("butler_tasks")
                        .map(|cat| {
                            cat.items
                                .iter()
                                .filter(|it| {
                                    // 排除 [done] 已结 + 已 [silent] 的
                                    !it.description.contains("[done]")
                                        && !it.description.contains("[silent]")
                                })
                                .map(|it| it.title.clone())
                                .collect()
                        })
                        .unwrap_or_default(),
                    Err(_) => Vec::new(),
                };
                match crate::telegram::bulk_silent::arm(candidates, minutes) {
                    Ok(state) => crate::telegram::commands::format_silent_all_reply(
                        state.titles.len(),
                        prior_count,
                        minutes,
                        Some(state.expires_at),
                    ),
                    Err(_) => {
                        // arm 失败：要么 candidates 空（无可 silent），要么
                        // task_set_silent 全失败。formatter armed_count=0 走友
                        // 好兜底。
                        crate::telegram::commands::format_silent_all_reply(
                            0,
                            prior_count,
                            minutes,
                            None,
                        )
                    }
                }
            }
        }
        TgCommand::FeedbackHistory { n } => {
            // 读取最近 n 条 feedback_history.log 条目（recent_feedback 返
            // oldest-first），reverse 让"最新一条"在 TG 屏顶。format
            // helper 接 newest-first slice。clamp 已在 parser 完成 (1..=20)。
            let mut entries =
                crate::feedback_history::recent_feedback(n as usize).await;
            entries.reverse();
            crate::telegram::commands::format_feedback_history_reply(&entries, n)
        }
        TgCommand::Transient { text, minutes } => {
            // 写 in-memory transient_note：复用 proactive::set_transient_note。
            // 空 text 由 formatter 走 usage hint，不调 backend。
            let trimmed = text.trim();
            if trimmed.is_empty() {
                crate::telegram::commands::format_transient_reply(&text, minutes, None)
            } else {
                let until_iso = crate::proactive::set_transient_note(
                    trimmed.to_string(),
                    minutes,
                );
                // until_iso 形如 "2026-05-17T18:45:00+08:00"。parse 回
                // DateTime<Local> 给 formatter 渲染 "HH:MM"；解析失败
                // （理论不会，但 defensive）→ None fallback。
                let until_local = chrono::DateTime::parse_from_str(
                    &until_iso,
                    "%Y-%m-%dT%H:%M:%S%:z",
                )
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Local));
                crate::telegram::commands::format_transient_reply(
                    &text,
                    minutes,
                    until_local,
                )
            }
        }
        TgCommand::EditDue { title, preset } => {
            // 用 friendly preset 改 due。computed = compute_edit_due_preset
            // 解出的 NaiveDateTime；Clear preset 返 None；caller 转
            // task_set_due Option<String>。空 title / 无效 preset →
            // formatter 走 usage hint。
            if title.trim().is_empty() || preset.is_none() {
                crate::telegram::commands::format_edit_due_reply(
                    &title,
                    preset.as_ref(),
                    None,
                    Ok(()),
                )
            } else {
                let now = chrono::Local::now().naive_local();
                let preset_ref = preset.as_ref().expect("checked Some above");
                let computed = crate::telegram::commands::compute_edit_due_preset(
                    preset_ref, now,
                );
                let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title),
                };
                match actual {
                    Ok(t) => {
                        let due_str = computed.map(|dt| {
                            dt.format("%Y-%m-%dT%H:%M").to_string()
                        });
                        match crate::commands::task::task_set_due(t.clone(), due_str) {
                            Ok(()) => crate::telegram::commands::format_edit_due_reply(
                                &t,
                                Some(preset_ref),
                                computed,
                                Ok(()),
                            ),
                            Err(e) => crate::telegram::commands::format_edit_due_reply(
                                &t,
                                Some(preset_ref),
                                computed,
                                Err(&e),
                            ),
                        }
                    }
                    Err(msg) => format_command_error(&msg),
                }
            }
        }
        TgCommand::Pri { title, priority } => {
            // 单改 priority — 走 task_set_priority 同后端（保 due / body /
            // 其它 markers 不动）。空 title / 无 priority → formatter usage
            // hint。title resolve 与 /done /cancel 同三层。
            if title.trim().is_empty() || priority.is_none() {
                crate::telegram::commands::format_pri_reply(&title, priority, Ok(()))
            } else {
                let pri = priority.unwrap();
                let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title),
                };
                match actual {
                    Ok(t) => {
                        match crate::commands::task::task_set_priority(t.clone(), pri) {
                            Ok(()) => crate::telegram::commands::format_pri_reply(
                                &t,
                                Some(pri),
                                Ok(()),
                            ),
                            Err(e) => crate::telegram::commands::format_pri_reply(
                                &t,
                                Some(pri),
                                Err(&e),
                            ),
                        }
                    }
                    Err(msg) => format_command_error(&msg),
                }
            }
        }
        TgCommand::SwapPriority { title_a, title_b } => {
            // 两 title 各自走 try_resolve_by_index + resolve_tg_task_title
            // 三层 fuzzy；任一端 trim 后空 → formatter 走 usage hint。
            // resolve 成功后读 pre-swap priority；对称写两次 task_set_priority
            // (a → pre_b, b → pre_a)。失败 per-step 累计；formatter 渲清晰
            // 报告。
            if title_a.trim().is_empty() || title_b.trim().is_empty() {
                crate::telegram::commands::format_swap_priority_reply(
                    &title_a, &title_b, None, None, Ok(()), Ok(()),
                )
            } else {
                let actual_a = match try_resolve_by_index(&title_a, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title_a),
                };
                let actual_b = match try_resolve_by_index(&title_b, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title_b),
                };
                match (actual_a, actual_b) {
                    (Ok(ta), Ok(tb)) => {
                        let views = read_tg_chat_task_views(chat_id.0);
                        let pri_a =
                            views.iter().find(|v| v.title == ta).map(|v| v.priority);
                        let pri_b =
                            views.iter().find(|v| v.title == tb).map(|v| v.priority);
                        if let (Some(a_val), Some(b_val)) = (pri_a, pri_b) {
                            // 已是相同 priority 时仍执行写入（无副作用 + 显
                            // success 让 owner 知道命中），但 formatter 文
                            // 案体现 "a → b, b → a" 即使值相同。
                            let save_a = crate::commands::task::task_set_priority(
                                ta.clone(),
                                b_val,
                            );
                            let save_b = crate::commands::task::task_set_priority(
                                tb.clone(),
                                a_val,
                            );
                            crate::telegram::commands::format_swap_priority_reply(
                                &ta,
                                &tb,
                                Some(a_val),
                                Some(b_val),
                                save_a.as_ref().map(|_| ()).map_err(|e| e.as_str()),
                                save_b.as_ref().map(|_| ()).map_err(|e| e.as_str()),
                            )
                        } else {
                            crate::telegram::commands::format_swap_priority_reply(
                                &ta, &tb, pri_a, pri_b, Ok(()), Ok(()),
                            )
                        }
                    }
                    (Err(msg), _) | (_, Err(msg)) => format_command_error(&msg),
                }
            }
        }
        TgCommand::Edit { title, new_desc } => {
            // 空 title / 空 new_desc → formatter 走 usage hint 路径，不真改。
            if title.trim().is_empty() || new_desc.trim().is_empty() {
                crate::telegram::commands::format_edit_reply(
                    &title,
                    &new_desc,
                    Ok(()),
                )
            } else {
                // resolve 与 /done / /cancel 同三层：数字 index → fuzzy → 错误候选。
                let actual = match try_resolve_by_index(&title, chat_id.0, state).await {
                    Some(t) => Ok(t),
                    None => resolve_tg_task_title(&title),
                };
                match actual {
                    Ok(t) => {
                        // 全量覆写描述：memory_edit("update") 在 butler_tasks
                        // category 内查 title → 写 description → 同步 SQLite
                        // mirror + butler_history 由调用链下游 hook 自动跟进。
                        match crate::commands::memory::memory_edit(
                            "update".to_string(),
                            "butler_tasks".to_string(),
                            t.clone(),
                            Some(new_desc.clone()),
                            None,
                        ) {
                            Ok(_) => crate::telegram::commands::format_edit_reply(
                                &t,
                                &new_desc,
                                Ok(()),
                            ),
                            Err(e) => crate::telegram::commands::format_edit_reply(
                                &t,
                                &new_desc,
                                Err(&e),
                            ),
                        }
                    }
                    Err(msg) => crate::telegram::commands::format_command_error(&msg),
                }
            }
        }
        TgCommand::Reflect { text } => {
            // 与 /note 同模板但 category="ai_insights" + title prefix="reflect-"。
            // 空 text → formatter 走 usage hint 路径，不真创建。
            let trimmed = text.trim();
            if trimmed.is_empty() {
                crate::telegram::commands::format_reflect_reply(&text, Ok(""))
            } else {
                let title = format!(
                    "reflect-{}",
                    chrono::Local::now().format("%Y-%m-%dT%H-%M-%S")
                );
                match crate::commands::memory::memory_edit(
                    "create".to_string(),
                    "ai_insights".to_string(),
                    title.clone(),
                    Some(trimmed.to_string()),
                    None,
                ) {
                    Ok(_) => crate::telegram::commands::format_reflect_reply(
                        &text,
                        Ok(&title),
                    ),
                    Err(e) => crate::telegram::commands::format_reflect_reply(
                        &text,
                        Err(&e),
                    ),
                }
            }
        }
        TgCommand::Note { text } => {
            // 空 text → formatter 走 usage hint 路径，不真创建。
            let trimmed = text.trim();
            if trimmed.is_empty() {
                crate::telegram::commands::format_note_reply(&text, Ok(""))
            } else {
                // 标题用本地时间秒精度，避免同分钟内多 /note 撞名。
                let title = format!(
                    "note-{}",
                    chrono::Local::now().format("%Y-%m-%dT%H-%M-%S")
                );
                match crate::commands::memory::memory_edit(
                    "create".to_string(),
                    "general".to_string(),
                    title.clone(),
                    Some(trimmed.to_string()),
                    None,
                ) {
                    Ok(_) => crate::telegram::commands::format_note_reply(
                        &text,
                        Ok(&title),
                    ),
                    Err(e) => crate::telegram::commands::format_note_reply(
                        &text,
                        Err(&e),
                    ),
                }
            }
        }
        TgCommand::Version => {
            // app_version 走编译期 env，schema_version 走 _migrations 最大 version。
            // 单 SQL 查不引入新 Tauri 命令；读失败 → 0（format 时省略 schema 行）。
            let schema_version = crate::db::with_db(|conn| {
                let v: i32 = conn
                    .query_row(
                        "SELECT COALESCE(MAX(version), 0) FROM _migrations",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                Ok(v)
            })
            .unwrap_or(0);
            crate::telegram::commands::format_version_reply(
                env!("CARGO_PKG_VERSION"),
                schema_version,
            )
        }
        TgCommand::Reset => {
            // 清掉 LLM 对话上下文：仅保留 role=="system" 的消息（人设 / SOUL
            // 提示）。同时持久化让 bot 重启后仍是 system-only。不动 last_tasks
            // 缓存（与 LLM 上下文正交）。
            let kept_system = {
                let mut msgs = state.session_messages.lock().await;
                let system: Vec<serde_json::Value> = msgs
                    .iter()
                    .filter(|m| {
                        m.get("role")
                            .and_then(|r| r.as_str())
                            .map(|r| r == "system")
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect();
                *msgs = system.clone();
                system
            };
            let now = chrono::Local::now()
                .format("%Y-%m-%dT%H:%M:%S%.3f")
                .to_string();
            let s = crate::commands::session::Session {
                id: state.session_id.clone(),
                title: "Telegram".to_string(),
                created_at: String::new(),
                updated_at: now,
                messages: kept_system,
                items: vec![],
            };
            if let Err(e) = crate::commands::session::save_session(s) {
                eprintln!("session save after /reset failed (best-effort): {e}");
            }
            crate::telegram::commands::format_reset_reply()
        }
        TgCommand::Help { topic } => match topic {
            Some(t) => crate::telegram::commands::format_help_for_topic(
                &t,
                &state.custom_command_objects,
            ),
            None => format_help_text(&state.custom_command_objects),
        },
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

/// 读 butler_tasks → 过滤 origin==Tg(chat_id) → build_task_view → 按 queue
/// 顺序排序的 views 列表。`/tasks` `/stats` 共用此读路径，不再各自拷一份过滤
/// 逻辑。memory_list 失败 / 类目缺失视作"无任务"，返回空 Vec。
fn read_tg_chat_task_views(chat_id: i64) -> Vec<crate::task_queue::TaskView> {
    let Ok(index) = crate::commands::memory::memory_list(Some("butler_tasks".to_string())) else {
        return Vec::new();
    };
    let Some(cat) = index.categories.get("butler_tasks") else {
        return Vec::new();
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
    views
}

/// 返回 `(body, ordered_titles)`：body 是给 TG 用户看的完整文本，ordered_titles
/// 是同显示顺序的 title vec（给 `/cancel N` / `/retry N` 解析序号用）。
/// 显示顺序遵循 `format_tasks_list` 的 section 排列（Pending → Done →
/// Error → Cancelled，section 内沿用 `compare_for_queue`）。
fn format_tasks_for_chat(chat_id: i64) -> (String, Vec<String>) {
    let views = read_tg_chat_task_views(chat_id);
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
