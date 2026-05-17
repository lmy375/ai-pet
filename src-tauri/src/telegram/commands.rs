//! Telegram 命令解析 + 回复文案（pure）。
//!
//! 与 `bot.rs` 的关系：
//! - 本模块**只**装"是不是命令、是哪条命令、参数是什么"的纯逻辑 + 文案
//!   formatter，不做 IO。
//! - `bot.rs::handle_message` 在收到消息后先调 `parse_tg_command`，命中
//!   就走 `handle_tg_command` 跳过 chat pipeline；未命中走原 chat pipeline。
//!
//! 命令一览：
//! - `/cancel <title>` —— 把任务标 cancelled（无原因）
//! - `/retry <title>` —— 把 error 任务重置为 pending
//! - `/tasks` —— 列出当前 chat 派出的任务清单（按状态分组）
//! - `/help` —— 列出全部命令清单
//!
//! 不识别的 `/xxx` 视作 `Unknown { name }`，由 handler 回一条简短"未知
//! 命令"提示并指向 `/help` 而非静默吞掉。

/// `/due` 命令的 preset 维度。绑定 caller 的 "今天" 后展开为具体 date
/// range（pure formatter 内做，避免 parser 拿运行时时间，便于单测）。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DuePreset {
    /// 明天：today + 1 day。
    Tomorrow,
    /// 本周：包含 today 在内的 Mon..=Sun（ISO 周）。已过去的工作日仍算
    /// 在内（owner 想 audit "本周还剩什么 due"），由 formatter 加 hint。
    ThisWeek,
    /// 下周：本周 Sun 之后的 Mon..=Sun。
    NextWeek,
}

/// pure：识别 owner 输入的 preset 字符串。中英 alias 同表；大小写不敏感。
/// 未识别返 None 让 handler 走 usage hint。
pub fn parse_due_preset(s: &str) -> Option<DuePreset> {
    let lower = s.trim().to_lowercase();
    match lower.as_str() {
        "tomorrow" | "tmr" | "tm" | "明天" | "明日" => Some(DuePreset::Tomorrow),
        "thisweek" | "this-week" | "this_week" | "week" | "本周" | "这周" => {
            Some(DuePreset::ThisWeek)
        }
        "nextweek" | "next-week" | "next_week" | "下周" => Some(DuePreset::NextWeek),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TgCommand {
    Cancel { title: String },
    Retry { title: String },
    /// `/done <title>` —— 把 pending / error 任务标 done。result 摘要走桌面
    /// 面板单条 mark-done dialog；TG 单行命令只支持空 result 路径（与键盘 `d`
    /// 等价）。终态任务被拒（与桌面 task_mark_done 同策略）。
    Done { title: String },
    /// `/task <title>` —— 单数，**创建**一条任务。与复数 `/tasks`（列表）
    /// 区分。空 title 由 handler 走 missing-argument 反馈。
    /// `priority` 由 `parse_task_prefix` 解析得出：默认 3 / `!!` 5 / `!!!` 7。
    Task { title: String, priority: u8 },
    Tasks,
    /// `/stats` —— 一行汇总当前 chat 派出的任务状态计数（待办 / 逾期 /
    /// 今日完成 / 出错 / 今日取消）。无参；对账 / 周末扫盘子的快速入口。
    Stats,
    /// `/mood` —— 查看宠物当前心情。无参；与桌面 MoodWidget 同源（mood
    /// state 文件），让手机端也能感知"宠物现在感觉如何"。
    Mood,
    /// `/snooze <title> [preset]` —— 把任务暂停到指定时刻；preset 缺省 `30m`。
    /// 与桌面右键菜单 Snooze 预设对偶（30m / 2h / tonight / tomorrow / monday）。
    /// token 在 parser 层只剥不解析（保 pure parse），解析交给 handler 在
    /// 有 now 时统一做。
    Snooze { title: String, token: String },
    /// `/unsnooze <title>` —— 解除暂停（清掉 `[snooze: ...]` marker）。与
    /// Snooze 分立命令避免"/snooze title 0" 这种歧义参数。
    Unsnooze { title: String },
    /// `/pin <title>` —— 给任务加 `[pinned]` marker，标"关键"。与桌面右键菜单
    /// 「📌 钉住」对偶；幂等（已 pinned 时再调 strip-before-write 不会让
    /// description 累积冗余 marker）。
    Pin { title: String },
    /// `/unpin <title>` —— 清掉 `[pinned]` marker。与 Pin 分立避免歧义。
    Unpin { title: String },
    /// `/pinned` —— 列出本 chat 派单中所有当前 pinned 任务（与桌面任务面板
    /// 「📌 N」chip 同源信号）。无参；多余尾部一律忽略。filter 范围与 `/tasks`
    /// 一致（origin == Tg(chat_id)），让两个查询命令的"范围语义"对齐。
    Pinned,
    /// `/silent <title>` —— 给任务加 `[silent]` marker，让 LLM 不在 proactive
    /// cycle 主动 pick 此任务（owner 仍可手动触发）。与桌面右键菜单
    /// 「🔇 标 silent」对偶；幂等（已 silent 时再调 strip-before-write 不会
    /// 让 description 累积冗余 marker）。
    Silent { title: String },
    /// `/unsilent <title>` —— 清掉 `[silent]` marker。与 Silent 分立避免歧义。
    Unsilent { title: String },
    /// `/silenced` —— 列出本 chat 派单中所有当前 silent 任务（与 /pinned 对
    /// 偶，给 owner audit "我标过哪些 silent" 用）。无参；多余尾部一律忽略。
    /// filter 范围与 /tasks 一致（origin == Tg(chat_id)）。
    Silenced,
    /// `/markers` —— 一次列本 chat 派单中所有 owner-intent markers（pinned +
    /// silent 联合）。与 /pinned + /silenced 两条命令对偶 —— 让 owner 用一
    /// 条命令 audit 自己标过的所有 marker 状态。无参；多余尾部一律忽略。
    Markers,
    /// `/whoami` —— 宠物自我介绍。无参；与桌面 chat `/whoami` 同信号源
    /// （陪伴天数 + 当前心情 + 自我画像首段 + 近常用工具 top 3），让 TG
    /// 端也能让宠物自报家门。
    Whoami,
    /// `/today` —— 今日叙事视图。无参；列出今日到期 (pending+due 是今天)
    /// 与今日已完成 (done+updated_at 在今天) 的任务标题清单。与 `/stats`
    /// 数字汇总互补 —— /today 看具体清单。
    Today,
    /// `/recent [N]` —— 列出本 chat 派单中最近 N 条 done 任务标题（按
    /// updated_at 倒序）。N 缺省 5，clamp 到 1..=20。owner 在 TG 上想"我最
    /// 近完成了什么"扫读 — 比 /today 更宽（不限今日 ）；比 /tasks 更聚焦
    /// （只 done 段）。
    Recent { n: u32 },
    /// `/find <keyword>` —— 在本 chat 派单中搜 keyword（命中标题 / 描述子
    /// 串，case-insensitive），返回最多 10 条命中行（status emoji + 标题 +
    /// 命中点 hint）。空 keyword 由 handler 走 missing-argument。
    Find { keyword: String },
    /// `/blocked` —— 列出本 chat 派单中被 `[blockedBy: ...]` 锁住的 active
    /// task（pending / error 状态）+ 每条仍未解决的 blocker 标题列表。无参；
    /// 多余尾部忽略（与 /tasks / /today 同容忍策略）。给 owner audit "我哪
    /// 些任务卡住了 / 卡在等什么" 用。
    Blocked,
    /// `/snoozed` —— 列出本 chat 派单中当前在 `[snooze: …]` 中的 task + 显
    /// 还多久醒。与 /silenced / /pinned 对偶。无参；多余尾部忽略。owner 想
    /// audit "我哪些任务被暂存了 / 还多久回到队列" 用。
    Snoozed,
    /// `/mute [N]` —— 临时静音 proactive 主动开口 N 分钟（缺省 30；0 = 解
    /// 除）。复用 `proactive::set_mute_minutes` 同后端 — 与桌面 PanelDebug
    /// "⚙️ mute" 按钮等价。让 owner 在 TG 上"嘿宠物先安静半小时"一句话搞定。
    /// clamp 0..=10080（≤ 7 天）。
    Mute { minutes: i64 },
    /// `/note <text>` —— 把任意文本作 general memory item 存（owner 在外
    /// 面随手"记一笔"）。title 自动生成 `note-YYYY-MM-DDTHH-MM-SS`（秒级
    /// 唯一）；description = trim 后的 text。空 text → missing-arg friendly
    /// hint。与 桌面 PanelMemory "新建 general item" 同后端，状态一致。
    Note { text: String },
    /// `/digest [N]` —— 最近 N 条 done task 标题 + [result:] 摘要一行式
    /// dump。与 /recent 只显标题互补 — owner 想"扫读最近做了啥 + 产物"
    /// 时用 /digest，纯标题用 /recent。N 缺省 5，clamp 1..=20。
    Digest { n: u32 },
    /// `/show <title>` —— 显示指定任务的 raw_description（含全部 markers）
    /// + detail.md 内容预览（前 300 字符），让 owner 在 TG 端 audit 单条
    /// 任务详情不必回桌面。空 title 走 missing-arg；title resolve 三层
    /// （数字 index → fuzzy → 错误候选）与 /done /cancel /edit 同源。
    Show { title: String },
    /// `/due <preset>` —— 列出 pending 任务在指定时间段的 due 清单。preset
    /// 缺省 `tomorrow`（最常用的"明天什么"前向 audit）。支持中英 alias
    /// （tomorrow / 明天 / 本周 / 下周 等）。与 `/today` 互补 —— /today 只
    /// 看今日，/due 看更远视角。preset 无效时 handler 走 usage hint 附识
    /// 别失败的字面字符串。
    Due {
        preset: Option<DuePreset>,
        raw_arg: String,
    },
    /// `/reflect <text>` —— 把任意文本作 **ai_insights** memory item 存
    /// （owner 在外面随手记反思 / observation）。与 `/note`（存 general）
    /// 对偶：那个是"杂项 brain-dump"，这个是"反思 / 自我洞察"——分类语义
    /// 不同的两个入口避免 ai_insights 段被日常杂项稀释。title 自动生成
    /// `reflect-YYYY-MM-DDTHH-MM-SS`（秒级唯一）；description = trim 后
    /// 的 text。空 text → missing-arg friendly hint。
    Reflect { text: String },
    /// `/edit <title> :: <new desc>` —— 覆写指定 butler_task 的 description
    /// 整段。`::` 是必填 separator —— 让 title 含空格 / 全角符号 / 中文标点
    /// 仍能精确切（与单空白切相比歧义最少；owner 外面想加 marker / 改 body
    /// 时单条命令搞定）。任一端 trim 后为空 → handler 走 missing-arg
    /// hint。**全量覆写**语义：新 desc 完全替换旧描述，既有 `[task pri=...]`
    /// `[every: ...]` 等 markers owner 自己负责保留 / 重写（与桌面 ✏️ 改
    /// schedule modal 不同 — 那个只改 prefix；本命令是 textarea 等价）。
    Edit { title: String, new_desc: String },
    /// `/reset` —— 清掉 LLM 对话上下文（保留 system / 人设）。单击生效，无
    /// armed 二次确认（与桌面 `/clear` 的 5s armed 模式分开 —— 不同设备 /
    /// 多用户文化下 armed 窗口不适用）。
    Reset,
    /// `/version` —— 查看 pet app 版本 + SQLite schema 版本。无参；与桌面
    /// `/version` slash / Settings chip 同源。bug report 写"什么版本"用。
    Version,
    /// `/help` —— 显示帮助。无 topic 时列全表（每命令一行 + 一行描述）；
    /// 有 topic（如 `/help cancel`）时只显该命令的详细用法 + 示例 + 注意
    /// 事项。topic 可以带 `/` 前缀（`/help /cancel`）或不带（`/help cancel`），
    /// 大小写不敏感。
    Help {
        topic: Option<String>,
    },
    Unknown { name: String },
}

impl TgCommand {
    /// 命令名（不带 `/`，已转小写），用于通用文案模板。
    pub fn name(&self) -> &str {
        match self {
            TgCommand::Cancel { .. } => "cancel",
            TgCommand::Retry { .. } => "retry",
            TgCommand::Done { .. } => "done",
            TgCommand::Task { .. } => "task",
            TgCommand::Tasks => "tasks",
            TgCommand::Stats => "stats",
            TgCommand::Mood => "mood",
            TgCommand::Whoami => "whoami",
            TgCommand::Snooze { .. } => "snooze",
            TgCommand::Unsnooze { .. } => "unsnooze",
            TgCommand::Pin { .. } => "pin",
            TgCommand::Unpin { .. } => "unpin",
            TgCommand::Pinned => "pinned",
            TgCommand::Silent { .. } => "silent",
            TgCommand::Unsilent { .. } => "unsilent",
            TgCommand::Silenced => "silenced",
            TgCommand::Markers => "markers",
            TgCommand::Today => "today",
            TgCommand::Recent { .. } => "recent",
            TgCommand::Find { .. } => "find",
            TgCommand::Blocked => "blocked",
            TgCommand::Snoozed => "snoozed",
            TgCommand::Mute { .. } => "mute",
            TgCommand::Note { .. } => "note",
            TgCommand::Digest { .. } => "digest",
            TgCommand::Edit { .. } => "edit",
            TgCommand::Reflect { .. } => "reflect",
            TgCommand::Due { .. } => "due",
            TgCommand::Show { .. } => "show",
            TgCommand::Reset => "reset",
            TgCommand::Version => "version",
            TgCommand::Help { .. } => "help",
            TgCommand::Unknown { name } => name,
        }
    }

    /// 命令参数（标题）。无参命令（Tasks / Stats / Mood / Today / Reset / Version / Help / Unknown）返回 ""。
    #[allow(dead_code)] // public API for future TG command handlers; covered by tests
    pub fn title(&self) -> &str {
        match self {
            TgCommand::Cancel { title }
            | TgCommand::Retry { title }
            | TgCommand::Done { title }
            | TgCommand::Snooze { title, .. }
            | TgCommand::Unsnooze { title }
            | TgCommand::Pin { title }
            | TgCommand::Unpin { title }
            | TgCommand::Silent { title }
            | TgCommand::Unsilent { title }
            | TgCommand::Find { keyword: title }
            | TgCommand::Note { text: title }
            | TgCommand::Reflect { text: title }
            | TgCommand::Show { title } => title.as_str(),
            TgCommand::Edit { title, .. } => title.as_str(),
            TgCommand::Task { title, .. } => title.as_str(),
            TgCommand::Tasks
            | TgCommand::Pinned
            | TgCommand::Silenced
            | TgCommand::Markers
            | TgCommand::Stats
            | TgCommand::Mood
            | TgCommand::Whoami
            | TgCommand::Today
            | TgCommand::Recent { .. }
            | TgCommand::Blocked
            | TgCommand::Snoozed
            | TgCommand::Mute { .. }
            | TgCommand::Digest { .. }
            | TgCommand::Due { .. }
            | TgCommand::Reset
            | TgCommand::Version
            | TgCommand::Help { .. }
            | TgCommand::Unknown { .. } => "",
        }
    }
}

/// pure：解析 settings.telegram.allowed_username 的 `,` 分隔多用户列表。
///
/// 规则：
/// - `,` 分隔；每段 trim + 剥首位 `@` + lowercase
/// - 空段 / 全空白跳过（容错连续逗号 / 末尾逗号）
/// - 同名去重保留首个
///
/// 空输入 → 空 Vec，与 handle_message 的"空白名单 = 任何人都允许"语义
/// 一致（与之前 String 为空的行为对齐，向后兼容）。
pub fn parse_allowed_usernames(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for part in raw.split(',') {
        let trimmed = part.trim().trim_start_matches('@').to_lowercase();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.clone()) {
            out.push(trimmed);
        }
    }
    out
}

/// pure：bot 启动时推给 Telegram `setMyCommands` 的命令清单。
/// 返回 `(name, description)` 元组按"用户输 `/` 时看到的顺序"排列：
/// `task` 在前 —— 创建是高频操作；`help` 在末 —— 兜底。
///
/// 描述长度受 Telegram 限制（≤ 256 字符）；这里全部都很短，留白足够。
/// `name` 必须全 lowercase ASCII（TG 命令名约束）。
///
/// 与 `format_help_text` 的关系：本函数是给 TG **客户端补全 UI** 用的结
/// 构化数据；`format_help_text` 是给用户主动 `/help` 看的多行文本。两者
/// 都要"覆盖全部命令"但不互相耦合 —— 出现 drift 时各自的测试会提醒。
pub fn tg_command_registry() -> Vec<(&'static str, &'static str)> {
    tg_command_registry_localized("zh")
}

/// 同 `tg_command_registry` 但按 `lang` 切换 description 语种。
/// `lang` 大小写不敏感；不识别的值兜底为 `"zh"` —— 让陌生配置不让 bot
/// 起不来 (defensive default)。
pub fn tg_command_registry_localized(lang: &str) -> Vec<(&'static str, &'static str)> {
    match lang.to_ascii_lowercase().as_str() {
        "en" => vec![
            ("task", "Queue a task (!! P5 / !!! P7)"),
            ("tasks", "List tasks dispatched in this chat"),
            ("stats", "Status counts: pending / overdue / done-today / etc."),
            ("done", "Mark a task as done"),
            ("cancel", "Cancel a task"),
            ("retry", "Retry a failed task"),
            ("snooze", "Snooze a task (30m / 2h / tonight / tomorrow / monday)"),
            ("unsnooze", "Clear a task's snooze"),
            ("pin", "Mark a task as pinned (key task)"),
            ("unpin", "Clear a task's pinned mark"),
            ("pinned", "List currently pinned tasks dispatched from this chat"),
            ("silent", "Mark a task as [silent] (LLM won't auto-pick; manual fire still works)"),
            ("unsilent", "Clear a task's [silent] mark"),
            ("silenced", "List currently silent tasks dispatched from this chat"),
            ("markers", "List all owner-intent markers in one shot (pinned + silent)"),
            ("mood", "Show the pet's current mood"),
            ("whoami", "Show pet's whoami digest (companionship / mood / persona / top tools)"),
            ("today", "Today's due / done task titles"),
            ("due", "List pending tasks due in a window (preset: tomorrow / thisweek / nextweek; default tomorrow)"),
            ("recent", "List recent N done tasks (default 5, cap 20)"),
            ("find", "Search this chat's tasks by keyword (title / description substring)"),
            ("show", "Show full raw description (with markers) + detail.md preview of a task"),
            ("blocked", "List active tasks blocked by [blockedBy: …] with their unresolved blockers"),
            ("snoozed", "List tasks currently in [snooze: …] with time until wake"),
            ("mute", "Mute proactive for N minutes (default 30; 0 to clear)"),
            ("note", "Save arbitrary text as a general memory item (quick brain-dump)"),
            ("reflect", "Save arbitrary text as an ai_insights memory item (reflection / self-observation)"),
            ("digest", "Recent N done tasks with [result:] summary one-liner (default 5, cap 20)"),
            ("edit", "Overwrite a butler task's description: /edit <title> :: <new desc>"),
            ("reset", "Clear LLM chat context (keep persona)"),
            ("version", "Show pet app version + SQLite schema version"),
            ("help", "Show command help"),
        ],
        _ => vec![
            ("task", "把单条任务塞进队列（!! P5 / !!! P7）"),
            ("tasks", "列出本会话派出的任务清单"),
            ("stats", "状态计数：待办 / 逾期 / 今日完成 等"),
            ("done", "把指定任务标 done"),
            ("cancel", "取消指定任务"),
            ("retry", "把失败任务重置回 pending"),
            ("snooze", "暂停任务（30m / 2h / tonight / tomorrow / monday，缺省 30m）"),
            ("unsnooze", "解除任务暂停"),
            ("pin", "钉住任务（标 [pinned]）"),
            ("unpin", "取消任务钉住（剥 [pinned]）"),
            ("pinned", "列出本聊天派单中所有钉住任务（与桌面「📌 N」chip 同源）"),
            ("silent", "标静默（LLM 不主动选；面板 / 手动触发不受影响）"),
            ("unsilent", "解除静默（剥 [silent] marker）"),
            ("silenced", "列出本聊天派单中所有 silent 任务（与「🔇 N silent」面板同源）"),
            ("markers", "一次列出所有 owner-intent markers（pinned + silent）"),
            ("mood", "查看宠物当前心情"),
            ("whoami", "宠物自我介绍（陪伴 / 心情 / 自我画像 / 近常用工具）"),
            ("today", "今日到期 / 已完成的任务标题清单"),
            ("due", "列指定时段 due 的 pending 任务（preset: tomorrow / thisweek / nextweek，缺省 tomorrow）"),
            ("recent", "最近 N 条已完成任务标题（默认 5，上限 20）"),
            ("find", "按 keyword 搜本聊天派单（命中标题或描述子串，至多 10 条）"),
            ("show", "显单条任务完整 raw description（含 markers）+ detail.md 预览"),
            ("blocked", "列出被 [blockedBy: …] 锁住的活跃 task + 仍未解决的 blocker 标题"),
            ("snoozed", "列出当前在 [snooze: …] 中的 task + 还多久醒"),
            ("mute", "临时静音 proactive N 分钟（默认 30；0 = 解除）"),
            ("note", "把任意文本作 general memory item 存（owner 随手记一笔）"),
            ("reflect", "把任意文本作 ai_insights memory item 存（反思 / 自我洞察）"),
            ("digest", "最近 N 条 done task 标题 + result 一行式（默认 5，上限 20）"),
            ("edit", "覆写 butler task 描述：/edit <title> :: <new desc>"),
            ("reset", "清掉 LLM 对话上下文（保留人设）"),
            ("version", "查看 pet 版本 + schema 版本"),
            ("help", "显示完整命令帮助"),
        ],
    }
}

/// pure：把 hardcoded 命令矩阵与用户自定义命令合并，过滤掉非法 / 冲突项，
/// 返回最终注册给 Telegram `setMyCommands` 的 `(name, description)` 序列。
///
/// 自定义条目过滤规则（无效条目静默丢弃，不让一条配错就 bot 起不来）：
/// - `name` 非空 + 仅含 lowercase ASCII / 数字 / `_`（TG API 约束）
/// - `description` trim 后非空 + 字符数 ≤ 256
/// - 不与 hardcoded 名重名（避免覆盖既有命令语义）
/// - custom 列表内同名重复 → 保留首个
///
/// 顺序：先 hardcoded（按 tg_command_registry 内部顺序），后 custom（按用
/// 户列表内出现顺序）。让"高频系统命令在前"在 TG 补全弹窗里保持稳定。
pub fn merged_command_registry(
    custom: &[crate::commands::settings::TgCustomCommand],
    lang: &str,
) -> Vec<(String, String)> {
    let hardcoded = tg_command_registry_localized(lang);
    let hardcoded_names: std::collections::HashSet<&str> =
        hardcoded.iter().map(|(n, _)| *n).collect();
    let mut seen_custom: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<(String, String)> = hardcoded
        .into_iter()
        .map(|(n, d)| (n.to_string(), d.to_string()))
        .collect();
    for c in custom {
        let name = c.name.trim();
        if name.is_empty() || name.len() > 32 {
            continue;
        }
        if !name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        {
            continue;
        }
        if hardcoded_names.contains(name) {
            continue;
        }
        if !seen_custom.insert(name.to_string()) {
            continue;
        }
        let desc = c.description.trim();
        if desc.is_empty() || desc.chars().count() > 256 {
            continue;
        }
        out.push((name.to_string(), desc.to_string()));
    }
    out
}

/// `/task` 优先级前缀的默认 / 紧迫 / 最紧迫三档。与 LLM 工具描述里 "日常
/// 1-3 / 紧迫 5-7 / 最高 8-9" 的档次表对齐；8/9 留给极端语境，不通过 TG
/// 命令直接拉到顶。
pub const TASK_PRI_DEFAULT: u8 = 3;
pub const TASK_PRI_URGENT: u8 = 5;
pub const TASK_PRI_VERY_URGENT: u8 = 7;

/// pure：从 `/task` 命令的尾部参数（已 trim）里识别可选的 `!!` / `!!!`
/// 优先级前缀，返回 `(priority, real_title)`。
///
/// 规则：
/// - 首个 whitespace token 是 `!!!` → (7, 余下 trim)
/// - 首个 token 是 `!!` → (5, 余下 trim)
/// - 否则 → (3, rest 原样 trim)
///
/// 设计取舍：
/// - **只识别恰好 2 / 3 个 `!`**：4 个或更多 ！ 整体回退到默认 P3 + 把
///   它当 title 的一部分（用户写 "!!!! foo" 大概率是表达兴奋而非档次）。
/// - **空 rest** → (3, "")，让上层 handler 走 missing-argument。
/// - **只 prefix 没 title**（如 "!!"）→ (5, "")，同样让 handler 报缺参，
///   这样错的不是"档次不对"而是"没写要做啥"，文案更精确。
pub fn parse_task_prefix(rest: &str) -> (u8, String) {
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return (TASK_PRI_DEFAULT, String::new());
    }
    let (head, tail) = match trimmed.split_once(char::is_whitespace) {
        Some((h, t)) => (h, t.trim().to_string()),
        None => (trimmed, String::new()),
    };
    match head {
        "!!!" => (TASK_PRI_VERY_URGENT, tail),
        "!!" => (TASK_PRI_URGENT, tail),
        _ => (TASK_PRI_DEFAULT, trimmed.to_string()),
    }
}

/// 解析 TG 文本是否为命令。
///
/// 规则：
/// - 必须以 `/` 开头；否则返回 `None`（让 chat pipeline 接管）
/// - 取首个空白前的 token 作命令名，去掉 `/`、转小写
/// - 剩余部分 trim 后作参数（标题），允许空（"/cancel" 单独）
/// - cancel / retry 命中 → 对应 variant
/// - 其它非空命令名 → `Unknown { name }`
/// pure：把 `/snooze <title> [preset]` 的参数串拆成 `(title, token)`。
/// 取最后一个 whitespace-分隔 token；命中 `parse_snooze_token` 时剥下作
/// preset，其余拼回 title；不命中 → 全 arg 当 title，token 空。
fn split_trailing_snooze_token(arg: &str) -> (String, String) {
    let arg = arg.trim();
    if arg.is_empty() {
        return (String::new(), String::new());
    }
    let words: Vec<&str> = arg.split_whitespace().collect();
    if words.len() < 2 {
        // 只有一个 token：可能是单 title（"shopping"）也可能是 preset-only
        // （"30m"）。两者都按 title 处理，让 handler 走 missing-argument 而
        // 非把 preset 误当 title。preset-only 没 title 本身就该报错。
        return (arg.to_string(), String::new());
    }
    let last = words[words.len() - 1];
    if parse_snooze_token(last).is_some() {
        let title = words[..words.len() - 1].join(" ");
        (title, last.to_string())
    } else {
        (arg.to_string(), String::new())
    }
}

/// Snooze preset 的语义键。Pure helper 把 user-typed 字符串映射到 enum，
/// handler 拿到 enum 后 + now 算绝对时刻。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnoozeSpec {
    /// `<N>m` ——  N 分钟（1..=10080，即 ≤ 7 天）
    Minutes(u32),
    /// `<N>h` ——  N 小时（1..=168，即 ≤ 7 天）
    Hours(u32),
    /// `tonight` —— 今晚 18:00（已过则跳明晚 18:00，与桌面右键 chip 同语义）
    Tonight,
    /// `tomorrow` —— 明天 09:00
    Tomorrow,
    /// `monday` —— 下个周一 09:00（今日是周一也跳下周一）
    Monday,
}

/// 把 `/snooze` 的 preset token 解析为 SnoozeSpec。大小写不敏感。
/// 支持 EN 预设 (tonight / tomorrow / monday) + CJK 预设 (今晚 / 明早 /
/// 明天 / 下周一 / 周一) + Nm / Nh / 分 / 小时 后缀格式。
/// 空串 / 不识别 / 数字越界 → None。
pub fn parse_snooze_token(token: &str) -> Option<SnoozeSpec> {
    let raw = token.trim();
    if raw.is_empty() {
        return None;
    }
    let t = raw.to_lowercase();
    // EN 预设：原 ASCII 短串
    match t.as_str() {
        "tonight" => return Some(SnoozeSpec::Tonight),
        "tomorrow" => return Some(SnoozeSpec::Tomorrow),
        "monday" => return Some(SnoozeSpec::Monday),
        _ => {}
    }
    // CJK 预设：直接 raw 比对（lowercase 对中文无影响但保持一致风格）。
    // 明早 / 明天 / 明日 都映射 Tomorrow（09:00），与既有 EN tomorrow 同语义。
    // 周一 / 下周一 / 下周1 都映射 Monday，"下周" 显式 = 下一个 Monday。
    match raw {
        "今晚" => return Some(SnoozeSpec::Tonight),
        "明早" | "明天" | "明日" => return Some(SnoozeSpec::Tomorrow),
        "周一" | "下周一" | "下周1" => return Some(SnoozeSpec::Monday),
        _ => {}
    }
    // CJK 数字后缀：30 分 / 2 小时（带 / 不带空格）。空白归一后比对 suffix。
    let raw_compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    if let Some(num_str) = raw_compact.strip_suffix('分') {
        let n: u32 = num_str.parse().ok()?;
        if n == 0 || n > 7 * 24 * 60 {
            return None;
        }
        return Some(SnoozeSpec::Minutes(n));
    }
    if let Some(num_str) = raw_compact.strip_suffix("小时") {
        let n: u32 = num_str.parse().ok()?;
        if n == 0 || n > 7 * 24 {
            return None;
        }
        return Some(SnoozeSpec::Hours(n));
    }
    // EN Nm / Nh：与既有路径同
    if let Some(num_str) = t.strip_suffix('m') {
        let n: u32 = num_str.parse().ok()?;
        if n == 0 || n > 7 * 24 * 60 {
            return None;
        }
        return Some(SnoozeSpec::Minutes(n));
    }
    if let Some(num_str) = t.strip_suffix('h') {
        let n: u32 = num_str.parse().ok()?;
        if n == 0 || n > 7 * 24 {
            return None;
        }
        return Some(SnoozeSpec::Hours(n));
    }
    None
}

/// 把 SnoozeSpec + now 算绝对 NaiveDateTime。Tonight / Tomorrow / Monday
/// 的边界规则与桌面右键 Snooze chip 完全一致：
/// - Tonight: today 18:00；已过 → tomorrow 18:00
/// - Tomorrow: tomorrow 09:00
/// - Monday: 下个周一 09:00；今日是周一也跳下周一（"下周一" 语义 = 下周第一天）
pub fn compute_snooze_until(
    spec: SnoozeSpec,
    now: chrono::NaiveDateTime,
) -> chrono::NaiveDateTime {
    use chrono::{Datelike, Duration};
    match spec {
        SnoozeSpec::Minutes(n) => now + Duration::minutes(n as i64),
        SnoozeSpec::Hours(n) => now + Duration::hours(n as i64),
        SnoozeSpec::Tonight => {
            let today_6pm = now
                .date()
                .and_hms_opt(18, 0, 0)
                .expect("18:00 always valid");
            if today_6pm <= now {
                today_6pm + Duration::days(1)
            } else {
                today_6pm
            }
        }
        SnoozeSpec::Tomorrow => (now.date() + Duration::days(1))
            .and_hms_opt(9, 0, 0)
            .expect("09:00 always valid"),
        SnoozeSpec::Monday => {
            let weekday = now.weekday().num_days_from_monday();
            let days_ahead = if weekday == 0 { 7 } else { 7 - weekday };
            (now.date() + Duration::days(days_ahead as i64))
                .and_hms_opt(9, 0, 0)
                .expect("09:00 always valid")
        }
    }
}

pub fn parse_tg_command(text: &str) -> Option<TgCommand> {
    let trimmed = text.trim_start();
    let after_slash = trimmed.strip_prefix('/')?;
    if after_slash.is_empty() {
        return None;
    }
    let (raw_name, rest) = match after_slash.split_once(char::is_whitespace) {
        Some((n, r)) => (n, r),
        None => (after_slash, ""),
    };
    let name = raw_name.to_lowercase();
    let title = rest.trim().to_string();
    match name.as_str() {
        "cancel" => Some(TgCommand::Cancel { title }),
        "retry" => Some(TgCommand::Retry { title }),
        "done" => Some(TgCommand::Done { title }),
        // `/show <title>`：所有参数 = title（与 /cancel /done 同模板）。空 title
        // 由 handler 走 missing-argument 反馈。
        "show" => Some(TgCommand::Show { title }),
        // `/task <title>`：单数，创建。空 title 由 handler 走 missing-argument。
        // 注意先于 `tasks` 判断不必要 — split 已按 token 边界切分，"task" 与
        // "tasks" 是两个独立 token。可选 `!!` / `!!!` 前缀映射 P5 / P7。
        "task" => {
            let (priority, real_title) = parse_task_prefix(&title);
            Some(TgCommand::Task {
                title: real_title,
                priority,
            })
        }
        // `/tasks` 是查询命令，没有参数；多余尾部一律忽略而非走 Unknown，
        // 让 `/tasks since:7d` 这种用户随手探的写法也能命中（暂不实现过滤
        // 语义，纯前向兼容预留）。
        "tasks" => Some(TgCommand::Tasks),
        // `/stats` 同 /tasks：无参；多余尾部忽略
        "stats" => Some(TgCommand::Stats),
        // `/mood` 同 /tasks：无参；多余尾部忽略（让 "/mood now?" 也能命中）
        "mood" => Some(TgCommand::Mood),
        // `/whoami` 同上：无参；多余尾部忽略（让 "/whoami please" 也能命中）
        "whoami" => Some(TgCommand::Whoami),
        // `/snooze <title> [preset]`：把任务暂停到某时刻。preset 是 arg 的最
        // 后一个 token；命中已知 preset 时剥下来作 token，其余拼回 title。
        // 不命中（最后 token 不是 preset / 全 arg 只有 title）→ token 空，
        // handler 默认走 30m。
        "snooze" => {
            let (title, token) = split_trailing_snooze_token(&title);
            Some(TgCommand::Snooze { title, token })
        }
        // `/unsnooze <title>`：解除暂停。所有参数 = title。
        "unsnooze" => Some(TgCommand::Unsnooze { title }),
        // `/pin <title>`：钉住任务（写 [pinned] marker）。空 title 由 handler
        // 走 missing-argument。无 preset 参数，所有内容当 title。
        "pin" => Some(TgCommand::Pin { title }),
        // `/unpin <title>`：取消钉住（剥 [pinned] marker）。与 pin 同样无参数。
        "unpin" => Some(TgCommand::Unpin { title }),
        // `/pinned`：列 pinned 任务清单。无参；多余尾部一律忽略（与 /tasks 同
        // 容忍策略），让 "/pinned now?" 这种用户随手探的写法也能命中。
        "pinned" => Some(TgCommand::Pinned),
        // `/silent <title>`：标 silent 让 LLM 不主动 pick；无 preset 参数，所有
        // 内容当 title（与 /pin 同模板）。
        "silent" => Some(TgCommand::Silent { title }),
        // `/unsilent <title>`：解除 silent。
        "unsilent" => Some(TgCommand::Unsilent { title }),
        // `/silenced`：列 silent 任务清单。无参；多余尾部一律忽略（与 /pinned
        // 同容忍策略）。
        "silenced" => Some(TgCommand::Silenced),
        // `/markers`：一次列 pinned + silent 联合。
        "markers" => Some(TgCommand::Markers),
        // `/today` 同上无参语义
        "today" => Some(TgCommand::Today),
        // `/due [preset]`：缺省 tomorrow（最常用前向 audit）；非空且无法识别
        // 时存 raw_arg 让 handler usage hint 时回显（preset 标 None 表示
        // "无效"）。preset 名单：tomorrow / thisweek / nextweek 含中英 alias。
        "due" => {
            let trimmed = title.trim();
            let (preset, raw) = if trimmed.is_empty() {
                (Some(DuePreset::Tomorrow), String::new())
            } else {
                let first_token = trimmed
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string();
                (parse_due_preset(&first_token), first_token)
            };
            Some(TgCommand::Due {
                preset,
                raw_arg: raw,
            })
        }
        // `/recent [N]`：N 缺省 5，clamp 1..=20。非数字尾部一律忽略走默认（与
        // /tasks since:7d 同前向兼容策略 —— 不让奇怪后缀走 Unknown）。
        "recent" => {
            let n = title
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .map(|n| n.clamp(1, 20))
                .unwrap_or(5);
            Some(TgCommand::Recent { n })
        }
        // `/find <keyword>`：所有 arg 作 keyword（含空格也保留 — 让 "/find
        // 整理 Downloads" 命中标题含"整理 Downloads"的 task）。空 keyword
        // 由 handler 走 missing-argument。
        "find" => Some(TgCommand::Find { keyword: title }),
        // `/blocked`：无参；多余尾部忽略（与 /tasks / /today 同容忍策略）。
        "blocked" => Some(TgCommand::Blocked),
        // `/snoozed`：无参；多余尾部忽略（与 /silenced / /pinned 同模板）。
        "snoozed" => Some(TgCommand::Snoozed),
        // `/mute [N]`：N 缺省 30 分钟；clamp 0..=10080（≤ 7 天）。非数字
        // 尾部一律忽略走默认（与 /recent / /tasks 同前向兼容策略）。N == 0
        // → 解除 mute（与桌面 PanelDebug "⚙️ mute" 二次点同语义）。
        "mute" => {
            let minutes = title
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<i64>().ok())
                .map(|n| n.clamp(0, 10080))
                .unwrap_or(30);
            Some(TgCommand::Mute { minutes })
        }
        // `/note <text>`：所有 arg 当 text（含空格保留）。空 text 由
        // handler 走 missing-arg 反馈。
        "note" => Some(TgCommand::Note { text: title }),
        // `/reflect <text>`：与 /note 同模板但写入 ai_insights category。
        // 空 text 由 handler 走 missing-arg。
        "reflect" => Some(TgCommand::Reflect { text: title }),
        // `/edit <title> :: <new desc>`：first-occurrence `::` 切分；任一端
        // trim 后为空 → handler 走 missing-arg。新 desc 是全量覆写（与
        // 桌面 detail.md textarea save 等价）。
        "edit" => {
            let (t, d) = match title.split_once("::") {
                Some((lhs, rhs)) => (lhs.trim().to_string(), rhs.trim().to_string()),
                None => (title, String::new()),
            };
            Some(TgCommand::Edit {
                title: t,
                new_desc: d,
            })
        }
        // `/digest [N]`：与 /recent 同 N 处理 — 缺省 5，clamp 1..=20，
        // 非数字尾部 fallback 默认。
        "digest" => {
            let n = title
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .map(|n| n.clamp(1, 20))
                .unwrap_or(5);
            Some(TgCommand::Digest { n })
        }
        // `/reset` 无参；多余尾部忽略
        "reset" => Some(TgCommand::Reset),
        // `/version` 无参；多余尾部忽略
        "version" => Some(TgCommand::Version),
        // `/help` 同 /tasks：无参，多余尾部忽略
        // `/help` 无参 = 显全表；`/help <cmd>` = 显该命令详细用法。topic
        // 可以带 `/` 前缀或不带，大小写不敏感 — 都在 format helper 内规整。
        "help" => Some(TgCommand::Help {
            topic: if title.is_empty() {
                None
            } else {
                Some(title)
            },
        }),
        _ => Some(TgCommand::Unknown { name }),
    }
}

/// 命令成功反馈文案。`kind` 是 cancel / retry；title 嵌进去给用户确认。
/// 末尾追加一行**反向命令**指引（cancel → /retry，retry → /cancel），让
/// 用户连续操作（误取消 → 立刻 retry / 重试失败 → 立刻 cancel）不必再回
/// /help 查语法。
pub fn format_command_success(kind: &str, title: &str) -> String {
    let title = title.trim();
    match kind {
        "cancel" => format!(
            "🚫 已取消「{}」\n如需恢复发 /retry {}",
            title, title
        ),
        "retry" => format!(
            "🔄 已重置「{}」回 pending，下一轮宠物会重新尝试\n如需取消发 /cancel {}",
            title, title
        ),
        "done" => format!(
            "✓ 已标 done「{}」\n想加 result 摘要请回桌面板「✓ 标 done」按钮（TG 暂只支持空 result 路径）",
            title
        ),
        _ => format!("✅ 「{}」 已处理", title),
    }
}

/// 命令失败反馈文案。err 是底层 task_cancel / task_retry 返回的字符串。
pub fn format_command_error(err: &str) -> String {
    format!("⚠️ 操作失败：{}", err)
}

/// `/task <title>` 成功反馈。强调"已入队 + 实际 P 档 + 怎么调细节"，让用户
/// 一眼知道这条命令做了什么、想精细化怎么走。/cancel 的 hint 用来自洽
/// "误派 → 一键撤"。
pub fn format_task_created_success(title: &str, priority: u8) -> String {
    let t = title.trim();
    format!(
        "✅ 已加到队列「{}」(P{})\n用 /tasks 查看，/cancel {} 撤回；想调截止时间请回桌面板",
        t, priority, t
    )
}

/// pure：byte-level Levenshtein 编辑距离。命令名都是 ascii lowercase，
/// 不需 unicode-aware。标准 DP 但只保留两行 → O(min(a,b)) 空间；命令名
/// 都 ≤ 8 字符性能不是事，主要保证代码清晰。
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1) // deletion
                .min(curr[j - 1] + 1) // insertion
                .min(prev[j - 1] + cost); // substitution
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// 距离阈值：≤ 2 的 typo 才提示。3+ 通常已是另一个意思而非笔误。
pub const SUGGEST_MAX_DISTANCE: usize = 2;

/// pure：从 valid 里找与 unknown 编辑距离 ≤ `SUGGEST_MAX_DISTANCE` 的
/// 最近命令；返回 Some 仅当唯一最近。距离相同时取 valid 数组首个（让
/// 调用方按"高频在前"自然控制歧义解析顺序）。
///
/// `unknown` 空 / `valid` 空 → None。
pub fn suggest_command<'a>(unknown: &str, valid: &[&'a str]) -> Option<&'a str> {
    if unknown.is_empty() || valid.is_empty() {
        return None;
    }
    let mut best: Option<(usize, &'a str)> = None;
    for &name in valid {
        let d = levenshtein(unknown, name);
        if d > SUGGEST_MAX_DISTANCE {
            continue;
        }
        match best {
            None => best = Some((d, name)),
            Some((bd, _)) if d < bd => best = Some((d, name)),
            // 距离相同保留首个（valid 顺序优先）
            _ => {}
        }
    }
    best.map(|(_, n)| n)
}

/// 未知命令的回复。指向 `/help` 让用户获得完整列表，避免在多处重复列举命令矩阵。
/// `suggestion` 非空时把建议放第一行 —— TG 客户端通知预览常只显首行，
/// "你是想发 /xx 吗？" 比 "未知命令" 更有用。
pub fn format_unknown_command(name: &str, suggestion: Option<&str>) -> String {
    match suggestion {
        Some(sug) => format!(
            "你是不是想发 /{} 吗？\n未知命令 /{}。输入 /help 查看可用命令。",
            sug, name,
        ),
        None => format!("未知命令 /{}。输入 /help 查看可用命令。", name),
    }
}

/// 命令缺参数的回复（如 /cancel 后面什么都没有）。
pub fn format_missing_argument(name: &str) -> String {
    format!("用法：/{} <任务标题>", name)
}

/// `/cancel` `/retry` 的 fuzzy title resolution 结果。优先级：Exact >
/// Single > Ambiguous > None。多命中保留全部候选（IO 层做 head-N 截断）。
#[derive(Debug, PartialEq, Eq)]
pub enum FuzzyMatch {
    Exact(String),
    Single(String),
    None,
    Ambiguous(Vec<String>),
}

const FUZZY_AMBIGUOUS_PREVIEW_MAX: usize = 5;

/// pure：在 titles 里找与 query 匹配的 task title。trim query 与每条 title。
/// 1. 先找 trim 后字面相等的 title → Exact
/// 2. 否则 case-insensitive substring：collect 全部命中 →
///    - 0 → None
///    - 1 → Single
///    - >1 → Ambiguous（保留全部候选，IO 层 format 时再截断到 N 条预览）
/// 空 query 一律 None（避免空字符串 substring 命中所有 title）。
pub fn find_task_fuzzy(query: &str, titles: &[String]) -> FuzzyMatch {
    let q = query.trim();
    if q.is_empty() {
        return FuzzyMatch::None;
    }
    if let Some(t) = titles.iter().find(|t| t.trim() == q) {
        return FuzzyMatch::Exact(t.clone());
    }
    let q_lower = q.to_lowercase();
    let matches: Vec<String> = titles
        .iter()
        .filter(|t| t.to_lowercase().contains(&q_lower))
        .cloned()
        .collect();
    match matches.len() {
        0 => FuzzyMatch::None,
        1 => FuzzyMatch::Single(matches.into_iter().next().unwrap()),
        _ => FuzzyMatch::Ambiguous(matches),
    }
}

/// pure：把 1-indexed 整数 query 解析为 titles 中对应位置的 title。让
/// `/cancel 1` `/retry 2` 等价于"上次 /tasks 输出第 N 条"，避免键入长 title。
/// query trim 后非纯数字 / 数字 0 / 越界 → None，让 caller fall back 到 fuzzy
/// resolve；非 None 时返回 owned String 直接给 cancel/retry inner 用。
pub fn resolve_index_to_title(query: &str, titles: &[String]) -> Option<String> {
    let n: usize = query.trim().parse().ok()?;
    if n == 0 {
        return None;
    }
    titles.get(n - 1).cloned()
}

/// pure：给 `/cancel` `/retry` 0 命中时的"你是不是想…"建议排名。基于 query
/// 与各 title 的字符重合度（HashSet 交集大小）排序，取 top N。0 重合的 title
/// 过滤掉避免给完全不相关的建议。
///
/// char-overlap vs Levenshtein：前者 cover 90% 实战 typo / 漏字 / 顺序错
/// case，且不会让 "整理" → "学习"（短串距离小）的反直觉建议出现。
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

/// pure：0 命中反馈文案。suggestions 非空时附"你是不是想…"列表；空时回简短
/// "找不到任务「query」"。bullet 文案要让用户能直接复制 / 修改其中一条 title
/// 重发命令。
pub fn format_no_match_with_suggestions(query: &str, suggestions: &[String]) -> String {
    let q = query.trim();
    if suggestions.is_empty() {
        return format!("找不到任务「{}」", q);
    }
    let bullets: Vec<String> = suggestions
        .iter()
        .map(|t| format!("• {}", t.trim()))
        .collect();
    format!(
        "找不到任务「{}」。你是不是想：\n{}",
        q,
        bullets.join("\n")
    )
}

/// pure：渲染"多个任务都包含 query"反馈。最多 `FUZZY_AMBIGUOUS_PREVIEW_MAX`
/// 条 bullet；超出截断 + "…等 N 条" 提示，避免长列表刷屏。
pub fn format_ambiguous_match(query: &str, candidates: &[String]) -> String {
    let total = candidates.len();
    let preview: Vec<String> = candidates
        .iter()
        .take(FUZZY_AMBIGUOUS_PREVIEW_MAX)
        .map(|t| format!("• {}", t.trim()))
        .collect();
    let mut out = format!("「{}」匹配多个任务：\n{}", query.trim(), preview.join("\n"));
    if total > FUZZY_AMBIGUOUS_PREVIEW_MAX {
        out.push_str(&format!(
            "\n…等 {} 条；请用更精确的标题再试。",
            total
        ));
    } else {
        out.push_str("\n请用更精确的标题再试。");
    }
    out
}

/// `/tasks` 去重文案：上次 `/tasks` 响应与本次完全相同（list 没变）时返回
/// 这条简短提示，避免在 TG 历史里堆两份相同的列表刷屏。
pub fn format_tasks_no_change() -> String {
    "📋 任务清单和上次一样，没有变化。".to_string()
}

/// `/help` 输出文案。每条命令一行 `/<name> [<arg>] — <说明>` + 总注脚。
/// pure：方便单测与未来调试器复用（如 panel 里复用同一份命令矩阵展示）。
/// `/help <cmd>` 单命令详细文案。pure：返回该命令的用法 + 示例 + 注意事项
/// 多行段。topic 可带或不带 `/` 前缀，大小写不敏感。命中 → 详细段；不命
/// 中 → 友好兜底（"未知命令 X — /help 看全表"）。custom 命令命中时显
/// "（自定义命令）"提示 + 描述（owner 自配，详细用法只他自己知道）。
pub fn format_help_for_topic(
    topic: &str,
    custom: &[crate::commands::settings::TgCustomCommand],
) -> String {
    let name = topic.trim().trim_start_matches('/').to_lowercase();
    if name.is_empty() {
        return format_help_text(custom);
    }
    let detail = match name.as_str() {
        "task" => "📝 /task <title>\n\n用法：把单条任务塞进队列。\n  · 默认优先级 P3\n  · 前缀 `!!` → P5（紧迫）\n  · 前缀 `!!!` → P7（最高）\n\n示例：\n  /task 整理 Downloads\n  /task !! 写周报\n  /task !!! 修复线上 bug\n\n创建后 chat 自动收到确认 + origin marker [origin:tg:<chat_id>]，桌面 watcher 完成时也回传通知。",
        "tasks" => "📋 /tasks\n\n用法：列出本会话派出的任务清单（按 compare_for_queue 排序 + 按状态分组）。无参；多余尾部忽略。\n\n示例：\n  /tasks\n\n相关：/stats（数字汇总）/ /today（今日切片）/ /recent（近完成）/ /find（关键词搜）。",
        "stats" => "📊 /stats\n\n用法：一行汇总当前 chat 派单的状态计数 — 待办 / 逾期 / 今日完成 / 出错 / 今日取消。无参。\n\n示例：\n  /stats\n\n与 /tasks 互补：/stats 看数字汇总，/tasks 看具体清单。",
        "done" => "✅ /done <title>\n\n用法：把 pending / error 任务标 done。已 done / cancelled 拒绝重复操作。\n\n示例：\n  /done 整理 Downloads\n\n注意：TG 端不支持 `[result: ...]` 摘要；想加 result 回桌面板单条 mark-done dialog。",
        "cancel" => "🚫 /cancel <title>\n\n用法：取消一条 pending / error 任务（终态）。\n\n示例：\n  /cancel 整理 Downloads\n  /cancel 1   （/tasks 输出第 1 条）\n\n相关：/retry 把 error 重置回 pending；二者可来回切。",
        "retry" => "🔄 /retry <title>\n\n用法：把 status==Error 的任务重置为 pending，剥所有 [error: ...] / [done] markers。\n\n示例：\n  /retry 跑步\n\n限制：仅 error 状态可 retry；pending / done / cancelled 拒。",
        "snooze" => "💤 /snooze <title> [preset]\n\n用法：暂停任务到指定时刻（preset 缺省 30m）。\n\nPreset：\n  · 30m / 2h / Nm / Nh（Nm ≤ 7 天）\n  · tonight（今晚 18:00）\n  · tomorrow（明早 09:00）\n  · monday（下周一 09:00）\n  · 今晚 / 明早 / 明天 / 周一 / 下周一 CJK 同义词\n\n示例：\n  /snooze 写周报\n  /snooze 跑步 tonight\n  /snooze 读论文 2h\n\n过点后 marker 自动失效，任务回到 proactive 选单。",
        "unsnooze" => "💤 /unsnooze <title>\n\n用法：清掉任务的 [snooze: ...] marker，立即回到 proactive 选单。\n\n示例：\n  /unsnooze 写周报",
        "pin" => "📌 /pin <title>\n\n用法：钉住关键任务（写 [pinned] marker）。pinned task 在桌面任务面板浮顶 + 「📌 N」chip 计数同源。\n\n示例：\n  /pin 季度规划\n\n相关：/pinned 列所有 pinned；/unpin 取消。",
        "unpin" => "📌 /unpin <title>\n\n用法：清掉任务的 [pinned] marker。\n\n示例：\n  /unpin 季度规划",
        "pinned" => "📌 /pinned\n\n用法：列出本聊天派单中所有 pinned 任务（按状态分组）。无参。\n\n示例：\n  /pinned\n\n相关：/markers 一次列 pinned + silent 联合。",
        "silent" => "🔇 /silent <title>\n\n用法：标静默 — LLM 不主动选此任务，但面板 / 手动触发仍可。\n\n示例：\n  /silent 周末家务\n\n相关：/silenced 列所有 silent；/unsilent 取消。owner 不想让 pet 主动 pick 某条时用。",
        "unsilent" => "🔇 /unsilent <title>\n\n用法：清掉 [silent] marker，任务回到 LLM auto-pick 池。\n\n示例：\n  /unsilent 周末家务",
        "silenced" => "🔇 /silenced\n\n用法：列出本聊天派单中所有 silent 任务（按状态分组）。无参。\n\n示例：\n  /silenced",
        "markers" => "🏷 /markers\n\n用法：一次列 pinned + silent 两段（与 /pinned + /silenced 组合等价）。无参。\n\n示例：\n  /markers\n\n给 owner audit 「我标过哪些 owner-intent」用。",
        "mood" => "🐾 /mood\n\n用法：查看宠物当前心情（与桌面 MoodWidget 同 mood state 文件）。无参。\n\n示例：\n  /mood",
        "whoami" => "🐾 /whoami\n\n用法：宠物自我介绍 — 陪伴天数 / 当前心情 / 自我画像首段 / 近常用工具 top 3。无参。\n\n示例：\n  /whoami",
        "today" => "📅 /today\n\n用法：今日叙事视图 — 今日到期 (pending + due 在今天) + 今日已完成 (done + updated_at 在今天) 两段标题清单。无参。\n\n示例：\n  /today\n\n相关：/recent（不限今日 done）；/blocked（被 [blockedBy:] 锁住的）；/due（更远视角 — tomorrow / thisweek / nextweek）。",
        "due" => "📅 /due [preset]\n\n用法：列指定时段 due 的 pending 任务（含 due 字段 + 落在指定窗口的）。preset 缺省 tomorrow。\n\nPreset：\n  · tomorrow / tmr / tm / 明天 / 明日\n  · thisweek / this-week / week / 本周 / 这周（含 today 在内的 ISO Mon..Sun）\n  · nextweek / next-week / 下周\n\n示例：\n  /due\n  /due tomorrow\n  /due thisweek\n  /due 下周\n\n相关：/today 只看今日；/blocked 看锁住的。",
        "recent" => "🕒 /recent [N]\n\n用法：最近 N 条 done 任务标题（按 updated_at 倒序）。N 缺省 5，clamp 1..=20。\n\n示例：\n  /recent\n  /recent 10\n\n相关：/digest（同范围但含 [result:] 摘要）；/today（只看今日 done）；/tasks（全部状态）。",
        "digest" => "📋 /digest [N]\n\n用法：最近 N 条 done 任务的标题 + [result:] 摘要一行式（按 updated_at 倒序）。N 缺省 5，clamp 1..=20。\n\n示例：\n  /digest\n  /digest 10\n\n相关：/recent 同范围但只显标题（无 result 摘要时更紧凑）；/today 只看今日 done。",
        "edit" => "✏️ /edit <title> :: <new desc>\n\n用法：全量覆写指定 butler_task 的 description。`::` 是必填 separator — title 含空格 / 中文标点也能精确切。\n\n示例：\n  /edit 整理 Downloads :: 整理 Downloads [task pri=5 due=2026-05-20] [pinned]\n  /edit 写周报 :: 完整新 body 一段\n\n注意：**全量覆写**语义 — 新 desc 完全替换旧描述。想保留 `[task pri=...]` `[every: ...]` `[pinned]` 等 markers 请自行写进新 desc（命令不会自动续 markers）。Title resolve 与 /done / /cancel 同三层（数字 index → fuzzy → 错误候选）。",
        "reflect" => "🪞 /reflect <text>\n\n用法：把任意文本作 ai_insights memory item 存盘（反思 / 自我洞察分类，与 /note 写 general 对偶）。title 自动 `reflect-YYYY-MM-DDTHH-MM-SS`。\n\n示例：\n  /reflect 今天回顾：我对中断接受度过高，应该早点说 no\n  /reflect 观察：长 task 拆细后完成率明显提升\n\n相关：/note 写 general（杂项 brain-dump）；二者按「信号类型」分流避免 ai_insights 段被日常杂项稀释。可在 PanelMemory → AI 洞察 段查看 / 整理。",
        "find" => "🔍 /find <keyword>\n\n用法：搜本聊天派单（命中标题 / raw_description 子串，case-insensitive），至多 10 条。pending / error 浮顶。\n\n示例：\n  /find Downloads\n  /find 整理 桌面\n  /find #健身\n\n相关：/tasks（看全表）；/blocked（被锁住的）；/show（看单条详情）。",
        "show" => "🔬 /show <title>\n\n用法：显单条任务完整 raw description（含 [task pri=...] / [every:] / [pinned] 等所有 markers）+ detail.md 内容预览（前 300 字符）。Title resolve 与 /done / /cancel 同三层（数字 index → fuzzy → 错误候选）。\n\n示例：\n  /show 整理 Downloads\n  /show 1  （/tasks 输出第 1 条）\n\n相关：/find 搜任务；/edit 改 description；/tasks 看清单。让 owner 在 TG 端 audit 任务详情不必回桌面。",
        "blocked" => "🔒 /blocked\n\n用法：列出本 chat 派单中被 [blockedBy: ...] 锁住的活跃 task（pending / error），每条下方缩进列出仍未解决的 blocker 标题。无参。\n\n示例：\n  /blocked\n\n相关：/snoozed（被 [snooze:] 暂停的）。",
        "snoozed" => "💤 /snoozed\n\n用法：列出当前在 [snooze: ...] 中的 task + 还多久醒（按醒时间升序）。无参。\n\n示例：\n  /snoozed\n\n相关：/snooze（暂停一条）；/unsnooze（解除）。",
        "reset" => "🔄 /reset\n\n用法：清掉 LLM 对话上下文（保留 system / 人设）。无 armed 二次确认（与桌面 `/clear` 不同 — 不同设备 / 多用户文化）。\n\n示例：\n  /reset",
        "version" => "🐾 /version\n\n用法：查看 pet app 版本 + SQLite schema 版本。无参。bug report 写「什么版本」用。\n\n示例：\n  /version",
        "help" => "❓ /help [cmd]\n\n用法：\n  · /help（无参）→ 显全表 + 一行描述\n  · /help <cmd> → 显该命令的详细用法 + 示例\n\n示例：\n  /help\n  /help cancel\n  /help /snooze   （`/` 前缀也接受）",
        _ => "",
    };
    if !detail.is_empty() {
        return detail.to_string();
    }
    // custom 命令命中 → 显 owner 配的 description；详细用法只 owner 自己知道
    for c in custom {
        if c.name.trim().to_lowercase() == name {
            return format!(
                "🛠 /{}（自定义命令）\n\n{}",
                c.name.trim(),
                c.description.trim()
            );
        }
    }
    format!(
        "❓ 未知命令「/{}」。\n发 /help 看完整命令表。",
        name
    )
}

pub fn format_help_text(custom: &[crate::commands::settings::TgCustomCommand]) -> String {
    // 精简版：把 `/task` + `/task !!` + `/task !!!` 合到一行；
    // `/cancel` `/retry` 用斜杠紧贴；保留 `/tasks` 单行；总注脚移到首行
    // 旁的副标题。原 8 行压到 4 行（不含 custom 段），更适合 TG 单屏。
    let mut lines: Vec<String> = vec![
        "🤖 可用命令（结果会自动回传，无需轮询 /tasks）：".to_string(),
        "/tasks  —  列出本会话派出的任务清单".to_string(),
        "/stats  —  状态计数：待办 / 逾期 / 今日完成 / 出错 / 今日取消".to_string(),
        "/task <title>  —  入队（默认 P3；前缀 !! P5、!!! P7）".to_string(),
        "/done <title> | /cancel <title> | /retry <title>  —  标 done / 取消 / 重试（详细原因 / result 回桌面）".to_string(),
        "/snooze <title> [preset] | /unsnooze <title>  —  暂停 / 解除暂停（preset = 30m / 2h / tonight / tomorrow / monday）".to_string(),
        "/pin <title> | /unpin <title>  —  钉住 / 取消钉住（与桌面「📌 N」chip 过滤同源）".to_string(),
        "/silent <title> | /unsilent <title>  —  标静默 / 解除静默（LLM 不主动选；面板仍可手动触发）".to_string(),
        "/silenced  —  列出本聊天派单中所有 silent 任务（按状态分组）".to_string(),
        "/markers  —  一次列出所有 owner-intent markers（pinned + silent 两段，与 /pinned + /silenced 组合等价）".to_string(),
        "/pinned  —  列出本聊天派单中所有钉住任务（按状态分组，含 done/error/cancelled）".to_string(),
        "/mood  —  查看宠物当前心情".to_string(),
        "/whoami  —  宠物自我介绍（陪伴 / 心情 / 自我画像 / 近常用工具）".to_string(),
        "/today  —  今日到期 / 已完成的任务标题清单".to_string(),
        "/due [preset]  —  列指定时段 due（tomorrow / thisweek / nextweek 含中英 alias，缺省 tomorrow）".to_string(),
        "/recent [N]  —  最近 N 条已完成任务标题（默认 5，上限 20）".to_string(),
        "/find <keyword>  —  搜本聊天派单（命中标题或描述子串，至多 10 条）".to_string(),
        "/show <title>  —  显单条任务完整 raw description（含 markers）+ detail.md 预览".to_string(),
        "/blocked  —  列出被 [blockedBy: …] 锁住的活跃 task + 仍未解决的 blocker".to_string(),
        "/snoozed  —  列出当前在 [snooze: …] 中的 task + 还多久醒".to_string(),
        "/mute [N]  —  临时静音 proactive N 分钟（默认 30；0 = 解除）".to_string(),
        "/note <text>  —  把任意文本作 general memory item 存（随手记一笔）".to_string(),
        "/reflect <text>  —  把任意文本作 ai_insights memory item 存（反思 / 自我洞察，与 /note 对偶但分类不同）".to_string(),
        "/digest [N]  —  最近 N 条 done task 标题 + result 一行式（默认 5，上限 20）".to_string(),
        "/edit <title> :: <new desc>  —  覆写 butler task 描述（全量替换，markers 需自己写进 new desc）".to_string(),
        "/reset  —  清掉 LLM 对话上下文（保留人设）".to_string(),
        "/version  —  查看 pet 版本 + schema 版本".to_string(),
        "/help  —  显示本帮助".to_string(),
    ];
    // custom 段：非空时插在硬编码段之后、注脚之前。规则：
    // - name / description trim 后空跳过（兜底）
    // - 不去重 / 不严格校验 — bot.rs 已基于 merged_command_registry 过滤
    //   过 customs；这里只是按列表呈现
    if !custom.is_empty() {
        lines.push("".to_string());
        lines.push("🛠 自定义命令：".to_string());
        for c in custom {
            let name = c.name.trim();
            let desc = c.description.trim();
            if name.is_empty() || desc.is_empty() {
                continue;
            }
            lines.push(format!("/{}  —  {}", name, desc));
        }
    }
    // 注脚已合并进首行副标题（"结果会自动回传"），不再单独拉一行；让 TG
    // 端 4 行就显完，对小屏幕也友好。
    lines.join("\n")
}

/// `/tasks` 输出文案。`views` 为已按 `compare_for_queue` 排好序、并按当前
/// chat_id 过滤好的列表（IO 在 bot.rs 那边）。pure：纯字符串拼装，全部
/// 边界条件都在单测里钉牢。
///
/// 输出按状态分四段（进行中 / 已完成 / 已失败 / 已取消），每段空 K=0
/// 时整段省略。每行 `<emoji> [P{pri}] <title> [— <suffix>]`：
/// - 进行中：`⏳`，附 `截至 M/D HH:MM`（无 due 时省略 suffix）。
/// - 已完成：`✅`，附 result（若有，超 40 char 截断）。
/// - 已失败：`⚠️`，附 error_message（同上）。
/// - 已取消：`🚫`，附 cancellation reason（同上）。
///
/// `priority == 0` 不渲染 P 前缀（默认值，省字数 / 减少噪音）。
///
/// `TG_TASKS_MSG_LIMIT` 防御：拼装结果若超 4096 byte（teloxide 单条上限），
/// 在末尾附"…(列表过长，剩余 N 条请回桌面查看)"提示。实战上 < 50 条任务
/// 不会触发，主要是兜底防 LLM 自己派单刷爆队列时不至于发不出来。
pub fn format_tasks_list(views: &[crate::task_queue::TaskView]) -> String {
    use crate::task_queue::TaskStatus;
    if views.is_empty() {
        return "📋 你的任务清单是空的，想派点啥？".to_string();
    }

    let mut pending: Vec<&crate::task_queue::TaskView> = Vec::new();
    let mut done: Vec<&crate::task_queue::TaskView> = Vec::new();
    let mut error: Vec<&crate::task_queue::TaskView> = Vec::new();
    let mut cancelled: Vec<&crate::task_queue::TaskView> = Vec::new();
    for v in views {
        match v.status {
            TaskStatus::Pending => pending.push(v),
            TaskStatus::Done => done.push(v),
            TaskStatus::Error => error.push(v),
            TaskStatus::Cancelled => cancelled.push(v),
        }
    }

    let mut out = String::new();
    out.push_str(&format!("📋 你的任务（共 {} 条）\n", views.len()));

    let mut sections: Vec<(&str, &str, &[&crate::task_queue::TaskView])> = Vec::new();
    sections.push(("进行中", "⏳", &pending));
    sections.push(("已完成", "✅", &done));
    sections.push(("已失败", "⚠️", &error));
    sections.push(("已取消", "🚫", &cancelled));

    for (label, emoji, items) in &sections {
        if items.is_empty() {
            continue;
        }
        out.push('\n');
        out.push_str(&format!("{}（{}）\n", label, items.len()));
        for v in items.iter() {
            out.push_str(&format_task_line(emoji, v));
            out.push('\n');
        }
    }

    let trimmed = out.trim_end_matches('\n').to_string();
    truncate_if_overflow(trimmed, views.len())
}

/// `/pinned` 命令回复文案。`views` 应已被 caller 过滤为"本 chat + pinned"子集。
/// 与 `format_tasks_list` 分立而非合并 —— header 文案不同（📌 vs 📋）、空集合
/// 引导也不同（教用户怎么 pin）。section 分组逻辑（Pending / Done / Error /
/// Cancelled）复用同一思路保 TG 视觉一致。
///
/// 空集合：友好提示"暂无钉住"+ 教用户怎么 pin（`/pin <title>` / 桌面右键）。
/// 非空集合：📌 header + 各状态 section。
pub fn format_pinned_tasks_list(views: &[crate::task_queue::TaskView]) -> String {
    use crate::task_queue::TaskStatus;
    if views.is_empty() {
        return "📌 暂无钉住任务（本聊天派单中）。\n用 /pin <标题> 钉住，或在桌面任务面板右键 → 「📌 钉住」。"
            .to_string();
    }

    let mut pending: Vec<&crate::task_queue::TaskView> = Vec::new();
    let mut done: Vec<&crate::task_queue::TaskView> = Vec::new();
    let mut error: Vec<&crate::task_queue::TaskView> = Vec::new();
    let mut cancelled: Vec<&crate::task_queue::TaskView> = Vec::new();
    for v in views {
        match v.status {
            TaskStatus::Pending => pending.push(v),
            TaskStatus::Done => done.push(v),
            TaskStatus::Error => error.push(v),
            TaskStatus::Cancelled => cancelled.push(v),
        }
    }

    let mut out = String::new();
    out.push_str(&format!("📌 当前钉住任务（共 {} 条）\n", views.len()));

    let sections: [(&str, &str, &[&crate::task_queue::TaskView]); 4] = [
        ("进行中", "⏳", &pending),
        ("已完成", "✅", &done),
        ("已失败", "⚠️", &error),
        ("已取消", "🚫", &cancelled),
    ];
    for (label, emoji, items) in &sections {
        if items.is_empty() {
            continue;
        }
        out.push('\n');
        out.push_str(&format!("{}（{}）\n", label, items.len()));
        for v in items.iter() {
            out.push_str(&format_task_line(emoji, v));
            out.push('\n');
        }
    }

    let trimmed = out.trim_end_matches('\n').to_string();
    truncate_if_overflow(trimmed, views.len())
}

/// `/silenced` 命令回复文案。`views` 应已被 caller 过滤为"本 chat + [silent]"
/// 子集。与 `format_pinned_tasks_list` 同模板 —— header 🔇 vs 📌，空集合教学
/// 引导不同，section 分组逻辑（Pending / Done / Error / Cancelled）复用。
pub fn format_silenced_tasks_list(views: &[crate::task_queue::TaskView]) -> String {
    use crate::task_queue::TaskStatus;
    if views.is_empty() {
        return "🔇 暂无静默任务（本聊天派单中）。\n用 /silent <标题> 标静默（LLM 不主动选；面板 / 手动触发仍可），或在桌面任务面板右键 → 「🔇 标 silent」。"
            .to_string();
    }

    let mut pending: Vec<&crate::task_queue::TaskView> = Vec::new();
    let mut done: Vec<&crate::task_queue::TaskView> = Vec::new();
    let mut error: Vec<&crate::task_queue::TaskView> = Vec::new();
    let mut cancelled: Vec<&crate::task_queue::TaskView> = Vec::new();
    for v in views {
        match v.status {
            TaskStatus::Pending => pending.push(v),
            TaskStatus::Done => done.push(v),
            TaskStatus::Error => error.push(v),
            TaskStatus::Cancelled => cancelled.push(v),
        }
    }

    let mut out = String::new();
    out.push_str(&format!("🔇 当前静默任务（共 {} 条 · LLM 不主动选）\n", views.len()));

    let sections: [(&str, &str, &[&crate::task_queue::TaskView]); 4] = [
        ("进行中", "⏳", &pending),
        ("已完成", "✅", &done),
        ("已失败", "⚠️", &error),
        ("已取消", "🚫", &cancelled),
    ];
    for (label, emoji, items) in &sections {
        if items.is_empty() {
            continue;
        }
        out.push('\n');
        out.push_str(&format!("{}（{}）\n", label, items.len()));
        for v in items.iter() {
            out.push_str(&format_task_line(emoji, v));
            out.push('\n');
        }
    }

    let trimmed = out.trim_end_matches('\n').to_string();
    truncate_if_overflow(trimmed, views.len())
}

/// `/markers` 命令回复文案。一次列 pinned + silent 两段 —— owner 想"一眼看
/// 我标过的 owner-intent markers" 时用，省 /pinned + /silenced 两条命令往返。
/// `views` 应已被 caller 过滤为"本 chat" 子集；本 helper 内部再按 pinned /
/// silent 分两组（同一 task 同时是 pinned + silent 时两段都列）。
///
/// 空集合：友好提示"暂无任何 owner-intent marker"+ 教学引导。
pub fn format_markers_list(views: &[crate::task_queue::TaskView]) -> String {
    let pinned: Vec<&crate::task_queue::TaskView> =
        views.iter().filter(|v| v.pinned).collect();
    let silent: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| crate::task_queue::parse_silent(&v.raw_description))
        .collect();

    if pinned.is_empty() && silent.is_empty() {
        return "暂无 owner-intent markers（本聊天派单中）。\n用 /pin <标题> 钉住关键任务，或 /silent <标题> 让 LLM 不主动选某条。"
            .to_string();
    }

    let mut out = String::new();
    out.push_str(&format!(
        "owner-intent markers · 📌 {} 钉 / 🔇 {} 静\n",
        pinned.len(),
        silent.len()
    ));
    if !pinned.is_empty() {
        out.push_str(&format!("\n📌 钉住（{}）\n", pinned.len()));
        for v in &pinned {
            let emoji = match v.status {
                crate::task_queue::TaskStatus::Pending => "⏳",
                crate::task_queue::TaskStatus::Done => "✅",
                crate::task_queue::TaskStatus::Error => "⚠️",
                crate::task_queue::TaskStatus::Cancelled => "🚫",
            };
            out.push_str(&format_task_line(emoji, v));
            out.push('\n');
        }
    }
    if !silent.is_empty() {
        out.push_str(&format!("\n🔇 静默（{}）\n", silent.len()));
        for v in &silent {
            let emoji = match v.status {
                crate::task_queue::TaskStatus::Pending => "⏳",
                crate::task_queue::TaskStatus::Done => "✅",
                crate::task_queue::TaskStatus::Error => "⚠️",
                crate::task_queue::TaskStatus::Cancelled => "🚫",
            };
            out.push_str(&format_task_line(emoji, v));
            out.push('\n');
        }
    }
    let trimmed = out.trim_end_matches('\n').to_string();
    // 双段都长时可能超 4KB；用现有 truncate_if_overflow 按 union 数兜底
    truncate_if_overflow(trimmed, pinned.len() + silent.len())
}

/// `/stats` 命令回复文案。pure：接收已过滤到本 chat 的 views + 当前时刻 +
/// 今天日期（caller 注入便于测试），返回 6 行汇总文本。
///
/// 计数语义：
/// - `待办`：状态==Pending 的全部
/// - `逾期`：状态==Pending 且 `due` 已过（caller 注入的 now 解析 "YYYY-MM-DDTHH:MM" 比较）
/// - `今日完成`：状态==Done 且 `updated_at` 以 `today` 开头
/// - `出错`：状态==Error 的全部（不限今日 —— error 是需要 follow-up 的"债"）
/// - `今日取消`：状态==Cancelled 且 `updated_at` 以 `today` 开头
///
/// 全 0 时 header 后追加 "（今日很安静 ✨）"，让用户在彻底空盘子时也有正反馈。
pub fn format_stats_reply(
    views: &[crate::task_queue::TaskView],
    now: chrono::NaiveDateTime,
    today: chrono::NaiveDate,
) -> String {
    use crate::task_queue::TaskStatus;
    let today_prefix = today.format("%Y-%m-%d").to_string();
    let mut pending = 0usize;
    let mut overdue = 0usize;
    let mut done_today = 0usize;
    let mut error = 0usize;
    let mut cancelled_today = 0usize;
    for v in views {
        match v.status {
            TaskStatus::Pending => {
                pending += 1;
                if let Some(due_str) = &v.due {
                    if let Ok(due_dt) =
                        chrono::NaiveDateTime::parse_from_str(due_str, "%Y-%m-%dT%H:%M")
                    {
                        if due_dt < now {
                            overdue += 1;
                        }
                    }
                }
            }
            TaskStatus::Done => {
                if v.updated_at.starts_with(&today_prefix) {
                    done_today += 1;
                }
            }
            TaskStatus::Error => error += 1,
            TaskStatus::Cancelled => {
                if v.updated_at.starts_with(&today_prefix) {
                    cancelled_today += 1;
                }
            }
        }
    }
    let all_zero =
        pending == 0 && overdue == 0 && done_today == 0 && error == 0 && cancelled_today == 0;
    let mut out = String::new();
    out.push_str("📊 任务状态");
    if all_zero {
        out.push_str("（今日很安静 ✨）");
    }
    out.push('\n');
    out.push_str(&format!("○ 待办：{}\n", pending));
    out.push_str(&format!("🔴 逾期：{}\n", overdue));
    out.push_str(&format!("✓ 今日完成：{}\n", done_today));
    out.push_str(&format!("⚠️ 出错：{}\n", error));
    out.push_str(&format!("🗑 今日取消：{}", cancelled_today));
    out
}

/// `/mood` 命令回复文案。pure：接收 `read_current_mood_parsed()` 的 Option<(text,
/// motion)>，返回给用户看的简短反馈。
///
/// 三态：
/// - `None`：宠物还没记心情 → 友好提示而非空字符串
/// - `Some(("", None))`：极端边界（写入空字符串）—— 视作"无文字"
/// - `Some((text, motion))`：text 是 LLM 写的自由心情描述；motion 是可选的
///   Live2D motion group 名（如 `happy_idle`）。motion 存在时多输一行让用户
///   看到"宠物在 idle 还是兴奋"。
pub fn format_mood_reply(parsed: Option<(String, Option<String>)>) -> String {
    match parsed {
        None => "🐾 宠物还没记心情；一会儿主动开口时会写一笔。".to_string(),
        Some((text, motion)) => {
            let text_line = if text.trim().is_empty() {
                "🐾 心情：（无文字）".to_string()
            } else {
                format!("🐾 心情：{}", text.trim())
            };
            match motion {
                Some(m) if !m.trim().is_empty() => format!("{}\n  动作组：{}", text_line, m.trim()),
                _ => text_line,
            }
        }
    }
}

/// `/whoami` 命令回复文案。pure：接收四个 IPC 源的派生输入，输出 multi-line
/// 自我介绍文本。每段独立可缺失（None / 空 / 空字符串）—— 某源失败 / 没数据
/// 时该行省略，不抛错也不输出"未知"。所有源都空 → 给一行温和兜底。
///
/// 与桌面 chat `case "whoami"` 的排版完全对齐（emoji + 顺序 + 90 字截断）。
///
/// 参数：
/// - `user_name`：settings.user_name，空 → 不渲染
/// - `companionship_days`：陪伴天数，None → 不渲染
/// - `mood`：`(text, motion)` —— 与 `read_current_mood_parsed` 同源；None /
///   空 text → 不渲染
/// - `persona_summary`：自我画像描述，空 → 不渲染。函数内做首段切分 + 90 字截断
/// - `top_tools`：`(name, count)` 列表，取前 3 渲染；空 → 不渲染
/// pure：根据 mood 文本关键词映射 emoji。给 `/whoami` 头部 / 其它显示
/// mood 的位置加视觉前缀用。case-insensitive 子串匹配 — 优先级按表内
/// 顺序（命中即返）。无任何关键词命中 → 默认 🐾（paw）兜底，让所有
/// caller 都能拿到一个 emoji（而非 Option<&str>）减少调用方 if-let。
pub fn mood_emoji_for(text: &str) -> &'static str {
    let t = text.to_lowercase();
    // 按"最具体 → 最泛"的顺序避免歧义（如 "happy" 命中 😊 而非 "love" 的
    // 兜底）。中英 keywords 同表 — pet 中文 mood 描述常见，英文外语 caller
    // 走 LLM 输出也可能 hit。
    const TABLE: &[(&[&str], &str)] = &[
        // joy / excitement
        (&["兴奋", "激动", "excited", "thrilled"], "🤩"),
        (&["开心", "高兴", "happy", "cheerful", "joyful", "快乐"], "😊"),
        (&["love", "喜欢", "喜爱", "爱"], "🥰"),
        (&["proud", "骄傲", "自豪"], "😎"),
        // calm / contemplative
        (&["平静", "calm", "peaceful", "放松", "舒适"], "😌"),
        (&["curious", "好奇", "interested", "感兴趣"], "🤔"),
        // negative
        (&["sad", "难过", "失落", "沮丧"], "😢"),
        (&["angry", "生气", "愤怒", "frustrated"], "😠"),
        (&["worried", "担心", "焦虑", "anxious"], "😰"),
        (&["tired", "累", "困", "sleepy", "exhausted"], "😴"),
        (&["bored", "无聊", "boring"], "😑"),
        (&["shy", "害羞"], "😳"),
        (&["confused", "困惑", "迷茫"], "😕"),
        // hunger / physical
        (&["hungry", "饿"], "🍔"),
        // 兜底
    ];
    for (keys, emoji) in TABLE {
        for k in *keys {
            if t.contains(k) {
                return emoji;
            }
        }
    }
    "🐾"
}

pub fn format_whoami_reply(
    user_name: &str,
    companionship_days: Option<u64>,
    mood: Option<(String, Option<String>)>,
    persona_summary: &str,
    top_tools: &[(String, u64)],
) -> String {
    let mut lines: Vec<String> = Vec::new();
    // 第一行加 mood emoji 前缀（mood 非空时）让 owner 在 reply 顶端
    // 视觉化心情 — 不必扫到第三行的 💗 才知道宠物现在什么状态。无 mood
    // 时仍走纯 🪪 /whoami（保持 backwards-compat）。
    let header = match &mood {
        Some((text, _)) if !text.trim().is_empty() => {
            format!("{} 🪪 /whoami", mood_emoji_for(text))
        }
        _ => "🪪 /whoami".to_string(),
    };
    lines.push(header);
    let trimmed_name = user_name.trim();
    if !trimmed_name.is_empty() {
        lines.push(format!("🐾 我叫你「{}」。", trimmed_name));
    }
    if let Some(days) = companionship_days {
        if days == 0 {
            lines.push("📅 今天与你初识。".to_string());
        } else {
            lines.push(format!("📅 与你相伴已 {} 天。", days));
        }
    }
    if let Some((text, motion)) = mood {
        let t = text.trim();
        if !t.is_empty() {
            match motion {
                Some(m) if !m.trim().is_empty() => {
                    lines.push(format!("💗 现在的心情：{} · 动作组 {}", t, m.trim()));
                }
                _ => lines.push(format!("💗 现在的心情：{}", t)),
            }
        }
    }
    let summary = persona_summary.trim();
    if !summary.is_empty() {
        // 取首段：按双空行 / 单空行 切割（与桌面 `/whoami` 同算法）；> 90 字
        // 截断 + 省略号。短 persona summary 整段就是首段。
        let first = summary
            .split("\n\n")
            .next()
            .unwrap_or(summary)
            .trim();
        if !first.is_empty() {
            let head: String = if first.chars().count() > 90 {
                let mut h: String = first.chars().take(90).collect();
                h.push('…');
                h
            } else {
                first.to_string()
            };
            lines.push(format!("🪞 自我画像：{}", head));
        }
    }
    if !top_tools.is_empty() {
        let top3: Vec<String> = top_tools
            .iter()
            .take(3)
            .map(|(name, count)| format!("`{}`×{}", name, count))
            .collect();
        lines.push(format!("🛠 近常用工具：{}", top3.join(" · ")));
    }
    if lines.len() == 1 {
        // 兜底：所有源都空（fresh install / 全清状态）。与桌面 `/whoami` 同文案。
        lines.push("🐾 还没攒到自我介绍的素材，先一起聊聊吧。".to_string());
    }
    lines.join("\n")
}

/// `/today` 命令回复文案。pure：接收已过滤到本 chat 的 views + 今天日期，
/// 输出"今日到期"+"今日已完成" 两段标题清单。与桌面 `/today` 语义对齐：
/// - 到期桶 = Pending && due.date == today
/// - 完成桶 = Done && updated_at.starts_with(today_str)
/// - 两段都空 → "今日队列清爽 ✨"
/// 每段 cap 5，溢出补 `…还有 N 条`。
pub fn format_today_reply(
    views: &[crate::task_queue::TaskView],
    today: chrono::NaiveDate,
) -> String {
    use crate::task_queue::TaskStatus;
    let today_str = today.format("%Y-%m-%d").to_string();
    let mut due_today: Vec<&str> = Vec::new();
    let mut done_today: Vec<&str> = Vec::new();
    for v in views {
        match v.status {
            TaskStatus::Pending => {
                if let Some(due) = &v.due {
                    if due.len() >= 10 && due[..10] == today_str {
                        due_today.push(v.title.as_str());
                    }
                }
            }
            TaskStatus::Done => {
                if v.updated_at.starts_with(&today_str) {
                    done_today.push(v.title.as_str());
                }
            }
            TaskStatus::Error | TaskStatus::Cancelled => {}
        }
    }
    let mut out = String::new();
    out.push_str(&format!("📅 今日（{}）", today_str));
    if due_today.is_empty() && done_today.is_empty() {
        out.push_str("\n\n今日队列清爽 ✨");
        return out;
    }
    let render_bucket = |out: &mut String, header: &str, items: &[&str]| {
        if items.is_empty() {
            return;
        }
        out.push_str(&format!("\n\n{}（{}）：", header, items.len()));
        for t in items.iter().take(5) {
            out.push_str(&format!("\n· {}", t));
        }
        if items.len() > 5 {
            out.push_str(&format!("\n…还有 {} 条", items.len() - 5));
        }
    };
    render_bucket(&mut out, "今日到期", &due_today);
    render_bucket(&mut out, "今日已完成", &done_today);
    out
}

/// pure：把 DuePreset + today 展开为 (start, end) 闭区间日期范围。
/// - Tomorrow：[today+1, today+1]
/// - ThisWeek：[本周一, 本周日] (ISO 周 — 周一=0)
/// - NextWeek：[下周一, 下周日]
pub fn due_preset_range(
    preset: DuePreset,
    today: chrono::NaiveDate,
) -> (chrono::NaiveDate, chrono::NaiveDate) {
    use chrono::{Datelike, Duration};
    match preset {
        DuePreset::Tomorrow => {
            let t = today + Duration::days(1);
            (t, t)
        }
        DuePreset::ThisWeek => {
            let weekday = today.weekday().num_days_from_monday() as i64;
            let mon = today - Duration::days(weekday);
            let sun = mon + Duration::days(6);
            (mon, sun)
        }
        DuePreset::NextWeek => {
            let weekday = today.weekday().num_days_from_monday() as i64;
            let mon = today + Duration::days(7 - weekday);
            let sun = mon + Duration::days(6);
            (mon, sun)
        }
    }
}

/// `/due <preset>` 命令回复文案。pure：preset 为 None 时返 usage hint
/// 附 raw_arg 让 owner 一眼看自己输错的字面；Some 时按 due_preset_range
/// 算出 [start, end] 闭区间，列出 pending 任务里 `due` 字段日期落入区间
/// 的标题清单（按 due 升序）。空 → "该时段无 due 任务" 兜底。
pub fn format_due_reply(
    views: &[crate::task_queue::TaskView],
    preset: Option<DuePreset>,
    raw_arg: &str,
    today: chrono::NaiveDate,
) -> String {
    let p = match preset {
        Some(p) => p,
        None => {
            return format!(
                "📅 未识别 preset「{}」。\n\n用法：/due [preset]（缺省 tomorrow）\n  · tomorrow / tmr / 明天\n  · thisweek / 本周\n  · nextweek / 下周",
                raw_arg.trim()
            );
        }
    };
    let (start, end) = due_preset_range(p, today);
    let label = match p {
        DuePreset::Tomorrow => format!("明天（{}）", start.format("%Y-%m-%d")),
        DuePreset::ThisWeek => format!(
            "本周（{} ~ {}）",
            start.format("%m-%d"),
            end.format("%m-%d")
        ),
        DuePreset::NextWeek => format!(
            "下周（{} ~ {}）",
            start.format("%m-%d"),
            end.format("%m-%d")
        ),
    };
    use crate::task_queue::TaskStatus;
    let start_str = start.format("%Y-%m-%d").to_string();
    let end_str = end.format("%Y-%m-%d").to_string();
    let mut hits: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| matches!(v.status, TaskStatus::Pending))
        .filter(|v| match &v.due {
            Some(d) if d.len() >= 10 => {
                let date = &d[..10];
                date.as_bytes() >= start_str.as_bytes()
                    && date.as_bytes() <= end_str.as_bytes()
            }
            _ => false,
        })
        .collect();
    // due 升序（ISO 字典序 = 时间序）
    hits.sort_by(|a, b| a.due.cmp(&b.due));
    if hits.is_empty() {
        return format!("📅 {}\n\n该时段无 due 任务 ✨", label);
    }
    let mut out = String::new();
    out.push_str(&format!("📅 {}（{} 条）", label, hits.len()));
    for v in hits.iter().take(10) {
        // due 字段取 MM-DD HH:MM 显（解析失败 fallback 截 10）
        let when = v
            .due
            .as_deref()
            .map(|d| {
                if d.len() >= 16 {
                    format!("{} {}", &d[5..10], &d[11..16])
                } else {
                    d[..d.len().min(10)].to_string()
                }
            })
            .unwrap_or_default();
        out.push_str(&format!("\n· {} · {}", when, v.title));
    }
    if hits.len() > 10 {
        out.push_str(&format!("\n…还有 {} 条", hits.len() - 10));
    }
    out
}

/// `/recent <N>` 命令回复文案。pure：接收已过滤到本 chat 的 views + n cap，
/// 输出最近 N 条 done 任务标题清单（按 updated_at 倒序）。空 → "暂无完成
/// 记录"。format：`✅ HH:MM · title`，每行一条；末尾追加 grand total 兜底。
pub fn format_recent_reply(
    views: &[crate::task_queue::TaskView],
    n: u32,
) -> String {
    use crate::task_queue::TaskStatus;
    let mut done: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| matches!(v.status, TaskStatus::Done))
        .collect();
    // updated_at 是 ISO `YYYY-MM-DDThh:mm[:ss]±TZ` 字典序与时间序一致 — 倒
    // 序拿"最新完成在前"。
    done.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    if done.is_empty() {
        return "✨ 本聊天派单暂无完成记录。\n做完一条后 /done <标题> 标记，再来 /recent 看清单。".to_string();
    }
    let take_n = (n as usize).max(1);
    let shown = &done[..done.len().min(take_n)];
    let mut out = String::new();
    out.push_str(&format!(
        "✅ 最近 {} 条完成（共 {}）：",
        shown.len(),
        done.len()
    ));
    for v in shown {
        // updated_at 截 MM-DD HH:MM；解析失败兜原串前 16 字符
        let when = if v.updated_at.len() >= 16 {
            format!("{} {}", &v.updated_at[5..10], &v.updated_at[11..16])
        } else {
            v.updated_at.clone()
        };
        out.push_str(&format!("\n· {} · {}", when, v.title));
    }
    if done.len() > shown.len() {
        out.push_str(&format!(
            "\n…还有 {} 条更早完成（用 /recent {} 看更多，上限 20）",
            done.len() - shown.len(),
            (done.len()).min(20)
        ));
    }
    out
}

/// `/find <keyword>` 命令回复文案。pure：在 views（已 chat-scoped 过滤）里
/// 找 title / raw_description 含 keyword（case-insensitive）的项，至多列
/// 10 条。空 keyword → missing-argument 反馈。无命中 → "未找到"文案附
/// keyword 让 owner 一眼确认搜了啥。
pub fn format_find_reply(
    views: &[crate::task_queue::TaskView],
    keyword: &str,
) -> String {
    use crate::task_queue::TaskStatus;
    let kw = keyword.trim();
    if kw.is_empty() {
        return "🔍 用法：/find <keyword>\n按标题或描述子串搜本聊天派单（不分大小写，至多 10 条）。\n例：/find Downloads / /find 周报".to_string();
    }
    let kw_lower = kw.to_lowercase();
    let mut hits: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| {
            v.title.to_lowercase().contains(&kw_lower)
                || v.raw_description.to_lowercase().contains(&kw_lower)
        })
        .collect();
    // pending / error 在前（活跃任务更可能是 owner 当下想找的），其次 done /
    // cancelled。同状态保留 views 原序（视图层已应用 compare_for_queue 综合
    // 序）。
    let status_rank = |s: &TaskStatus| match s {
        TaskStatus::Pending => 0u8,
        TaskStatus::Error => 1,
        TaskStatus::Done => 2,
        TaskStatus::Cancelled => 3,
    };
    hits.sort_by_key(|v| status_rank(&v.status));
    if hits.is_empty() {
        return format!(
            "🔍 没有任务命中「{}」（搜了标题 + description 子串）。\n试试更短的关键词或部分字符；或 /tasks 看清单。",
            kw
        );
    }
    let cap = 10;
    let shown = &hits[..hits.len().min(cap)];
    let mut out = format!(
        "🔍 命中「{}」{} 条：",
        kw,
        hits.len()
    );
    for v in shown {
        let emoji = match v.status {
            TaskStatus::Pending => "🟢",
            TaskStatus::Error => "⚠️",
            TaskStatus::Done => "✅",
            TaskStatus::Cancelled => "🚫",
        };
        out.push_str(&format!("\n{} {}", emoji, v.title));
    }
    if hits.len() > cap {
        out.push_str(&format!(
            "\n…还有 {} 条命中（关键词太宽？试更精确的词）",
            hits.len() - cap
        ));
    }
    out
}

/// `/blocked` 命令回复文案。pure：接收已 chat-scoped 过滤的 views，
/// 1) 算 active 集合 = pending / error 状态的 title 集（done / cancelled 视
///    作"已解决"不阻塞依赖）；2) 对每条 view，把 `blocked_by` 与 active
///    集合求交集 = 仍未解决的 blocker；3) 仅当本条 view 也是 active 且未
///    解决 blocker 非空时列出。
///
/// 与 `task_queue::unresolved_blockers` 同算法（独立实现保 formatter 纯函
/// 数）。无命中 → "本聊天派单暂无被卡的 task"。
pub fn format_blocked_reply(views: &[crate::task_queue::TaskView]) -> String {
    use crate::task_queue::TaskStatus;
    let active: std::collections::HashSet<&str> = views
        .iter()
        .filter(|v| matches!(v.status, TaskStatus::Pending | TaskStatus::Error))
        .map(|v| v.title.as_str())
        .collect();
    let mut rows: Vec<(&str, Vec<&str>, &TaskStatus)> = Vec::new();
    for v in views {
        if !matches!(v.status, TaskStatus::Pending | TaskStatus::Error) {
            continue;
        }
        if v.blocked_by.is_empty() {
            continue;
        }
        let unresolved: Vec<&str> = v
            .blocked_by
            .iter()
            .filter(|b| active.contains(b.as_str()))
            .map(|s| s.as_str())
            .collect();
        if unresolved.is_empty() {
            continue;
        }
        rows.push((v.title.as_str(), unresolved, &v.status));
    }
    if rows.is_empty() {
        return "✅ 本聊天派单暂无被卡的 task（所有 active task 的 blockedBy 都解锁了）。".to_string();
    }
    let mut out = format!("🔒 被卡的 task {} 条：", rows.len());
    for (title, blockers, status) in &rows {
        let icon = match status {
            TaskStatus::Pending => "🟢",
            TaskStatus::Error => "⚠️",
            // unreachable per filter above, but keep arms exhaustive
            _ => "·",
        };
        out.push_str(&format!("\n{} {}", icon, title));
        for b in blockers {
            out.push_str(&format!("\n   └ 等：{}", b));
        }
    }
    out
}

/// `/snoozed` 命令回复文案。pure：接收已 chat-scoped + `snoozed_until.is_some()`
/// 过滤的 views，按醒来时刻升序排（最近醒的在前 — owner 想看"下一个回到队
/// 列的是哪条"），每行显 task + 倒计时（N 分 / N 时 / N 天 后醒）+ 状态
/// emoji。无 snoozed task → 友好引导文案。
///
/// `now` 由 caller 注入便于单测；生产用 `chrono::Local::now().naive_local()`。
pub fn format_snoozed_reply(
    views: &[crate::task_queue::TaskView],
    now: chrono::NaiveDateTime,
) -> String {
    use crate::task_queue::TaskStatus;
    let mut rows: Vec<(&crate::task_queue::TaskView, chrono::NaiveDateTime)> =
        Vec::new();
    for v in views {
        let Some(until_str) = &v.snoozed_until else {
            continue;
        };
        let Ok(until) =
            chrono::NaiveDateTime::parse_from_str(until_str, "%Y-%m-%dT%H:%M")
        else {
            continue;
        };
        rows.push((v, until));
    }
    if rows.is_empty() {
        return "💤 暂无被暂存的任务（本聊天派单中）。\n用 /snooze <标题> [30m / 2h / tonight / tomorrow / monday] 暂存一条；过点后自动回到队列。".to_string();
    }
    // 按 until asc：最近醒的先列（owner 关心"马上回到队列"那条）。
    rows.sort_by(|a, b| a.1.cmp(&b.1));
    let mut out = format!("💤 当前暂存任务（共 {} 条）", rows.len());
    for (v, until) in &rows {
        let icon = match v.status {
            TaskStatus::Pending => "⏳",
            TaskStatus::Error => "⚠️",
            TaskStatus::Done => "✅",
            TaskStatus::Cancelled => "🚫",
        };
        let diff = *until - now;
        let total_mins = diff.num_minutes();
        let label = if total_mins < 1 {
            "马上醒".to_string()
        } else if total_mins < 60 {
            format!("{} 分后醒", total_mins)
        } else if total_mins < 60 * 24 {
            let h = total_mins / 60;
            let m = total_mins % 60;
            if m == 0 {
                format!("{} 时后醒", h)
            } else {
                format!("{} 时 {} 分后醒", h, m)
            }
        } else {
            let d = total_mins / (60 * 24);
            let h = (total_mins % (60 * 24)) / 60;
            if h == 0 {
                format!("{} 天后醒", d)
            } else {
                format!("{} 天 {} 时后醒", d, h)
            }
        };
        // 时刻截 `MM-DD HH:MM` — until_str 是 `YYYY-MM-DDTHH:MM`，5..10 + 11..16 二段。
        let until_short =
            if let Some(s) = v.snoozed_until.as_deref().filter(|s| s.len() >= 16) {
                format!("{} {}", &s[5..10], &s[11..16])
            } else {
                v.snoozed_until.clone().unwrap_or_default()
            };
        out.push_str(&format!(
            "\n{} {} · {}（{}）",
            icon, v.title, label, until_short
        ));
    }
    out
}

/// `/mute [N]` 命令回复文案。pure：caller 已经调过 `set_mute_minutes(minutes)`
/// 实际写后端 MUTE_UNTIL；本函数仅按 minutes 与 caller 注入的 `until_local`
/// （None = 已清；Some = 解除时刻）生成 owner 友好的反馈：
/// - minutes > 0 + until Some → "🔕 已静音 N 分钟（到 HH:MM 自动解除）"
/// - minutes == 0 / until None → "🔊 已解除静音"
///
/// `until_local` 由 caller 用 `chrono::Local::now() + Duration::minutes(N)`
/// 拼出（保 pure 函数 — 不读时钟）。
pub fn format_mute_reply(
    minutes: i64,
    until_local: Option<chrono::DateTime<chrono::Local>>,
) -> String {
    if minutes <= 0 {
        return "🔊 已解除静音（proactive 主动开口恢复）。".to_string();
    }
    let when = until_local
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_else(|| "—".to_string());
    let nice = if minutes < 60 {
        format!("{} 分钟", minutes)
    } else if minutes < 60 * 24 {
        let h = minutes / 60;
        let m = minutes % 60;
        if m == 0 {
            format!("{} 小时", h)
        } else {
            format!("{} 小时 {} 分钟", h, m)
        }
    } else {
        let d = minutes / (60 * 24);
        let h = (minutes % (60 * 24)) / 60;
        if h == 0 {
            format!("{} 天", d)
        } else {
            format!("{} 天 {} 小时", d, h)
        }
    };
    format!(
        "🔕 已静音 proactive {}（到 {} 自动解除）。期间宠物不主动开口；用 /mute 0 立刻解除。",
        nice, when
    )
}

/// `/digest [N]` 命令回复文案。pure：接收 chat-scoped views + n cap，
/// 输出最近 N 条 done 任务的标题 + `[result: ...]` 摘要一行式（按
/// updated_at 倒序）。与 `format_recent_reply` 互补 — recent 仅标题，
/// digest 含 result 摘要让 owner 在 TG 上看具体产物。
///
/// 单行格式：`· MM-DD HH:MM · title — result` （result 缺时省 `—` 段）。
/// result 截 80 字 + `…` 避免单条爆行；header 显共多少条 done + 实际 cap。
pub fn format_digest_reply(
    views: &[crate::task_queue::TaskView],
    n: u32,
) -> String {
    use crate::task_queue::TaskStatus;
    let mut done: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| matches!(v.status, TaskStatus::Done))
        .collect();
    done.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    if done.is_empty() {
        return "✨ 本聊天派单暂无完成记录。\n做完一条 /done <标题> 标记，再来 /digest 看清单（含 [result:] 摘要）。"
            .to_string();
    }
    let cap = (n as usize).max(1);
    let shown = &done[..done.len().min(cap)];
    let mut out = format!(
        "📋 最近 {} 条完成（共 {}）：",
        shown.len(),
        done.len()
    );
    for v in shown {
        let when = if v.updated_at.len() >= 16 {
            format!("{} {}", &v.updated_at[5..10], &v.updated_at[11..16])
        } else {
            v.updated_at.clone()
        };
        let result_part = match v.result.as_deref() {
            Some(r) if !r.trim().is_empty() => {
                let r = r.trim();
                let chars: Vec<char> = r.chars().collect();
                let summary = if chars.len() > 80 {
                    let s: String = chars.iter().take(80).collect();
                    format!("{}…", s)
                } else {
                    r.to_string()
                };
                format!(" — {}", summary)
            }
            _ => String::new(),
        };
        out.push_str(&format!("\n· {} · {}{}", when, v.title, result_part));
    }
    if done.len() > shown.len() {
        out.push_str(&format!(
            "\n…还有 {} 条更早完成（/digest {} 看更多，上限 20）",
            done.len() - shown.len(),
            done.len().min(20)
        ));
    }
    out
}

/// `/note <text>` 命令回复文案。pure：
/// - 空 / 全空白 text → usage hint
/// - save_result == Some(title) → "📝 已记到 general/<title>"，附复制
///   预览（前 60 字 + …）让 owner 在 TG 看到"我刚记了啥"
/// - save_result == Err(msg) → 失败反馈含原 err
pub fn format_note_reply(text: &str, save_result: Result<&str, &str>) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "📝 用法：/note <text>\n\n把任意一段文本作 general memory item 存盘（随手记一笔；进 PanelMemory → 通用 段查看 / 整理）。\n\n例：/note 周末跑 5km 后腿酸；下次先热身\n例：/note 想试试 sourdough 起子培养".to_string();
    }
    match save_result {
        Ok(title) => {
            let preview = if trimmed.chars().count() > 60 {
                let s: String = trimmed.chars().take(60).collect();
                format!("{}…", s)
            } else {
                trimmed.to_string()
            };
            format!(
                "📝 已记到 general/{}\n\n{}",
                title, preview
            )
        }
        Err(e) => format!("📝 保存失败：{}", e),
    }
}

/// `/show <title>` 命令回复文案。pure：
/// - title 行 + status emoji
/// - raw_description 全量（含 markers），cap 1500 char 防 TG 4096 上限被
///   detail 段挤爆
/// - detail.md 段：非空时显前 `DETAIL_PREVIEW_CHARS` 字符 + 总字数 hint
///
/// caller 负责传 task_get_detail 拉的 raw_description / detail_md / status。
pub const SHOW_RAW_DESC_CAP: usize = 1500;
pub const SHOW_DETAIL_PREVIEW_CHARS: usize = 300;
pub fn format_show_reply(
    title: &str,
    raw_description: &str,
    detail_md: &str,
    status: crate::task_queue::TaskStatus,
) -> String {
    use crate::task_queue::TaskStatus;
    let status_emoji = match status {
        TaskStatus::Pending => "⏳",
        TaskStatus::Done => "✅",
        TaskStatus::Error => "⚠️",
        TaskStatus::Cancelled => "🚫",
    };
    let mut out = String::new();
    out.push_str(&format!("🔬 {} 「{}」", status_emoji, title.trim()));
    out.push_str("\n\n");
    // raw_description trim + cap。空 description（极端情况）显占位防"空响应"。
    let raw = raw_description.trim();
    let raw_total = raw.chars().count();
    if raw_total == 0 {
        out.push_str("（raw_description 为空）");
    } else if raw_total > SHOW_RAW_DESC_CAP {
        let head: String = raw.chars().take(SHOW_RAW_DESC_CAP).collect();
        out.push_str(&head);
        out.push_str(&format!(
            "\n…（raw 截断 · 共 {} 字符）",
            raw_total
        ));
    } else {
        out.push_str(raw);
    }
    // detail.md：空文件直接省略；非空显前 N 字符 + 字数 hint
    let detail = detail_md.trim();
    if !detail.is_empty() {
        let detail_total = detail.chars().count();
        let preview: String = if detail_total > SHOW_DETAIL_PREVIEW_CHARS {
            let head: String = detail.chars().take(SHOW_DETAIL_PREVIEW_CHARS).collect();
            format!("{}…", head)
        } else {
            detail.to_string()
        };
        out.push_str(&format!(
            "\n\n📝 detail.md（{} 字符）:\n{}",
            detail_total, preview
        ));
    }
    out
}

/// `/reflect <text>` 命令回复文案。pure，与 format_note_reply 同模板但
/// 走 ai_insights 类目（消息文案点明分类不同避免与 /note 混淆）。
/// - 空 / 全空白 text → usage hint（带 /note 对照例避免 owner 用错入口）
/// - Ok(title) → "🪞 已记到 ai_insights/<title>" + 前 60 字预览
/// - Err(msg) → "🪞 保存失败：<msg>"
pub fn format_reflect_reply(text: &str, save_result: Result<&str, &str>) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "🪞 用法：/reflect <text>\n\n把任意一段反思 / 自我观察文本作 ai_insights memory item 存盘（进 PanelMemory → AI 洞察 段查看）。\n\n例：/reflect 今天回顾：我对中断接受度过高\n例：/reflect 观察：长 task 拆细后完成率明显提升\n\n对比 /note：那个写 general（杂项 brain-dump），这个写 ai_insights（反思）— 按信号类型分流。".to_string();
    }
    match save_result {
        Ok(title) => {
            let preview = if trimmed.chars().count() > 60 {
                let s: String = trimmed.chars().take(60).collect();
                format!("{}…", s)
            } else {
                trimmed.to_string()
            };
            format!("🪞 已记到 ai_insights/{}\n\n{}", title, preview)
        }
        Err(e) => format!("🪞 保存失败：{}", e),
    }
}

/// `/edit <title> :: <new desc>` 命令回复文案。pure：
/// - title 或 new_desc trim 后任一空 → usage hint（与 missing-arg 同模板
///   但带 `::` separator 例子，避免 owner 看完不懂怎么写）
/// - save_result == Ok(()) → "✏️ 已覆写「<title>」"+ 新 desc 前 80 字预览
/// - save_result == Err(msg) → 失败反馈含原 err
pub fn format_edit_reply(
    title: &str,
    new_desc: &str,
    save_result: Result<(), &str>,
) -> String {
    let t = title.trim();
    let d = new_desc.trim();
    if t.is_empty() || d.is_empty() {
        return "✏️ 用法：/edit <title> :: <new desc>\n\n覆写指定 butler task 的 description 整段。`::` 是必填 separator（让 title 含空格 / 中文标点也能精确切）。\n\n例：/edit 整理 Downloads :: 整理 Downloads [task pri=5 due=2026-05-20] [pinned]\n例：/edit 写周报 :: 完整新 body 一段\n\n注意：新 desc 完全覆写旧描述。想保留 [task pri=...] [every: ...] [pinned] 等 markers 请自行写进新 desc。".to_string();
    }
    match save_result {
        Ok(()) => {
            let preview = if d.chars().count() > 80 {
                let s: String = d.chars().take(80).collect();
                format!("{}…", s)
            } else {
                d.to_string()
            };
            format!("✏️ 已覆写「{}」\n\n{}", t, preview)
        }
        Err(e) => format!("✏️ 覆写失败：{}", e),
    }
}

/// `/reset` 命令固定回复文案。caller 负责真正清空 session_messages（仅保留
/// system / 人设），本函数只生成给 TG 用户看的反馈。
pub fn format_reset_reply() -> String {
    "🔄 已重置对话上下文（保留人设 / 系统提示）。".to_string()
}

/// `/version` 命令回复文案。app_version 走 `env!("CARGO_PKG_VERSION")`；
/// schema_version 走 _migrations 表最大 version。app_version 空 → fallback
/// "（版本号缺失）"；schema_version=0（旧 backend / 读失败）→ 该行省略。
pub fn format_version_reply(app_version: &str, schema_version: i32) -> String {
    let mut out = String::new();
    if app_version.is_empty() {
        out.push_str("🐾 pet（版本号缺失）");
    } else {
        out.push_str(&format!("🐾 pet v{}", app_version));
    }
    if schema_version > 0 {
        out.push_str(&format!("\nschema v{}", schema_version));
    }
    out
}

const TG_TASKS_MSG_LIMIT: usize = 4096;
const TG_TASK_SUFFIX_MAX: usize = 40;

fn format_task_line(emoji: &str, v: &crate::task_queue::TaskView) -> String {
    use crate::task_queue::TaskStatus;
    let mut line = String::new();
    line.push_str(emoji);
    line.push(' ');
    if v.priority > 0 {
        line.push_str(&format!("P{} ", v.priority));
    }
    line.push_str(v.title.trim());

    let suffix: Option<String> = match v.status {
        TaskStatus::Pending => v.due.as_deref().and_then(format_due_short),
        TaskStatus::Done => v.result.as_deref().map(truncate_suffix),
        TaskStatus::Error | TaskStatus::Cancelled => {
            v.error_message.as_deref().map(truncate_suffix)
        }
    };
    if let Some(s) = suffix {
        line.push_str(" — ");
        line.push_str(&s);
    }
    line
}

/// `2026-05-05T18:00` → `截至 5/5 18:00`。无效格式 → None（前置 due 字段
/// 就是 `task_queue` 写出来的标准字符串，理论上不会失败；防御性设 None
/// 而不是 panic）。
fn format_due_short(due: &str) -> Option<String> {
    use chrono::{Datelike, Timelike};
    let dt = chrono::NaiveDateTime::parse_from_str(due, "%Y-%m-%dT%H:%M").ok()?;
    Some(format!(
        "截至 {}/{} {:02}:{:02}",
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute(),
    ))
}

/// 把 suffix 字符按 char 数截断到 `TG_TASK_SUFFIX_MAX`，超长追加 `…`。
/// 用 char 而非 byte 因为中文常出现，按 byte 截断容易切坏字。
fn truncate_suffix(s: &str) -> String {
    let trimmed = s.trim();
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= TG_TASK_SUFFIX_MAX {
        trimmed.to_string()
    } else {
        let mut out: String = chars.into_iter().take(TG_TASK_SUFFIX_MAX).collect();
        out.push('…');
        out
    }
}

/// 单条 TG 消息上限是 4096 byte。超出时在末尾追加截断提示，并按 byte
/// 安全边界切到上限附近。`total_count` 是原始任务总数（用来给提示算
/// "剩余 N 条"）。
fn truncate_if_overflow(s: String, total_count: usize) -> String {
    if s.len() <= TG_TASKS_MSG_LIMIT {
        return s;
    }
    let suffix_template = "\n\n…(列表过长，剩余 {N} 条请回桌面查看)";
    let mut budget = TG_TASKS_MSG_LIMIT.saturating_sub(suffix_template.len() + 8);
    while !s.is_char_boundary(budget) && budget > 0 {
        budget -= 1;
    }
    let kept = &s[..budget];
    let kept_lines = kept.lines().filter(|l| starts_with_status_emoji(l)).count();
    let remaining = total_count.saturating_sub(kept_lines);
    format!(
        "{}\n\n…(列表过长，剩余 {} 条请回桌面查看)",
        kept.trim_end(),
        remaining
    )
}

fn starts_with_status_emoji(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("⏳")
        || trimmed.starts_with("✅")
        || trimmed.starts_with("⚠️")
        || trimmed.starts_with("🚫")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    // -------- parse_tg_command --------

    #[test]
    fn parse_cancel_with_title() {
        assert_eq!(
            parse_tg_command("/cancel 整理 Downloads"),
            Some(TgCommand::Cancel {
                title: "整理 Downloads".to_string()
            })
        );
    }

    #[test]
    fn parse_retry_with_title() {
        assert_eq!(
            parse_tg_command("/retry 跑步"),
            Some(TgCommand::Retry {
                title: "跑步".to_string()
            })
        );
    }

    #[test]
    fn parse_done_with_title() {
        assert_eq!(
            parse_tg_command("/done 写日报"),
            Some(TgCommand::Done {
                title: "写日报".to_string()
            })
        );
    }

    #[test]
    fn parse_done_empty_title() {
        // 空 title 走 handler missing-argument 分支
        assert_eq!(parse_tg_command("/done"), Some(TgCommand::Done { title: "".to_string() }));
        assert_eq!(parse_tg_command("/done   "), Some(TgCommand::Done { title: "".to_string() }));
    }

    #[test]
    fn done_command_name_and_title() {
        let c = TgCommand::Done { title: "x".to_string() };
        assert_eq!(c.name(), "done");
        assert_eq!(c.title(), "x");
    }

    #[test]
    fn format_done_success_includes_panel_hint() {
        let msg = format_command_success("done", "整理 Downloads");
        assert!(msg.contains("✓ 已标 done"));
        assert!(msg.contains("整理 Downloads"));
        assert!(msg.contains("result"), "should hint that result needs desktop");
    }

    #[test]
    fn parse_command_is_case_insensitive() {
        assert_eq!(
            parse_tg_command("/CANCEL x"),
            Some(TgCommand::Cancel {
                title: "x".to_string()
            })
        );
        assert_eq!(
            parse_tg_command("/Retry y"),
            Some(TgCommand::Retry {
                title: "y".to_string()
            })
        );
    }

    #[test]
    fn parse_command_trims_leading_whitespace_in_text() {
        // TG 客户端有时在 / 前加空格（手机自动加），不应当成 None
        assert_eq!(
            parse_tg_command("  /cancel x"),
            Some(TgCommand::Cancel {
                title: "x".to_string()
            })
        );
    }

    #[test]
    fn parse_command_trims_arg_whitespace() {
        assert_eq!(
            parse_tg_command("/cancel   整理   Downloads   "),
            Some(TgCommand::Cancel {
                title: "整理   Downloads".to_string()
            })
        );
    }

    #[test]
    fn parse_command_with_empty_arg() {
        // /cancel 单独发：parse 仍命中 Cancel，title 是空字符串；handler 据此走"缺参"分支
        assert_eq!(
            parse_tg_command("/cancel"),
            Some(TgCommand::Cancel {
                title: String::new()
            })
        );
        assert_eq!(
            parse_tg_command("/cancel   "),
            Some(TgCommand::Cancel {
                title: String::new()
            })
        );
    }

    #[test]
    fn parse_command_unknown() {
        // /help 现在是正式命令；这里用纯臆造名验证 Unknown 路径
        assert_eq!(
            parse_tg_command("/zzznotacmd"),
            Some(TgCommand::Unknown {
                name: "zzznotacmd".to_string()
            })
        );
        assert_eq!(
            parse_tg_command("/foobar arg"),
            Some(TgCommand::Unknown {
                name: "foobar".to_string()
            })
        );
    }

    #[test]
    fn parse_returns_none_for_non_command_text() {
        // 普通文本走 chat pipeline，不该被命令拦截
        assert_eq!(parse_tg_command("帮我整理 Downloads"), None);
        assert_eq!(parse_tg_command("早上好"), None);
        assert_eq!(parse_tg_command(""), None);
    }

    #[test]
    fn parse_returns_none_for_lone_slash() {
        // 单个 / 不是命令
        assert_eq!(parse_tg_command("/"), None);
    }

    #[test]
    fn parse_unknown_preserves_lowercase_name() {
        // 文案要展示给用户，统一小写。/HeLp 现已是 Help variant，换个臆造名。
        assert_eq!(
            parse_tg_command("/FoOBaR"),
            Some(TgCommand::Unknown {
                name: "foobar".to_string()
            })
        );
    }

    // -------- format_* helpers --------

    #[test]
    fn success_cancel_uses_block_emoji() {
        let s = format_command_success("cancel", "整理 Downloads");
        assert!(s.starts_with("🚫"));
        assert!(s.contains("「整理 Downloads」"));
        // 反向命令指引（连续操作场景下省去回 /help 查语法）
        assert!(s.contains("/retry 整理 Downloads"));
    }

    #[test]
    fn success_retry_uses_arrow_emoji_and_explains() {
        let s = format_command_success("retry", "跑步");
        assert!(s.starts_with("🔄"));
        assert!(s.contains("「跑步」"));
        assert!(s.contains("pending"));
        // 反向命令指引
        assert!(s.contains("/cancel 跑步"));
    }

    #[test]
    fn error_uses_warning_emoji_and_includes_err() {
        let s = format_command_error("task not found: x");
        assert!(s.starts_with("⚠️"));
        assert!(s.contains("task not found: x"));
    }

    #[test]
    fn unknown_lists_available_commands() {
        let s = format_unknown_command("foo", None);
        assert!(s.contains("/foo"));
        // 收紧后：未知命令仅指向 /help，详细列表交给 format_help_text
        assert!(s.contains("/help"));
    }

    #[test]
    fn unknown_with_suggestion_puts_hint_in_first_line() {
        // TG 客户端通知预览常只显首行，建议放最前比"未知命令"更有用
        let s = format_unknown_command("tsks", Some("tasks"));
        let first_line = s.lines().next().unwrap();
        assert!(first_line.contains("/tasks"), "first line should hint /tasks: {}", first_line);
        assert!(s.contains("/tsks"), "still mentions the typo: {}", s);
        assert!(s.contains("/help"));
    }

    // -------- levenshtein --------

    #[test]
    fn levenshtein_zero_for_identical() {
        assert_eq!(levenshtein("tasks", "tasks"), 0);
        assert_eq!(levenshtein("", ""), 0);
    }

    #[test]
    fn levenshtein_handles_empty_inputs() {
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", "abc"), 3);
    }

    #[test]
    fn levenshtein_single_edit_operations() {
        // 一次插入 / 删除 / 替换都是距 1
        assert_eq!(levenshtein("tasks", "task"), 1); // 删除
        assert_eq!(levenshtein("task", "tasks"), 1); // 插入
        assert_eq!(levenshtein("tasks", "tasys"), 1); // 替换
    }

    #[test]
    fn levenshtein_typical_typos() {
        // 漏字母 / 顺序错（顺序错 = 一次替换 + 一次替换 = 2）
        assert_eq!(levenshtein("tsks", "tasks"), 1); // 漏 a
        assert_eq!(levenshtein("ttasks", "tasks"), 1); // 多 t
        assert_eq!(levenshtein("taska", "tasks"), 1); // a vs s
    }

    // -------- suggest_command --------

    #[test]
    fn suggest_picks_within_threshold() {
        let valid = ["task", "tasks", "cancel", "retry", "help"];
        // tsks → tasks (距 1)
        assert_eq!(suggest_command("tsks", &valid), Some("tasks"));
        // cancl → cancel (距 1)
        assert_eq!(suggest_command("cancl", &valid), Some("cancel"));
        // retry → retry (距 0 — 但这种应该已被 parse 命中，suggest 不会被
        // 调用；测试确保仍正确)
        assert_eq!(suggest_command("retry", &valid), Some("retry"));
    }

    #[test]
    fn suggest_returns_none_above_threshold() {
        let valid = ["task", "tasks", "cancel", "retry", "help"];
        // 距 3 (整体改写) 不命中
        assert_eq!(suggest_command("xyzzy", &valid), None);
        // 距 4
        assert_eq!(suggest_command("blahblah", &valid), None);
    }

    #[test]
    fn suggest_picks_first_valid_when_distances_tie() {
        // 用人造命令名构造严格 tie：input "abc" 与 "abx" / "aby" 距离都 = 1。
        // valid 顺序里 "abx" 在前应优先。
        let valid = ["abx", "aby"];
        assert_eq!(suggest_command("abc", &valid), Some("abx"));
        // 反过来 → 取 "aby"
        let valid_rev = ["aby", "abx"];
        assert_eq!(suggest_command("abc", &valid_rev), Some("aby"));
    }

    #[test]
    fn suggest_returns_none_for_empty_inputs() {
        let valid = ["task", "tasks"];
        assert_eq!(suggest_command("", &valid), None);
        assert_eq!(suggest_command("tsks", &[]), None);
    }

    #[test]
    fn missing_argument_shows_usage() {
        let s = format_missing_argument("cancel");
        assert!(s.contains("/cancel <任务标题>"));
    }

    // -------- TgCommand accessors --------

    #[test]
    fn name_and_title_accessors() {
        let cancel = TgCommand::Cancel {
            title: "x".to_string(),
        };
        assert_eq!(cancel.name(), "cancel");
        assert_eq!(cancel.title(), "x");

        let retry = TgCommand::Retry {
            title: "y".to_string(),
        };
        assert_eq!(retry.name(), "retry");

        let unk = TgCommand::Unknown {
            name: "foo".to_string(),
        };
        assert_eq!(unk.name(), "foo");
        assert_eq!(unk.title(), "");

        let tasks = TgCommand::Tasks;
        assert_eq!(tasks.name(), "tasks");
        assert_eq!(tasks.title(), "");

        let help = TgCommand::Help { topic: None };
        assert_eq!(help.name(), "help");
        assert_eq!(help.title(), "");

        let task = TgCommand::Task {
            title: "整理 Downloads".to_string(),
            priority: 3,
        };
        assert_eq!(task.name(), "task");
        assert_eq!(task.title(), "整理 Downloads");
    }

    // -------- /task (singular: create) parsing --------

    #[test]
    fn parse_task_create_command() {
        assert_eq!(
            parse_tg_command("/task 整理 Downloads"),
            Some(TgCommand::Task {
                title: "整理 Downloads".to_string(),
                priority: 3,
            })
        );
    }

    #[test]
    fn parse_task_empty_title_yields_empty_title_variant() {
        // 空 title 不在解析层报错，让 handler 走统一的 missing-argument
        // 反馈，与 /cancel / /retry 行为对称。
        assert_eq!(
            parse_tg_command("/task"),
            Some(TgCommand::Task {
                title: "".to_string(),
                priority: 3,
            })
        );
        assert_eq!(
            parse_tg_command("/task   "),
            Some(TgCommand::Task {
                title: "".to_string(),
                priority: 3,
            })
        );
    }

    #[test]
    fn parse_task_distinct_from_tasks() {
        // 单 vs 复数：用户在 TG 客户端两个命令补全都看得到，分别落到不同
        // variant —— 解析层若把 /task 误归到 /tasks 就会让"创建"跳到"列表"。
        assert!(matches!(
            parse_tg_command("/task hello"),
            Some(TgCommand::Task { .. })
        ));
        assert_eq!(parse_tg_command("/tasks"), Some(TgCommand::Tasks));
    }

    #[test]
    fn parse_task_is_case_insensitive() {
        assert_eq!(
            parse_tg_command("/TASK abc"),
            Some(TgCommand::Task {
                title: "abc".to_string(),
                priority: 3,
            })
        );
    }

    #[test]
    fn format_task_created_success_includes_title_and_followups() {
        let s = format_task_created_success("整理 Downloads", 3);
        assert!(s.contains("整理 Downloads"), "should mention title: {}", s);
        assert!(s.contains("P3"), "should mention default priority P3: {}", s);
        assert!(s.contains("/tasks"), "should hint /tasks: {}", s);
        assert!(s.contains("/cancel"), "should hint /cancel: {}", s);
    }

    #[test]
    fn format_task_created_success_renders_actual_priority() {
        // 紧迫 / 最紧迫档要在反馈里直接展示 P5 / P7，让用户验证前缀真的命中
        // 而不是被识别成 title 的一部分。
        let s5 = format_task_created_success("交报告", 5);
        assert!(s5.contains("P5"), "P5 should appear: {}", s5);
        assert!(!s5.contains("P3"), "must not still say P3: {}", s5);
        let s7 = format_task_created_success("交报告", 7);
        assert!(s7.contains("P7"), "P7 should appear: {}", s7);
    }

    // -------- /task priority prefix --------

    #[test]
    fn parse_prefix_no_marks_keeps_default_priority() {
        let (p, t) = parse_task_prefix("整理 Downloads");
        assert_eq!(p, 3);
        assert_eq!(t, "整理 Downloads");
    }

    #[test]
    fn parse_prefix_two_bangs_maps_to_p5() {
        let (p, t) = parse_task_prefix("!! 交报告");
        assert_eq!(p, 5);
        assert_eq!(t, "交报告");
    }

    #[test]
    fn parse_prefix_three_bangs_maps_to_p7() {
        let (p, t) = parse_task_prefix("!!! 交报告");
        assert_eq!(p, 7);
        assert_eq!(t, "交报告");
    }

    #[test]
    fn parse_prefix_preserves_multi_token_title() {
        // tail 多个 token：用 split_once 切首个 whitespace，剩下整体保留
        let (p, t) = parse_task_prefix("!! foo bar baz");
        assert_eq!(p, 5);
        assert_eq!(t, "foo bar baz");
    }

    #[test]
    fn parse_prefix_only_bangs_no_title_yields_empty_title() {
        // 只有前缀没标题：让 handler 走 missing-argument 反馈，错误更精确
        let (p, t) = parse_task_prefix("!!");
        assert_eq!(p, 5);
        assert_eq!(t, "");
        let (p3, t3) = parse_task_prefix("!!!");
        assert_eq!(p3, 7);
        assert_eq!(t3, "");
    }

    #[test]
    fn parse_prefix_four_bangs_falls_back_to_default() {
        // 4 个 ！ 不识别，整体回退到 P3 + 当 title 一部分（用户大概率是
        // 表达兴奋而非档次）
        let (p, t) = parse_task_prefix("!!!! foo");
        assert_eq!(p, 3);
        assert_eq!(t, "!!!! foo");
    }

    #[test]
    fn parse_prefix_single_bang_falls_back_to_default() {
        // 单个 ！ 不在三档表里，整体回退默认
        let (p, t) = parse_task_prefix("! foo");
        assert_eq!(p, 3);
        assert_eq!(t, "! foo");
    }

    #[test]
    fn parse_tg_command_threads_priority_prefix_into_task_variant() {
        assert_eq!(
            parse_tg_command("/task !! 交报告"),
            Some(TgCommand::Task {
                title: "交报告".to_string(),
                priority: 5,
            })
        );
        assert_eq!(
            parse_tg_command("/task !!! 立刻搞"),
            Some(TgCommand::Task {
                title: "立刻搞".to_string(),
                priority: 7,
            })
        );
    }

    // -------- /tasks parsing --------

    #[test]
    fn parse_tasks_command() {
        assert_eq!(parse_tg_command("/tasks"), Some(TgCommand::Tasks));
    }

    #[test]
    fn parse_tasks_is_case_insensitive() {
        assert_eq!(parse_tg_command("/TASKS"), Some(TgCommand::Tasks));
        assert_eq!(parse_tg_command("/Tasks"), Some(TgCommand::Tasks));
    }

    #[test]
    fn parse_tasks_ignores_trailing_argument() {
        // 多余的参数（用户随手加的过滤词等）一律忽略而非走 Unknown，
        // 让 `/tasks since:7d` 这种探索式输入直接命中 Tasks。
        assert_eq!(parse_tg_command("/tasks since:7d"), Some(TgCommand::Tasks));
        assert_eq!(parse_tg_command("/tasks   "), Some(TgCommand::Tasks));
    }

    // -------- /help parsing + format --------

    #[test]
    fn parse_help_command_no_topic() {
        assert_eq!(
            parse_tg_command("/help"),
            Some(TgCommand::Help { topic: None })
        );
        assert_eq!(
            parse_tg_command("/help   "),
            Some(TgCommand::Help { topic: None })
        );
    }

    #[test]
    fn parse_help_is_case_insensitive() {
        assert_eq!(
            parse_tg_command("/HELP"),
            Some(TgCommand::Help { topic: None })
        );
        assert_eq!(
            parse_tg_command("/Help"),
            Some(TgCommand::Help { topic: None })
        );
    }

    #[test]
    fn parse_help_with_topic_keeps_arg() {
        assert_eq!(
            parse_tg_command("/help cancel"),
            Some(TgCommand::Help {
                topic: Some("cancel".to_string())
            })
        );
        // `/` 前缀也接受
        assert_eq!(
            parse_tg_command("/help /snooze"),
            Some(TgCommand::Help {
                topic: Some("/snooze".to_string())
            })
        );
    }

    #[test]
    fn format_help_for_topic_strips_slash_prefix() {
        let s = format_help_for_topic("/cancel", &[]);
        assert!(s.contains("/cancel"));
        assert!(s.contains("用法"));
    }

    #[test]
    fn format_help_for_topic_is_case_insensitive() {
        let s = format_help_for_topic("CANCEL", &[]);
        assert!(s.contains("/cancel"));
    }

    #[test]
    fn format_help_for_unknown_topic_returns_friendly_hint() {
        let s = format_help_for_topic("nope", &[]);
        assert!(s.contains("未知命令"), "{s}");
        assert!(s.contains("/help"), "{s}");
    }

    #[test]
    fn format_help_for_custom_command_returns_owner_description() {
        let custom = vec![crate::commands::settings::TgCustomCommand {
            name: "morning".to_string(),
            description: "把今天的日历汇总发到群".to_string(),
        }];
        let s = format_help_for_topic("morning", &custom);
        assert!(s.contains("/morning"), "{s}");
        assert!(s.contains("自定义命令"), "{s}");
        assert!(s.contains("把今天的日历汇总发到群"), "{s}");
    }

    #[test]
    fn format_help_for_empty_topic_falls_back_to_full_help() {
        // 空 topic 视作 /help 无参 — 显全表
        let s = format_help_for_topic("", &[]);
        let full = format_help_text(&[]);
        assert_eq!(s, full);
    }

    #[test]
    fn format_help_for_each_listed_command_returns_detail() {
        // 全表里每条命令都应该有 /help <cmd> 详细文案，避免 drift
        for name in [
            "task", "tasks", "stats", "done", "cancel", "retry", "snooze",
            "unsnooze", "pin", "unpin", "pinned", "silent", "unsilent",
            "silenced", "markers", "mood", "whoami", "today", "due", "recent",
            "digest", "edit", "reflect", "find", "show", "blocked", "snoozed",
            "reset", "version", "help",
        ] {
            let s = format_help_for_topic(name, &[]);
            assert!(s.contains("用法"), "{name} missing 用法 section: {s}");
            assert!(!s.contains("未知命令"), "{name} fell to unknown branch: {s}");
        }
    }

    // -------- fuzzy match --------

    fn ts(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn fuzzy_returns_none_for_empty_query() {
        let titles = ts(&["整理 Downloads", "跑步"]);
        assert_eq!(find_task_fuzzy("", &titles), FuzzyMatch::None);
        assert_eq!(find_task_fuzzy("   ", &titles), FuzzyMatch::None);
    }

    #[test]
    fn fuzzy_returns_exact_match_first() {
        let titles = ts(&["整理 Downloads", "整理"]);
        // query "整理" 子串命中两条，但精确匹配 "整理" 优先（Exact > Single）
        assert_eq!(
            find_task_fuzzy("整理", &titles),
            FuzzyMatch::Exact("整理".to_string()),
        );
    }

    #[test]
    fn fuzzy_returns_exact_match_with_trim() {
        let titles = ts(&["整理 Downloads"]);
        assert_eq!(
            find_task_fuzzy("  整理 Downloads  ", &titles),
            FuzzyMatch::Exact("整理 Downloads".to_string()),
        );
    }

    #[test]
    fn fuzzy_returns_single_substring_match() {
        let titles = ts(&["整理 Downloads", "跑步"]);
        assert_eq!(
            find_task_fuzzy("Down", &titles),
            FuzzyMatch::Single("整理 Downloads".to_string()),
        );
    }

    #[test]
    fn fuzzy_substring_is_case_insensitive() {
        let titles = ts(&["整理 Downloads"]);
        assert_eq!(
            find_task_fuzzy("DOWN", &titles),
            FuzzyMatch::Single("整理 Downloads".to_string()),
        );
        assert_eq!(
            find_task_fuzzy("dOWn", &titles),
            FuzzyMatch::Single("整理 Downloads".to_string()),
        );
    }

    #[test]
    fn fuzzy_ambiguous_returns_all_candidates() {
        let titles = ts(&["整理 Downloads", "整理 Documents", "跑步"]);
        match find_task_fuzzy("整理", &titles) {
            FuzzyMatch::Ambiguous(list) => {
                assert_eq!(list.len(), 2);
                assert!(list.contains(&"整理 Downloads".to_string()));
                assert!(list.contains(&"整理 Documents".to_string()));
            }
            other => panic!("expected Ambiguous, got {:?}", other),
        }
    }

    #[test]
    fn fuzzy_returns_none_when_no_match() {
        let titles = ts(&["整理 Downloads", "跑步"]);
        assert_eq!(find_task_fuzzy("不存在", &titles), FuzzyMatch::None);
    }

    // -------- resolve_index_to_title --------

    #[test]
    fn resolve_index_returns_none_for_non_numeric() {
        let titles = ts(&["a", "b"]);
        assert_eq!(resolve_index_to_title("abc", &titles), None);
        assert_eq!(resolve_index_to_title("1abc", &titles), None);
        assert_eq!(resolve_index_to_title("", &titles), None);
    }

    #[test]
    fn resolve_index_returns_none_for_zero() {
        let titles = ts(&["a", "b"]);
        assert_eq!(resolve_index_to_title("0", &titles), None);
    }

    #[test]
    fn resolve_index_returns_none_for_out_of_range() {
        let titles = ts(&["a", "b"]);
        assert_eq!(resolve_index_to_title("3", &titles), None);
        assert_eq!(resolve_index_to_title("99", &titles), None);
    }

    #[test]
    fn resolve_index_returns_title_for_valid_1_indexed() {
        let titles = ts(&["first", "second", "third"]);
        assert_eq!(resolve_index_to_title("1", &titles), Some("first".to_string()));
        assert_eq!(resolve_index_to_title("2", &titles), Some("second".to_string()));
        assert_eq!(resolve_index_to_title("3", &titles), Some("third".to_string()));
    }

    #[test]
    fn resolve_index_trims_whitespace() {
        let titles = ts(&["a", "b"]);
        assert_eq!(resolve_index_to_title("  2  ", &titles), Some("b".to_string()));
    }

    #[test]
    fn resolve_index_returns_none_for_empty_titles() {
        assert_eq!(resolve_index_to_title("1", &[]), None);
    }

    // -------- suggest_titles / format_no_match --------

    #[test]
    fn suggest_titles_empty_for_empty_query() {
        let titles = ts(&["a", "b"]);
        assert!(suggest_titles("", &titles, 2).is_empty());
        assert!(suggest_titles("   ", &titles, 2).is_empty());
    }

    #[test]
    fn suggest_titles_empty_for_n_zero() {
        let titles = ts(&["abc"]);
        assert!(suggest_titles("a", &titles, 0).is_empty());
    }

    #[test]
    fn suggest_titles_filters_zero_overlap() {
        // query 与 title 字符集毫无交集 → 过滤
        let titles = ts(&["xyz"]);
        assert!(suggest_titles("abc", &titles, 5).is_empty());
    }

    #[test]
    fn suggest_titles_sorts_by_overlap_desc_and_takes_n() {
        // query="ab" → "abcdef" (2 overlap) > "axyz" (1 overlap) > "qrs" (0 → 过滤)
        let titles = ts(&["axyz", "abcdef", "qrs"]);
        let out = suggest_titles("ab", &titles, 2);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], "abcdef"); // higher overlap first
        assert_eq!(out[1], "axyz");
    }

    #[test]
    fn suggest_titles_chinese_overlap_works() {
        let titles = ts(&["整理 Downloads", "整理 Documents", "学习 Rust"]);
        // "整理D" → "整 / 理 / d" 与 "整理 Downloads" / "整理 Documents" 各
        // 共享 "整理 d"（小写）；与 "学习 Rust" 仅共 0 个（无重合）。
        let out = suggest_titles("整理D", &titles, 2);
        assert_eq!(out.len(), 2);
        // 两个"整理 X" 都至少 score > 0，确切 ranking 不强约束（同 score 顺序
        // 不稳，取 set 即可）
        let out_set: std::collections::HashSet<&String> = out.iter().collect();
        assert!(out_set.contains(&"整理 Downloads".to_string()));
        assert!(out_set.contains(&"整理 Documents".to_string()));
    }

    #[test]
    fn format_no_match_falls_back_when_no_suggestions() {
        let s = format_no_match_with_suggestions("foo", &[]);
        assert!(s.contains("找不到任务"));
        assert!(s.contains("「foo」"));
        assert!(!s.contains("你是不是想"));
    }

    #[test]
    fn format_no_match_lists_suggestions_with_bullets() {
        let s = format_no_match_with_suggestions("整理D", &ts(&["整理 Downloads", "整理 Documents"]));
        assert!(s.contains("找不到任务"));
        assert!(s.contains("「整理D」"));
        assert!(s.contains("你是不是想"));
        assert!(s.contains("• 整理 Downloads"));
        assert!(s.contains("• 整理 Documents"));
    }

    #[test]
    fn ambiguous_format_lists_candidates_with_bullets() {
        let candidates = ts(&["A", "B", "C"]);
        let s = format_ambiguous_match("整理", &candidates);
        assert!(s.contains("「整理」"));
        assert!(s.contains("• A"));
        assert!(s.contains("• B"));
        assert!(s.contains("• C"));
        assert!(s.contains("更精确"));
    }

    #[test]
    fn ambiguous_format_truncates_with_ellipsis_when_over_limit() {
        let candidates = ts(&["A", "B", "C", "D", "E", "F", "G"]); // 7 个
        let s = format_ambiguous_match("x", &candidates);
        // 仅前 5 条 bullet
        for ch in &["A", "B", "C", "D", "E"] {
            assert!(s.contains(&format!("• {}", ch)));
        }
        // 第 6/7 条不出现
        assert!(!s.contains("• F"));
        assert!(!s.contains("• G"));
        // 截断提示 "…等 7 条"
        assert!(s.contains("等 7 条"));
    }

    #[test]
    fn format_tasks_no_change_mentions_no_change() {
        let s = format_tasks_no_change();
        assert!(s.contains("📋"));
        assert!(s.contains("没有变化") || s.contains("无变化"));
    }

    #[test]
    fn format_help_text_lists_all_commands_with_descriptions() {
        let s = format_help_text(&[]);
        // 矩阵覆盖：五条命令名都出现（/task 单 + /tasks 复 + /cancel + /retry + /help）
        assert!(s.contains("/tasks"));
        assert!(s.contains("/task "), "expect /task <title> entry: {}", s);
        assert!(s.contains("/cancel"));
        assert!(s.contains("/retry"));
        assert!(s.contains("/help"));
        // 优先级前缀语法应被记录在 help 里，否则用户不知道功能存在
        assert!(s.contains("!!"), "expect prefix syntax in help: {}", s);
        assert!(s.contains("P5"), "expect P5 mention in help: {}", s);
        assert!(s.contains("P7"), "expect P7 mention in help: {}", s);
        // 标题与注脚锚点
        assert!(s.contains("可用命令"));
        // 至少一处中文说明而非纯命令清单（避免回归到全英文 / 纯标识符）
        assert!(s.contains("任务"));
        // 空 custom 时不该出现"自定义命令"段
        assert!(!s.contains("自定义命令"), "empty custom should not render section: {}", s);
    }

    #[test]
    fn format_help_text_renders_custom_commands_section() {
        let custom = vec![
            cc("timer", "设置一个提醒"),
            cc("translate", "翻译为英文"),
        ];
        let s = format_help_text(&custom);
        assert!(s.contains("自定义命令"), "section header missing: {}", s);
        assert!(s.contains("/timer"), "missing custom name: {}", s);
        assert!(s.contains("设置一个提醒"));
        assert!(s.contains("/translate"));
        assert!(s.contains("翻译为英文"));
        // 精简后注脚合到首行副标题（"结果会自动回传"）
        assert!(s.contains("结果会自动回传"));
    }

    #[test]
    fn format_help_text_skips_blank_custom_entries() {
        let custom = vec![
            cc("good", "合法"),
            cc("", "空 name"),
            cc("nodesc", "   "),
        ];
        let s = format_help_text(&custom);
        assert!(s.contains("/good"));
        assert!(!s.contains("/nodesc"), "blank desc must be skipped: {}", s);
        // 空 name 不会出现孤立 `/  —  空 name`
        assert!(!s.contains("空 name"));
    }

    // -------- format_tasks_list --------

    use crate::task_queue::{TaskStatus, TaskView};

    fn view(
        title: &str,
        priority: u8,
        due: Option<&str>,
        status: TaskStatus,
        suffix: Option<&str>,
    ) -> TaskView {
        // 复用 TaskView 的字段：error_message 字段在 Error / Cancelled 下
        // 承担"原因"角色；Done 下 result 承担"产物"角色（与 task_queue
        // 模块的语义一致）。
        let (error_message, result) = match status {
            TaskStatus::Done => (None, suffix.map(String::from)),
            TaskStatus::Error | TaskStatus::Cancelled => (suffix.map(String::from), None),
            TaskStatus::Pending => (None, None),
        };
        TaskView {
            title: title.to_string(),
            body: String::new(),
            raw_description: String::new(),
            priority,
            due: due.map(String::from),
            status,
            error_message,
            tags: Vec::new(),
            result,
            created_at: "2026-05-04T13:00:00+08:00".to_string(),
            updated_at: "2026-05-04T13:00:00+08:00".to_string(),
            detail_path: String::new(),
            blocked_by: Vec::new(),
            snoozed_until: None,
            pinned: false,
        }
    }

    #[test]
    fn empty_list_returns_friendly_prompt() {
        let s = format_tasks_list(&[]);
        assert!(s.contains("空"));
        assert!(s.contains("📋"));
        // 空列表不应有"进行中"等分组标题
        assert!(!s.contains("进行中"));
    }

    #[test]
    fn renders_total_count_in_header() {
        let tasks = vec![
            view("a", 0, None, TaskStatus::Pending, None),
            view("b", 0, None, TaskStatus::Done, None),
        ];
        let s = format_tasks_list(&tasks);
        assert!(s.contains("共 2 条"));
    }

    #[test]
    fn pending_section_uses_hourglass_emoji_and_due() {
        let tasks = vec![view(
            "整理 Downloads",
            3,
            Some("2026-05-05T18:00"),
            TaskStatus::Pending,
            None,
        )];
        let s = format_tasks_list(&tasks);
        assert!(s.contains("进行中（1）"));
        assert!(s.contains("⏳"));
        assert!(s.contains("P3"));
        assert!(s.contains("整理 Downloads"));
        assert!(s.contains("截至 5/5 18:00"));
    }

    #[test]
    fn pending_without_due_omits_suffix() {
        let tasks = vec![view("喝水", 1, None, TaskStatus::Pending, None)];
        let s = format_tasks_list(&tasks);
        // 应有标题但不带 ` — `
        assert!(s.contains("喝水"));
        assert!(!s.contains("喝水 — "));
    }

    #[test]
    fn priority_zero_omits_prefix() {
        let tasks = vec![view("x", 0, None, TaskStatus::Pending, None)];
        let s = format_tasks_list(&tasks);
        assert!(!s.contains("P0"));
    }

    #[test]
    fn done_section_renders_result_when_present() {
        let tasks = vec![view(
            "写周报",
            0,
            None,
            TaskStatus::Done,
            Some("生成 weekly_summary"),
        )];
        let s = format_tasks_list(&tasks);
        assert!(s.contains("已完成（1）"));
        assert!(s.contains("✅"));
        assert!(s.contains("生成 weekly_summary"));
    }

    #[test]
    fn error_section_renders_message() {
        let tasks = vec![view("跑步", 2, None, TaskStatus::Error, Some("下雨了"))];
        let s = format_tasks_list(&tasks);
        assert!(s.contains("已失败（1）"));
        assert!(s.contains("⚠️"));
        assert!(s.contains("下雨了"));
    }

    #[test]
    fn cancelled_section_renders_reason() {
        let tasks = vec![view(
            "学习 Rust",
            0,
            None,
            TaskStatus::Cancelled,
            Some("改主意了"),
        )];
        let s = format_tasks_list(&tasks);
        assert!(s.contains("已取消（1）"));
        assert!(s.contains("🚫"));
        assert!(s.contains("改主意了"));
    }

    #[test]
    fn empty_sections_are_omitted() {
        // 只有 pending — 不应该出现 "已完成（0）" 之类
        let tasks = vec![view("a", 0, None, TaskStatus::Pending, None)];
        let s = format_tasks_list(&tasks);
        assert!(!s.contains("已完成"));
        assert!(!s.contains("已失败"));
        assert!(!s.contains("已取消"));
    }

    #[test]
    fn sections_appear_in_canonical_order() {
        // 进行中 → 已完成 → 已失败 → 已取消
        let tasks = vec![
            view("can", 0, None, TaskStatus::Cancelled, Some("c")),
            view("err", 0, None, TaskStatus::Error, Some("e")),
            view("don", 0, None, TaskStatus::Done, Some("d")),
            view("pen", 0, None, TaskStatus::Pending, None),
        ];
        let s = format_tasks_list(&tasks);
        let idx_pending = s.find("进行中").unwrap();
        let idx_done = s.find("已完成").unwrap();
        let idx_error = s.find("已失败").unwrap();
        let idx_cancelled = s.find("已取消").unwrap();
        assert!(idx_pending < idx_done);
        assert!(idx_done < idx_error);
        assert!(idx_error < idx_cancelled);
    }

    #[test]
    fn long_suffix_is_truncated_with_ellipsis() {
        // 41 个字符的 result（大于 40 的 char-based 上限）应被截断 + …
        let long = "啊".repeat(50);
        let tasks = vec![view("x", 0, None, TaskStatus::Done, Some(long.as_str()))];
        let s = format_tasks_list(&tasks);
        assert!(s.contains("…"));
        // 渲染后的结果整体包含原文的 40 char 前缀（这里一字一码点，截断
        // 后保留前 40 个）但不含全部 50 个
        assert!(!s.contains(&long));
    }

    #[test]
    fn short_suffix_not_truncated() {
        let tasks = vec![view("x", 0, None, TaskStatus::Done, Some("简短产物"))];
        let s = format_tasks_list(&tasks);
        assert!(s.contains("简短产物"));
        // 不该被误加省略号
        assert!(!s.contains("简短产物…"));
    }

    // -------- tg_command_registry (setMyCommands payload) --------

    #[test]
    fn tg_command_registry_covers_all_user_facing_commands() {
        let names: Vec<&str> = tg_command_registry()
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        // 与 parse_tg_command 接受的命令矩阵对齐。Unknown / "/" 等不算用户命令。
        // 新加 TG 命令时务必同步两处：registry（让 TG slash autocomplete 浮）
        // + 本断言（让"忘加"被测试拦下）。历史上 /whoami / /snooze / /unsnooze
        // 实现但漏注册了几轮才补；本测试就是把这种 silent gap 钉死。
        for expected in [
            "task", "tasks", "cancel", "retry", "done", "stats", "mood",
            "whoami", "snooze", "unsnooze", "pin", "unpin", "pinned", "today",
            "due", "edit", "reflect", "show", "reset", "version", "help",
        ] {
            assert!(
                names.contains(&expected),
                "registry missing user-facing command `{}`",
                expected,
            );
        }
    }

    #[test]
    fn tg_command_registry_orders_task_first_help_last() {
        // 顺序就是用户输 `/` 时看到的顺序：高频创建在前、兜底 help 在末
        let names: Vec<&str> = tg_command_registry()
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(names.first(), Some(&"task"));
        assert_eq!(names.last(), Some(&"help"));
    }

    #[test]
    fn tg_command_registry_descriptions_within_telegram_limit() {
        // Telegram setMyCommands 限制 description ≤ 256 字符，name ≤ 32
        // & lowercase ASCII。回归保护：往清单加项时不要超长 / 写错大小写。
        for (name, desc) in tg_command_registry() {
            assert!(!name.is_empty(), "command name must not be empty");
            assert!(name.len() <= 32, "name too long: {}", name);
            assert!(
                name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "name must be lowercase ASCII / digit / underscore: {}",
                name,
            );
            assert!(!desc.is_empty(), "description must not be empty: {}", name);
            assert!(desc.chars().count() <= 256, "description too long: {}", name);
        }
    }

    // -------- merged_command_registry --------

    fn cc(name: &str, desc: &str) -> crate::commands::settings::TgCustomCommand {
        crate::commands::settings::TgCustomCommand {
            name: name.to_string(),
            description: desc.to_string(),
        }
    }

    #[test]
    fn merged_with_empty_custom_equals_hardcoded() {
        let merged = merged_command_registry(&[], "");
        let hardcoded = tg_command_registry();
        assert_eq!(merged.len(), hardcoded.len());
        for (m, h) in merged.iter().zip(hardcoded.iter()) {
            assert_eq!(m.0, h.0);
            assert_eq!(m.1, h.1);
        }
    }

    #[test]
    fn merged_appends_valid_custom_after_hardcoded() {
        let merged = merged_command_registry(&[cc("timer", "设置一个提醒")], "");
        let hardcoded = tg_command_registry();
        assert_eq!(merged.len(), hardcoded.len() + 1);
        assert_eq!(merged.last().unwrap().0, "timer");
        assert_eq!(merged.last().unwrap().1, "设置一个提醒");
    }

    #[test]
    fn merged_drops_invalid_custom_silently() {
        let custom = vec![
            cc("", "空 name"),
            cc("Tasks", "name 撞 hardcoded（大小写无关? 实际严格 lowercase 比较，但 Tasks 含大写直接非法）"),
            cc("tasks", "重名 hardcoded"),
            cc("bad name", "name 含空格"),
            cc("good", ""),
            cc("good", "   "),
            cc("超长名超长名超长名超长名超长名超长名超长名超长名超长名超长名超长名超长名超长名超长名超长名超长名超长", "描述"),
            cc("legit", "合法的"),
        ];
        let merged = merged_command_registry(&custom, "");
        let hardcoded = tg_command_registry();
        assert_eq!(merged.len(), hardcoded.len() + 1, "only `legit` should pass");
        assert_eq!(merged.last().unwrap().0, "legit");
    }

    #[test]
    fn merged_dedupes_same_name_in_custom() {
        let merged = merged_command_registry(
            &[
                cc("alpha", "first"),
                cc("alpha", "second"),
                cc("beta", "third"),
            ],
            "",
        );
        let hardcoded = tg_command_registry();
        assert_eq!(merged.len(), hardcoded.len() + 2);
        let custom_only: Vec<&(String, String)> =
            merged.iter().skip(hardcoded.len()).collect();
        assert_eq!(custom_only[0].0, "alpha");
        assert_eq!(custom_only[0].1, "first", "first occurrence wins");
        assert_eq!(custom_only[1].0, "beta");
    }

    #[test]
    fn merged_drops_description_over_256_chars() {
        let long_desc = "x".repeat(257);
        let merged = merged_command_registry(&[cc("foo", &long_desc)], "");
        let hardcoded = tg_command_registry();
        assert_eq!(merged.len(), hardcoded.len(), "over-256 desc should be dropped");
    }

    // -------- parse_allowed_usernames --------

    #[test]
    fn parse_allowed_usernames_single() {
        assert_eq!(parse_allowed_usernames("alice"), vec!["alice"]);
    }

    #[test]
    fn parse_allowed_usernames_comma_separated() {
        assert_eq!(
            parse_allowed_usernames("alice, bob, carol"),
            vec!["alice", "bob", "carol"]
        );
    }

    #[test]
    fn parse_allowed_usernames_strips_at_prefix_and_lowercases() {
        assert_eq!(
            parse_allowed_usernames("@Alice, @BOB"),
            vec!["alice", "bob"]
        );
    }

    #[test]
    fn parse_allowed_usernames_skips_blank_segments() {
        assert_eq!(parse_allowed_usernames("alice,,bob"), vec!["alice", "bob"]);
        assert_eq!(parse_allowed_usernames(",alice,"), vec!["alice"]);
        assert_eq!(parse_allowed_usernames(" , , "), Vec::<String>::new());
    }

    #[test]
    fn parse_allowed_usernames_dedupes() {
        // 同名去重，case-insensitive 通过 lowercase 自然落到同条
        assert_eq!(
            parse_allowed_usernames("alice, Alice, alice"),
            vec!["alice"]
        );
    }

    #[test]
    fn parse_allowed_usernames_empty_input() {
        assert!(parse_allowed_usernames("").is_empty());
        assert!(parse_allowed_usernames("   ").is_empty());
    }

    // -------- tg_command_registry_localized --------

    #[test]
    fn registry_localized_zh_returns_chinese() {
        let r = tg_command_registry_localized("zh");
        let task_desc = r.iter().find(|(n, _)| *n == "task").unwrap().1;
        assert!(task_desc.contains("队列"), "zh task desc should be Chinese: {}", task_desc);
    }

    #[test]
    fn registry_localized_en_returns_english() {
        let r = tg_command_registry_localized("en");
        let task_desc = r.iter().find(|(n, _)| *n == "task").unwrap().1;
        assert!(task_desc.contains("Queue"), "en task desc should be English: {}", task_desc);
        let cancel_desc = r.iter().find(|(n, _)| *n == "cancel").unwrap().1;
        assert!(cancel_desc.contains("Cancel"));
    }

    #[test]
    fn registry_localized_unknown_falls_back_to_zh() {
        // Defensive default：陌生 lang 不让 bot 起不来，兜底中文
        let r = tg_command_registry_localized("klingon");
        let task_desc = r.iter().find(|(n, _)| *n == "task").unwrap().1;
        assert!(task_desc.contains("队列"));
    }

    #[test]
    fn registry_localized_is_case_insensitive() {
        let r = tg_command_registry_localized("EN");
        let task_desc = r.iter().find(|(n, _)| *n == "task").unwrap().1;
        assert!(task_desc.contains("Queue"));
    }

    #[test]
    fn merged_passes_lang_to_hardcoded_section() {
        // custom 不翻译，hardcoded 段跟 lang
        let custom = vec![cc("timer", "中文描述（不翻译）")];
        let merged_en = merged_command_registry(&custom, "en");
        let task_in_en = merged_en.iter().find(|(n, _)| n == "task").unwrap();
        assert!(task_in_en.1.contains("Queue"));
        let timer_in_en = merged_en.iter().find(|(n, _)| n == "timer").unwrap();
        assert!(timer_in_en.1.contains("中文描述"), "custom should not be translated");
    }

    // -------- /stats parse + format --------

    #[test]
    fn parses_stats() {
        let p = parse_tg_command("/stats");
        assert_eq!(p, Some(TgCommand::Stats));
    }

    #[test]
    fn parses_stats_ignores_trailing_args() {
        // 与 /tasks /help 同模式：尾部 token 全忽略，保持前向兼容
        let p = parse_tg_command("/stats since:7d");
        assert_eq!(p, Some(TgCommand::Stats));
    }

    #[test]
    fn stats_reply_all_zero_shows_quiet_marker() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 14)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let s = format_stats_reply(&[], now, now.date());
        assert!(s.contains("📊 任务状态"));
        assert!(s.contains("今日很安静"));
        assert!(s.contains("待办：0"));
    }

    #[test]
    fn stats_reply_counts_pending_overdue_done_today() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 14)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let today_iso = "2026-05-14T11:30:00+08:00";
        let earlier_iso = "2026-05-13T11:30:00+08:00";
        // 一个过期 pending（due 在 now 之前）
        let mut overdue_pending = view(
            "整理 Downloads",
            3,
            Some("2026-05-13T10:00"),
            TaskStatus::Pending,
            None,
        );
        overdue_pending.updated_at = today_iso.to_string();
        // 一个未过期 pending（due 在 now 之后）
        let mut fresh_pending = view(
            "写周报",
            3,
            Some("2026-05-20T18:00"),
            TaskStatus::Pending,
            None,
        );
        fresh_pending.updated_at = today_iso.to_string();
        // 一个今日完成
        let mut done_today = view("跑步", 0, None, TaskStatus::Done, Some("5km"));
        done_today.updated_at = today_iso.to_string();
        // 一个昨日完成（不计今日）
        let mut done_yesterday = view("洗碗", 0, None, TaskStatus::Done, None);
        done_yesterday.updated_at = earlier_iso.to_string();
        // 一个 error（不限今日）
        let error_task = view("跑步失败", 0, None, TaskStatus::Error, Some("天气"));
        // 一个今日取消
        let mut cancelled_today = view("学 Rust", 0, None, TaskStatus::Cancelled, Some("改主意"));
        cancelled_today.updated_at = today_iso.to_string();
        let views = vec![
            overdue_pending,
            fresh_pending,
            done_today,
            done_yesterday,
            error_task,
            cancelled_today,
        ];
        let s = format_stats_reply(&views, now, now.date());
        assert!(s.contains("待办：2"), "stats reply: {s}");
        assert!(s.contains("逾期：1"), "stats reply: {s}");
        assert!(s.contains("今日完成：1"), "stats reply: {s}");
        assert!(s.contains("出错：1"), "stats reply: {s}");
        assert!(s.contains("今日取消：1"), "stats reply: {s}");
        assert!(!s.contains("今日很安静"));
    }

    // -------- /mood parse + format --------

    #[test]
    fn parses_mood() {
        assert_eq!(parse_tg_command("/mood"), Some(TgCommand::Mood));
    }

    #[test]
    fn parses_mood_ignores_trailing_args() {
        assert_eq!(parse_tg_command("/mood now?"), Some(TgCommand::Mood));
    }

    #[test]
    fn mood_reply_none_shows_friendly_empty() {
        let s = format_mood_reply(None);
        assert!(s.contains("还没记心情"), "mood reply: {s}");
    }

    #[test]
    fn mood_reply_with_motion_shows_two_lines() {
        let s = format_mood_reply(Some(("有点兴奋".to_string(), Some("happy_idle".to_string()))));
        assert!(s.contains("心情：有点兴奋"), "mood reply: {s}");
        assert!(s.contains("动作组：happy_idle"), "mood reply: {s}");
    }

    #[test]
    fn mood_reply_without_motion_skips_action_line() {
        let s = format_mood_reply(Some(("默默坐着".to_string(), None)));
        assert!(s.contains("心情：默默坐着"), "mood reply: {s}");
        assert!(!s.contains("动作组"), "mood reply: {s}");
    }

    #[test]
    fn mood_reply_empty_text_keeps_marker() {
        let s = format_mood_reply(Some((String::new(), None)));
        assert!(s.contains("（无文字）"), "mood reply: {s}");
    }

    // -------- /whoami parse + format --------

    #[test]
    fn parses_whoami() {
        assert_eq!(parse_tg_command("/whoami"), Some(TgCommand::Whoami));
    }

    #[test]
    fn parses_whoami_ignores_trailing() {
        assert_eq!(
            parse_tg_command("/whoami please"),
            Some(TgCommand::Whoami),
        );
    }

    #[test]
    fn whoami_reply_full_signal_renders_all_lines() {
        let s = format_whoami_reply(
            "Moon",
            Some(14),
            Some(("阳光特别足".to_string(), Some("happy".to_string()))),
            "观察 Moon 在上午写代码、下午开会的节奏。",
            &[
                ("shell".to_string(), 12),
                ("read_file".to_string(), 7),
                ("weather".to_string(), 3),
            ],
        );
        assert!(s.contains("我叫你「Moon」"), "{s}");
        assert!(s.contains("相伴已 14 天"), "{s}");
        assert!(s.contains("现在的心情：阳光特别足"), "{s}");
        assert!(s.contains("动作组 happy"), "{s}");
        assert!(s.contains("自我画像"), "{s}");
        assert!(s.contains("`shell`×12"), "{s}");
        assert!(s.contains("`read_file`×7"), "{s}");
        assert!(s.contains("`weather`×3"), "{s}");
    }

    #[test]
    fn whoami_reply_zero_days_says_today() {
        let s = format_whoami_reply("M", Some(0), None, "", &[]);
        assert!(s.contains("今天与你初识"), "{s}");
        // 没心情 / 自我画像 / 工具 → 不渲染这些行
        assert!(!s.contains("现在的心情"));
        assert!(!s.contains("自我画像"));
        assert!(!s.contains("近常用工具"));
    }

    #[test]
    fn whoami_reply_skips_missing_sources() {
        // 用户名空 → 不渲染该行；心情 raw text 空 → 不渲染；其它源 None → 不渲染
        let s = format_whoami_reply(
            "",
            Some(3),
            Some((String::new(), Some("happy".to_string()))),
            "",
            &[],
        );
        assert!(!s.contains("我叫你"));
        assert!(!s.contains("现在的心情"));
        assert!(!s.contains("自我画像"));
        assert!(s.contains("相伴已 3 天"));
    }

    #[test]
    fn whoami_reply_all_empty_falls_back_to_friendly_line() {
        let s = format_whoami_reply("", None, None, "", &[]);
        assert!(s.contains("还没攒到自我介绍的素材"), "{s}");
    }

    #[test]
    fn whoami_reply_truncates_long_persona_summary() {
        // 100 字符的 ASCII 字符串：> 90 → 应被截断 + 加省略号。
        let long = "abcdefghij".repeat(10);
        let s = format_whoami_reply("", None, None, &long, &[]);
        assert!(s.contains("…"), "long persona should be truncated: {s}");
    }

    // -------- mood_emoji_for + whoami header prefix --------

    #[test]
    fn mood_emoji_maps_chinese_keywords() {
        assert_eq!(mood_emoji_for("今天特别开心"), "😊");
        assert_eq!(mood_emoji_for("有点难过"), "😢");
        assert_eq!(mood_emoji_for("好困啊"), "😴");
        assert_eq!(mood_emoji_for("非常好奇这个问题"), "🤔");
        assert_eq!(mood_emoji_for("感觉很平静"), "😌");
    }

    #[test]
    fn mood_emoji_maps_english_keywords_case_insensitive() {
        assert_eq!(mood_emoji_for("Feeling HAPPY today"), "😊");
        assert_eq!(mood_emoji_for("So Excited!!"), "🤩");
        assert_eq!(mood_emoji_for("kinda Tired"), "😴");
        assert_eq!(mood_emoji_for("a bit ANGRY"), "😠");
    }

    #[test]
    fn mood_emoji_falls_back_to_paw_when_unknown() {
        assert_eq!(mood_emoji_for(""), "🐾");
        assert_eq!(mood_emoji_for("blah blah unrelated"), "🐾");
    }

    #[test]
    fn whoami_header_includes_mood_emoji_prefix_when_mood_present() {
        let s = format_whoami_reply(
            "M",
            None,
            Some(("今天特别开心".to_string(), None)),
            "",
            &[],
        );
        // 第一行应该带 😊 emoji 前缀
        let first_line = s.lines().next().expect("has first line");
        assert!(first_line.contains("😊"), "header should prefix mood emoji: {first_line}");
        assert!(first_line.contains("🪪 /whoami"), "should retain whoami label: {first_line}");
    }

    #[test]
    fn whoami_header_uses_paw_fallback_for_unknown_mood() {
        let s = format_whoami_reply(
            "M",
            None,
            Some(("一种说不清的状态".to_string(), None)),
            "",
            &[],
        );
        let first_line = s.lines().next().expect("has first line");
        assert!(
            first_line.contains("🐾"),
            "unknown mood text should fall back to 🐾: {first_line}"
        );
    }

    #[test]
    fn whoami_header_plain_when_no_mood() {
        let s = format_whoami_reply("M", Some(3), None, "", &[]);
        let first_line = s.lines().next().expect("has first line");
        // 没 mood → 头部不该混入任何 mood emoji，保持原 plain "🪪 /whoami"
        assert_eq!(first_line, "🪪 /whoami");
    }

    #[test]
    fn whoami_reply_persona_first_paragraph_only() {
        let multi = "第一段内容，简短一句。\n\n第二段不该出现。\n\n第三段更不该。";
        let s = format_whoami_reply("", None, None, multi, &[]);
        assert!(s.contains("第一段内容"), "{s}");
        assert!(!s.contains("第二段"), "should drop after first blank line: {s}");
    }

    // -------- /snooze parse + token + compute --------

    fn ndt2(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
    }

    #[test]
    fn parses_snooze_with_preset_token() {
        let cmd = parse_tg_command("/snooze 倒垃圾 tomorrow");
        assert_eq!(
            cmd,
            Some(TgCommand::Snooze {
                title: "倒垃圾".to_string(),
                token: "tomorrow".to_string(),
            }),
        );
    }

    #[test]
    fn parses_snooze_no_preset_token() {
        // 末尾不是已知 preset → 全 arg 当 title，token 空
        let cmd = parse_tg_command("/snooze 倒垃圾 with whitespace");
        assert_eq!(
            cmd,
            Some(TgCommand::Snooze {
                title: "倒垃圾 with whitespace".to_string(),
                token: "".to_string(),
            }),
        );
    }

    #[test]
    fn parses_snooze_single_word_arg_is_title_not_preset() {
        // 单 token 即便是 "30m" 也按 title 处理 —— 没 title 的命令报错语义比
        // "preset 没绑定 task" 更直接（用户漏了 title）。
        let cmd = parse_tg_command("/snooze 30m");
        assert_eq!(
            cmd,
            Some(TgCommand::Snooze {
                title: "30m".to_string(),
                token: "".to_string(),
            }),
        );
    }

    #[test]
    fn parses_snooze_cjk_preset() {
        let cmd = parse_tg_command("/snooze 倒垃圾 今晚");
        assert_eq!(
            cmd,
            Some(TgCommand::Snooze {
                title: "倒垃圾".to_string(),
                token: "今晚".to_string(),
            }),
        );
        let cmd2 = parse_tg_command("/snooze 整理桌面 明早");
        assert_eq!(
            cmd2,
            Some(TgCommand::Snooze {
                title: "整理桌面".to_string(),
                token: "明早".to_string(),
            }),
        );
    }

    #[test]
    fn parses_snooze_minutes_form() {
        let cmd = parse_tg_command("/snooze 倒垃圾 45m");
        assert_eq!(
            cmd,
            Some(TgCommand::Snooze {
                title: "倒垃圾".to_string(),
                token: "45m".to_string(),
            }),
        );
    }

    #[test]
    fn parses_unsnooze() {
        let cmd = parse_tg_command("/unsnooze 倒垃圾");
        assert_eq!(
            cmd,
            Some(TgCommand::Unsnooze { title: "倒垃圾".to_string() }),
        );
    }

    #[test]
    fn parses_pin_unpin() {
        // 全 arg 当 title（无 preset 解析），含多 token 也合法。
        assert_eq!(
            parse_tg_command("/pin 整理 Downloads"),
            Some(TgCommand::Pin { title: "整理 Downloads".to_string() }),
        );
        assert_eq!(
            parse_tg_command("/unpin 周报"),
            Some(TgCommand::Unpin { title: "周报".to_string() }),
        );
    }

    #[test]
    fn parses_pin_unpin_empty_title_yields_command_with_empty() {
        // 空 title 由 bot handler 走 missing-argument 反馈（与 done / snooze 同
        // 路径），parser 层不做特殊化。
        assert_eq!(
            parse_tg_command("/pin"),
            Some(TgCommand::Pin { title: "".to_string() }),
        );
        assert_eq!(
            parse_tg_command("/unpin"),
            Some(TgCommand::Unpin { title: "".to_string() }),
        );
    }

    #[test]
    fn parses_silent_unsilent() {
        // 与 /pin /unpin 同模板：全 arg 当 title，含多 token 也合法。
        assert_eq!(
            parse_tg_command("/silent 整理 Downloads"),
            Some(TgCommand::Silent { title: "整理 Downloads".to_string() }),
        );
        assert_eq!(
            parse_tg_command("/unsilent 周报"),
            Some(TgCommand::Unsilent { title: "周报".to_string() }),
        );
        // 大小写不敏感
        assert_eq!(
            parse_tg_command("/SILENT foo"),
            Some(TgCommand::Silent { title: "foo".to_string() }),
        );
    }

    #[test]
    fn parses_silent_unsilent_empty_title() {
        // 空 title 走 missing-argument 反馈（与 /pin 同路径）
        assert_eq!(
            parse_tg_command("/silent"),
            Some(TgCommand::Silent { title: "".to_string() }),
        );
        assert_eq!(
            parse_tg_command("/unsilent"),
            Some(TgCommand::Unsilent { title: "".to_string() }),
        );
    }

    #[test]
    fn parses_pinned() {
        // 无参；多余尾部一律忽略（与 /tasks 同容忍策略，让 "/pinned all" 也能命中）
        assert_eq!(parse_tg_command("/pinned"), Some(TgCommand::Pinned));
        assert_eq!(parse_tg_command("/PINNED"), Some(TgCommand::Pinned));
        assert_eq!(parse_tg_command("/pinned now?"), Some(TgCommand::Pinned));
    }

    #[test]
    fn parses_silenced() {
        // 与 /pinned 同模板：无参，大小写不敏感，尾部尾巴忽略
        assert_eq!(parse_tg_command("/silenced"), Some(TgCommand::Silenced));
        assert_eq!(parse_tg_command("/SILENCED"), Some(TgCommand::Silenced));
        assert_eq!(parse_tg_command("/silenced all"), Some(TgCommand::Silenced));
    }

    #[test]
    fn parses_markers() {
        assert_eq!(parse_tg_command("/markers"), Some(TgCommand::Markers));
        assert_eq!(parse_tg_command("/MARKERS"), Some(TgCommand::Markers));
        assert_eq!(parse_tg_command("/markers all"), Some(TgCommand::Markers));
    }

    #[test]
    fn format_markers_list_empty_teaches_both_commands() {
        let s = format_markers_list(&[]);
        assert!(s.contains("/pin"), "should teach /pin: {s}");
        assert!(s.contains("/silent"), "should teach /silent: {s}");
        assert!(
            s.contains("无") || s.contains("none") || s.contains("暂无"),
            "should signal empty: {s}",
        );
    }

    #[test]
    fn format_markers_list_separates_pinned_and_silent_sections() {
        let pinned = crate::task_queue::TaskView {
            title: "Pin-only".to_string(),
            body: "".to_string(),
            raw_description: "Pin-only".to_string(),
            priority: 3,
            due: None,
            status: crate::task_queue::TaskStatus::Pending,
            error_message: None,
            tags: vec![],
            result: None,
            created_at: "2026-05-16T09:00:00+08:00".to_string(),
            updated_at: "2026-05-16T09:00:00+08:00".to_string(),
            detail_path: "".to_string(),
            blocked_by: vec![],
            snoozed_until: None,
            pinned: true,
        };
        let silent = crate::task_queue::TaskView {
            title: "Silent-only".to_string(),
            raw_description: "Silent-only [silent]".to_string(),
            pinned: false,
            ..pinned.clone()
        };
        let both = crate::task_queue::TaskView {
            title: "Both".to_string(),
            raw_description: "Both [silent]".to_string(),
            pinned: true,
            ..pinned.clone()
        };
        let s = format_markers_list(&[pinned, silent, both]);
        // header counts
        assert!(s.contains("📌 2 钉 / 🔇 2 静"), "header should show counts: {s}");
        // sections
        assert!(s.contains("📌 钉住（2）"));
        assert!(s.contains("🔇 静默（2）"));
        // task lines in both sections (Both appears in both)
        assert!(s.contains("Pin-only"));
        assert!(s.contains("Silent-only"));
        assert_eq!(
            s.matches("Both").count(),
            2,
            "Both 应在 pinned + silent 两段各出现一次: {s}"
        );
    }

    #[test]
    fn format_silenced_tasks_list_empty_teaches_silent_command() {
        // 0 命中：友好提示 + 教学
        let s = format_silenced_tasks_list(&[]);
        assert!(s.contains("🔇"), "should keep silent emoji in header: {s}");
        assert!(s.contains("/silent"), "should teach `/silent` syntax: {s}");
        assert!(s.contains("桌面") || s.contains("右键"), "should mention desktop entry: {s}");
    }

    #[test]
    fn format_silenced_tasks_list_sections_show_per_status() {
        // 简单 smoke：含至少一条任务时 header 有 "共 N 条"，content 出现 emoji
        let pending = crate::task_queue::TaskView {
            title: "X".to_string(),
            body: "".to_string(),
            raw_description: "X [silent]".to_string(),
            priority: 3,
            due: None,
            status: crate::task_queue::TaskStatus::Pending,
            error_message: None,
            tags: vec![],
            result: None,
            created_at: "2026-05-16T09:00:00+08:00".to_string(),
            updated_at: "2026-05-16T09:00:00+08:00".to_string(),
            detail_path: "".to_string(),
            blocked_by: vec![],
            snoozed_until: None,
            pinned: false,
        };
        let s = format_silenced_tasks_list(&[pending]);
        assert!(s.contains("🔇"), "should have silent emoji header: {s}");
        assert!(s.contains("共 1 条"), "should show count: {s}");
        assert!(s.contains("进行中"), "should have status section: {s}");
    }

    #[test]
    fn format_pinned_tasks_list_empty_teaches_pin_command() {
        // 0 命中：友好提示 + 教学（与 /tasks 空集合 "📋 你的任务清单是空的" 思路同）
        let s = format_pinned_tasks_list(&[]);
        assert!(s.contains("📌"), "should keep pin emoji in header: {s}");
        assert!(s.contains("/pin"), "should teach `/pin` syntax: {s}");
        assert!(s.contains("桌面") || s.contains("右键"), "should mention desktop entry: {s}");
    }

    #[test]
    fn format_pinned_tasks_list_groups_by_status_and_counts() {
        // 三条混合：pending + done + cancelled。header 总数 3；section
        // 各自报 (1) 计数；每条 title 出现一次。
        let v_pending = view("活的", 3, None, TaskStatus::Pending, None);
        let v_done = view("做完了", 3, None, TaskStatus::Done, Some("产物 X"));
        let v_cancelled = view("不做了", 3, None, TaskStatus::Cancelled, Some("没意义"));
        let s = format_pinned_tasks_list(&[v_pending, v_done, v_cancelled]);
        assert!(s.contains("📌 当前钉住任务（共 3 条）"), "header: {s}");
        assert!(s.contains("进行中（1）"), "pending section: {s}");
        assert!(s.contains("已完成（1）"), "done section: {s}");
        assert!(s.contains("已取消（1）"), "cancelled section: {s}");
        assert!(s.contains("活的"));
        assert!(s.contains("做完了"));
        assert!(s.contains("不做了"));
    }

    #[test]
    fn parse_snooze_token_keywords() {
        assert_eq!(parse_snooze_token("tonight"), Some(SnoozeSpec::Tonight));
        assert_eq!(parse_snooze_token("Tomorrow"), Some(SnoozeSpec::Tomorrow));
        assert_eq!(parse_snooze_token("MONDAY"), Some(SnoozeSpec::Monday));
    }

    #[test]
    fn parse_snooze_token_minutes_hours() {
        assert_eq!(parse_snooze_token("30m"), Some(SnoozeSpec::Minutes(30)));
        assert_eq!(parse_snooze_token("2h"), Some(SnoozeSpec::Hours(2)));
        assert_eq!(parse_snooze_token("1h"), Some(SnoozeSpec::Hours(1)));
    }

    #[test]
    fn parse_snooze_token_rejects_invalid() {
        assert_eq!(parse_snooze_token(""), None);
        assert_eq!(parse_snooze_token("0m"), None, "0 分无意义");
        assert_eq!(parse_snooze_token("0h"), None);
        assert_eq!(parse_snooze_token("99y"), None, "未知后缀");
        assert_eq!(parse_snooze_token("xm"), None, "非数字");
        // 超 7 天上限
        assert_eq!(parse_snooze_token("99999m"), None);
        assert_eq!(parse_snooze_token("200h"), None);
    }

    #[test]
    fn parse_snooze_token_cjk_keywords() {
        assert_eq!(parse_snooze_token("今晚"), Some(SnoozeSpec::Tonight));
        assert_eq!(parse_snooze_token("明早"), Some(SnoozeSpec::Tomorrow));
        assert_eq!(parse_snooze_token("明天"), Some(SnoozeSpec::Tomorrow));
        assert_eq!(parse_snooze_token("明日"), Some(SnoozeSpec::Tomorrow));
        assert_eq!(parse_snooze_token("周一"), Some(SnoozeSpec::Monday));
        assert_eq!(parse_snooze_token("下周一"), Some(SnoozeSpec::Monday));
        assert_eq!(parse_snooze_token("下周1"), Some(SnoozeSpec::Monday));
    }

    #[test]
    fn parse_snooze_token_cjk_durations() {
        assert_eq!(parse_snooze_token("30分"), Some(SnoozeSpec::Minutes(30)));
        assert_eq!(parse_snooze_token("90分"), Some(SnoozeSpec::Minutes(90)));
        assert_eq!(parse_snooze_token("2小时"), Some(SnoozeSpec::Hours(2)));
        assert_eq!(parse_snooze_token("1小时"), Some(SnoozeSpec::Hours(1)));
        // 空白宽容：30 分 / 2 小时 同等 OK（与中文打字习惯一致）
        assert_eq!(parse_snooze_token("30 分"), Some(SnoozeSpec::Minutes(30)));
        assert_eq!(parse_snooze_token("2 小时"), Some(SnoozeSpec::Hours(2)));
    }

    #[test]
    fn parse_snooze_token_cjk_rejects_overflow() {
        assert_eq!(parse_snooze_token("0分"), None, "0 分无意义");
        assert_eq!(parse_snooze_token("99999分"), None, "超 7 天");
        assert_eq!(parse_snooze_token("200小时"), None);
        assert_eq!(parse_snooze_token("后天"), None, "未实现的关键词");
    }

    #[test]
    fn compute_snooze_until_minutes() {
        let now = ndt2(2026, 5, 14, 12, 0);
        let until = compute_snooze_until(SnoozeSpec::Minutes(30), now);
        assert_eq!(until, ndt2(2026, 5, 14, 12, 30));
    }

    #[test]
    fn compute_snooze_until_hours() {
        let now = ndt2(2026, 5, 14, 12, 0);
        let until = compute_snooze_until(SnoozeSpec::Hours(2), now);
        assert_eq!(until, ndt2(2026, 5, 14, 14, 0));
    }

    #[test]
    fn compute_snooze_until_tonight_before_6pm() {
        let now = ndt2(2026, 5, 14, 12, 0);
        let until = compute_snooze_until(SnoozeSpec::Tonight, now);
        assert_eq!(until, ndt2(2026, 5, 14, 18, 0), "今天还没到 18:00");
    }

    #[test]
    fn compute_snooze_until_tonight_after_6pm_jumps_tomorrow() {
        let now = ndt2(2026, 5, 14, 22, 0);
        let until = compute_snooze_until(SnoozeSpec::Tonight, now);
        assert_eq!(until, ndt2(2026, 5, 15, 18, 0), "已过 18:00 跳明晚");
    }

    #[test]
    fn compute_snooze_until_tomorrow() {
        let now = ndt2(2026, 5, 14, 12, 0);
        let until = compute_snooze_until(SnoozeSpec::Tomorrow, now);
        assert_eq!(until, ndt2(2026, 5, 15, 9, 0));
    }

    #[test]
    fn compute_snooze_until_monday_when_today_is_monday_jumps_next_week() {
        // 2026-05-11 是周一；snooze monday 应跳到 2026-05-18（下周一）
        let now = ndt2(2026, 5, 11, 12, 0);
        let until = compute_snooze_until(SnoozeSpec::Monday, now);
        assert_eq!(until, ndt2(2026, 5, 18, 9, 0));
    }

    #[test]
    fn compute_snooze_until_monday_when_today_is_wednesday() {
        // 2026-05-13 是周三；snooze monday 应跳到 2026-05-18（5 天后周一）
        let now = ndt2(2026, 5, 13, 12, 0);
        let until = compute_snooze_until(SnoozeSpec::Monday, now);
        assert_eq!(until, ndt2(2026, 5, 18, 9, 0));
    }

    #[test]
    fn compute_snooze_until_monday_when_today_is_sunday() {
        // 2026-05-17 是周日；snooze monday 应跳到 2026-05-18（次日周一）
        let now = ndt2(2026, 5, 17, 12, 0);
        let until = compute_snooze_until(SnoozeSpec::Monday, now);
        assert_eq!(until, ndt2(2026, 5, 18, 9, 0));
    }

    #[test]
    fn whoami_reply_top_tools_caps_at_three() {
        let tools: Vec<(String, u64)> = vec![
            ("a".to_string(), 5),
            ("b".to_string(), 4),
            ("c".to_string(), 3),
            ("d".to_string(), 2),
            ("e".to_string(), 1),
        ];
        let s = format_whoami_reply("", None, None, "", &tools);
        assert!(s.contains("`a`×5"));
        assert!(s.contains("`b`×4"));
        assert!(s.contains("`c`×3"));
        assert!(!s.contains("`d`"), "should cap at top 3: {s}");
        assert!(!s.contains("`e`"), "should cap at top 3: {s}");
    }

    // -------- /today parse + format --------

    #[test]
    fn parses_today() {
        assert_eq!(parse_tg_command("/today"), Some(TgCommand::Today));
    }

    #[test]
    fn parses_today_ignores_trailing() {
        assert_eq!(parse_tg_command("/today rest"), Some(TgCommand::Today));
    }

    #[test]
    fn today_reply_empty_buckets_show_quiet() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        let s = format_today_reply(&[], today);
        assert!(s.contains("📅 今日（2026-05-14）"), "today reply: {s}");
        assert!(s.contains("今日队列清爽 ✨"), "today reply: {s}");
    }

    // -------- /due parse + range + format --------

    #[test]
    fn due_parses_default_to_tomorrow_when_no_arg() {
        assert_eq!(
            parse_tg_command("/due"),
            Some(TgCommand::Due {
                preset: Some(DuePreset::Tomorrow),
                raw_arg: String::new(),
            })
        );
        // 全空白也算无参
        assert_eq!(
            parse_tg_command("/due   "),
            Some(TgCommand::Due {
                preset: Some(DuePreset::Tomorrow),
                raw_arg: String::new(),
            })
        );
    }

    #[test]
    fn due_parses_aliases_case_insensitive() {
        for s in ["tomorrow", "TMR", "明天", "明日"] {
            let parsed = parse_tg_command(&format!("/due {s}"));
            match parsed {
                Some(TgCommand::Due { preset: Some(DuePreset::Tomorrow), .. }) => {}
                other => panic!("expected Tomorrow for {s}, got {other:?}"),
            }
        }
        for s in ["thisweek", "this-week", "本周", "这周"] {
            let parsed = parse_tg_command(&format!("/due {s}"));
            match parsed {
                Some(TgCommand::Due { preset: Some(DuePreset::ThisWeek), .. }) => {}
                other => panic!("expected ThisWeek for {s}, got {other:?}"),
            }
        }
        for s in ["nextweek", "next-week", "下周"] {
            let parsed = parse_tg_command(&format!("/due {s}"));
            match parsed {
                Some(TgCommand::Due { preset: Some(DuePreset::NextWeek), .. }) => {}
                other => panic!("expected NextWeek for {s}, got {other:?}"),
            }
        }
    }

    #[test]
    fn due_parses_unknown_preset_stores_raw_arg() {
        let parsed = parse_tg_command("/due lastweek");
        match parsed {
            Some(TgCommand::Due { preset: None, raw_arg }) => {
                assert_eq!(raw_arg, "lastweek");
            }
            other => panic!("expected None preset for unknown, got {other:?}"),
        }
    }

    #[test]
    fn due_preset_range_tomorrow_is_single_day() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        let (s, e) = due_preset_range(DuePreset::Tomorrow, today);
        assert_eq!(s, chrono::NaiveDate::from_ymd_opt(2026, 5, 15).unwrap());
        assert_eq!(s, e);
    }

    #[test]
    fn due_preset_range_thisweek_iso_mon_to_sun() {
        // 2026-05-14 是周四 (weekday=3 from Monday)。本周 = 5/11 (Mon) ~ 5/17 (Sun)。
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        let (s, e) = due_preset_range(DuePreset::ThisWeek, today);
        assert_eq!(s, chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap());
        assert_eq!(e, chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap());
    }

    #[test]
    fn due_preset_range_thisweek_when_today_is_monday() {
        // 边界：今天就是周一 — 本周从今天起。
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap();
        let (s, e) = due_preset_range(DuePreset::ThisWeek, today);
        assert_eq!(s, today);
        assert_eq!(e, chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap());
    }

    #[test]
    fn due_preset_range_nextweek_starts_after_this_sunday() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        let (s, e) = due_preset_range(DuePreset::NextWeek, today);
        assert_eq!(s, chrono::NaiveDate::from_ymd_opt(2026, 5, 18).unwrap());
        assert_eq!(e, chrono::NaiveDate::from_ymd_opt(2026, 5, 24).unwrap());
    }

    #[test]
    fn due_reply_unknown_preset_shows_usage_hint_with_raw() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        let s = format_due_reply(&[], None, "lastweek", today);
        assert!(s.contains("未识别 preset"), "{s}");
        assert!(s.contains("lastweek"), "should echo raw arg: {s}");
    }

    #[test]
    fn due_reply_tomorrow_filters_by_date() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        let due_tomorrow = view(
            "写周报",
            3,
            Some("2026-05-15T18:00"),
            TaskStatus::Pending,
            None,
        );
        let due_today = view(
            "整理 Downloads",
            3,
            Some("2026-05-14T18:00"),
            TaskStatus::Pending,
            None,
        );
        let due_next_monday = view(
            "季度规划",
            3,
            Some("2026-05-18T09:00"),
            TaskStatus::Pending,
            None,
        );
        let views = vec![due_today, due_tomorrow, due_next_monday];
        let s = format_due_reply(&views, Some(DuePreset::Tomorrow), "", today);
        assert!(s.contains("明天"), "{s}");
        assert!(s.contains("写周报"), "{s}");
        assert!(!s.contains("整理 Downloads"), "today excluded: {s}");
        assert!(!s.contains("季度规划"), "next week excluded: {s}");
    }

    #[test]
    fn due_reply_thisweek_includes_remaining_days() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        let mon = view(
            "周一 task",
            3,
            Some("2026-05-11T09:00"),
            TaskStatus::Pending,
            None,
        );
        let sat = view(
            "周六 task",
            3,
            Some("2026-05-16T20:00"),
            TaskStatus::Pending,
            None,
        );
        let next_mon = view(
            "下周一",
            3,
            Some("2026-05-18T09:00"),
            TaskStatus::Pending,
            None,
        );
        let views = vec![mon, sat, next_mon];
        let s = format_due_reply(&views, Some(DuePreset::ThisWeek), "", today);
        assert!(s.contains("本周"), "{s}");
        assert!(s.contains("周一 task"), "{s}");
        assert!(s.contains("周六 task"), "{s}");
        assert!(!s.contains("下周一"), "next week excluded: {s}");
    }

    #[test]
    fn due_reply_excludes_done_and_no_due() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        // done 在 tomorrow 也不计（命令只看 pending）
        let done = view("完成的", 3, Some("2026-05-15T18:00"), TaskStatus::Done, None);
        // pending 但无 due → 不计
        let no_due = view("无 due 的", 3, None, TaskStatus::Pending, None);
        let s = format_due_reply(
            &[done, no_due],
            Some(DuePreset::Tomorrow),
            "",
            today,
        );
        assert!(s.contains("无 due 任务"), "should be empty: {s}");
        assert!(!s.contains("完成的"), "{s}");
        assert!(!s.contains("无 due 的"), "{s}");
    }

    #[test]
    fn due_reply_sorts_by_due_ascending() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        let mid = view(
            "中间",
            3,
            Some("2026-05-13T12:00"),
            TaskStatus::Pending,
            None,
        );
        let early = view(
            "靠前",
            3,
            Some("2026-05-11T09:00"),
            TaskStatus::Pending,
            None,
        );
        let late = view(
            "靠后",
            3,
            Some("2026-05-17T22:00"),
            TaskStatus::Pending,
            None,
        );
        let views = vec![mid, late, early];
        let s = format_due_reply(&views, Some(DuePreset::ThisWeek), "", today);
        let idx_early = s.find("靠前").expect("early in output");
        let idx_mid = s.find("中间").expect("mid in output");
        let idx_late = s.find("靠后").expect("late in output");
        assert!(idx_early < idx_mid, "early should be before mid: {s}");
        assert!(idx_mid < idx_late, "mid should be before late: {s}");
    }

    #[test]
    fn today_reply_mixed_buckets() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        // 今日到期
        let mut due_today = view(
            "整理 Downloads",
            3,
            Some("2026-05-14T18:00"),
            TaskStatus::Pending,
            None,
        );
        due_today.updated_at = "2026-05-14T11:00:00+08:00".to_string();
        // 明日到期 → 不计
        let mut due_tomorrow = view(
            "写周报",
            3,
            Some("2026-05-15T18:00"),
            TaskStatus::Pending,
            None,
        );
        due_tomorrow.updated_at = "2026-05-14T11:00:00+08:00".to_string();
        // 今日完成
        let mut done_today = view("跑步", 0, None, TaskStatus::Done, Some("5km"));
        done_today.updated_at = "2026-05-14T10:00:00+08:00".to_string();
        // 昨日完成 → 不计
        let mut done_yesterday = view("洗碗", 0, None, TaskStatus::Done, None);
        done_yesterday.updated_at = "2026-05-13T10:00:00+08:00".to_string();
        let views = vec![due_today, due_tomorrow, done_today, done_yesterday];
        let s = format_today_reply(&views, today);
        assert!(s.contains("今日到期（1）"), "today reply: {s}");
        assert!(s.contains("整理 Downloads"), "today reply: {s}");
        assert!(s.contains("今日已完成（1）"), "today reply: {s}");
        assert!(s.contains("跑步"), "today reply: {s}");
        assert!(!s.contains("写周报"), "today reply: {s}");
        assert!(!s.contains("洗碗"), "today reply: {s}");
        assert!(!s.contains("今日队列清爽"), "today reply: {s}");
    }

    // -------- /recent parse + format --------

    #[test]
    fn recent_parses_default_5_when_no_arg() {
        assert_eq!(parse_tg_command("/recent"), Some(TgCommand::Recent { n: 5 }));
        assert_eq!(parse_tg_command("/recent  "), Some(TgCommand::Recent { n: 5 }));
    }

    #[test]
    fn recent_parses_explicit_n() {
        assert_eq!(parse_tg_command("/recent 10"), Some(TgCommand::Recent { n: 10 }));
        assert_eq!(parse_tg_command("/recent 1"), Some(TgCommand::Recent { n: 1 }));
    }

    #[test]
    fn recent_clamps_to_1_20_range() {
        assert_eq!(parse_tg_command("/recent 0"), Some(TgCommand::Recent { n: 1 }));
        assert_eq!(parse_tg_command("/recent 21"), Some(TgCommand::Recent { n: 20 }));
        assert_eq!(parse_tg_command("/recent 9999"), Some(TgCommand::Recent { n: 20 }));
    }

    #[test]
    fn recent_garbage_arg_falls_back_to_default() {
        // 非数字 → 默认 5（与 /tasks since:7d 同前向兼容策略）
        assert_eq!(
            parse_tg_command("/recent abc"),
            Some(TgCommand::Recent { n: 5 })
        );
    }

    #[test]
    fn recent_reply_empty_done_says_no_records() {
        let s = format_recent_reply(&[], 5);
        assert!(s.contains("✨"), "recent reply: {s}");
        assert!(s.contains("暂无完成记录"), "recent reply: {s}");
    }

    #[test]
    fn recent_reply_orders_by_updated_at_desc() {
        let mut a = view("早的任务", 0, None, TaskStatus::Done, None);
        a.updated_at = "2026-05-13T10:00:00+08:00".to_string();
        let mut b = view("最新的任务", 0, None, TaskStatus::Done, None);
        b.updated_at = "2026-05-14T11:00:00+08:00".to_string();
        let mut c = view("中间的任务", 0, None, TaskStatus::Done, None);
        c.updated_at = "2026-05-14T09:00:00+08:00".to_string();
        let views = vec![a, b, c];
        let s = format_recent_reply(&views, 3);
        // "最新的任务" 在 "中间的任务" 之前；"早的任务" 在最后
        let pos_latest = s.find("最新的任务").expect("latest present");
        let pos_middle = s.find("中间的任务").expect("middle present");
        let pos_early = s.find("早的任务").expect("early present");
        assert!(pos_latest < pos_middle, "order: {s}");
        assert!(pos_middle < pos_early, "order: {s}");
        assert!(s.contains("共 3"), "header: {s}");
        assert!(s.contains("05-14 11:00"), "ts format: {s}");
    }

    #[test]
    fn recent_reply_skips_non_done_status() {
        let mut p = view("pending 的", 0, None, TaskStatus::Pending, None);
        p.updated_at = "2026-05-14T11:00:00+08:00".to_string();
        let mut d = view("done 的", 0, None, TaskStatus::Done, None);
        d.updated_at = "2026-05-14T10:00:00+08:00".to_string();
        let s = format_recent_reply(&vec![p, d], 5);
        assert!(s.contains("done 的"), "done present: {s}");
        assert!(!s.contains("pending 的"), "pending skipped: {s}");
    }

    #[test]
    fn recent_reply_truncates_to_n_and_shows_remaining_count() {
        let mut views = Vec::new();
        for i in 0..7 {
            let mut v = view(&format!("done-{}", i), 0, None, TaskStatus::Done, None);
            // 升序 ts → 最高 idx 最新（formatter 倒序后 done-6 在前）
            v.updated_at = format!("2026-05-14T1{}:00:00+08:00", i);
            views.push(v);
        }
        let s = format_recent_reply(&views, 3);
        assert!(s.contains("最近 3 条完成（共 7）"), "header: {s}");
        // 倒序应显 done-6 / done-5 / done-4
        assert!(s.contains("done-6"), "{s}");
        assert!(s.contains("done-5"), "{s}");
        assert!(s.contains("done-4"), "{s}");
        // done-3 / done-2 / done-1 / done-0 不显（被截断）
        assert!(!s.contains("done-3"), "{s}");
        assert!(s.contains("还有 4 条更早完成"), "overflow hint: {s}");
    }

    // -------- /find parse + format --------

    #[test]
    fn find_parses_keyword_arg() {
        assert_eq!(
            parse_tg_command("/find Downloads"),
            Some(TgCommand::Find {
                keyword: "Downloads".to_string()
            })
        );
        assert_eq!(
            parse_tg_command("/find 整理 桌面"),
            Some(TgCommand::Find {
                keyword: "整理 桌面".to_string()
            })
        );
    }

    #[test]
    fn find_empty_keyword_returns_usage_hint() {
        let s = format_find_reply(&[], "");
        assert!(s.contains("用法"), "missing-arg reply: {s}");
        assert!(s.contains("/find <keyword>"), "{s}");
    }

    #[test]
    fn find_no_hits_shows_keyword_in_reply() {
        let v = view("跑步", 0, None, TaskStatus::Pending, None);
        let s = format_find_reply(&[v], "周报");
        assert!(s.contains("没有任务命中「周报」"), "{s}");
    }

    #[test]
    fn find_matches_title_case_insensitive() {
        let v = view("Download 整理", 0, None, TaskStatus::Pending, None);
        let s = format_find_reply(&[v], "download");
        assert!(s.contains("命中「download」"), "{s}");
        assert!(s.contains("Download 整理"), "{s}");
    }

    #[test]
    fn find_matches_raw_description_substring() {
        let mut v = view("跑步", 0, None, TaskStatus::Pending, None);
        v.raw_description = "[task pri=3] 跑步 #健身 [origin:tg:1] 5km".to_string();
        let s = format_find_reply(&[v], "健身");
        assert!(s.contains("跑步"), "{s}");
    }

    #[test]
    fn find_orders_pending_before_done() {
        let mut p = view("pending-cmd", 0, None, TaskStatus::Pending, None);
        p.updated_at = "2026-05-13T10:00:00+08:00".to_string();
        let mut d = view("done-cmd", 0, None, TaskStatus::Done, None);
        d.updated_at = "2026-05-14T11:00:00+08:00".to_string();
        let s = format_find_reply(&[d, p], "cmd");
        let pos_pending = s.find("pending-cmd").expect("pending shown");
        let pos_done = s.find("done-cmd").expect("done shown");
        assert!(pos_pending < pos_done, "pending before done: {s}");
    }

    #[test]
    fn find_caps_at_10_hits_with_overflow_hint() {
        let mut views = Vec::new();
        for i in 0..15 {
            views.push(view(
                &format!("task-{}", i),
                0,
                None,
                TaskStatus::Pending,
                None,
            ));
        }
        let s = format_find_reply(&views, "task");
        // header 显总命中数 15
        assert!(s.contains("命中「task」15 条"), "{s}");
        // 只显前 10
        assert!(s.contains("task-0"), "{s}");
        assert!(s.contains("task-9"), "{s}");
        assert!(!s.contains("task-10"), "{s}");
        // 溢出 hint
        assert!(s.contains("还有 5 条命中"), "{s}");
    }

    // -------- /blocked parse + format --------

    #[test]
    fn blocked_parses_no_arg() {
        assert_eq!(parse_tg_command("/blocked"), Some(TgCommand::Blocked));
        assert_eq!(parse_tg_command("/blocked  "), Some(TgCommand::Blocked));
        assert_eq!(parse_tg_command("/blocked now"), Some(TgCommand::Blocked));
    }

    #[test]
    fn blocked_reply_empty_views_friendly() {
        let s = format_blocked_reply(&[]);
        assert!(s.contains("✅"), "{s}");
        assert!(s.contains("暂无被卡的 task"), "{s}");
    }

    #[test]
    fn blocked_reply_no_active_blockers_friendly() {
        // 有 task 但都没 blockedBy
        let a = view("a", 0, None, TaskStatus::Pending, None);
        let b = view("b", 0, None, TaskStatus::Done, None);
        let s = format_blocked_reply(&[a, b]);
        assert!(s.contains("暂无被卡的 task"), "{s}");
    }

    #[test]
    fn blocked_reply_lists_blocker_with_active_dependency() {
        let mut a = view("写决策文档", 0, None, TaskStatus::Pending, None);
        a.blocked_by = vec!["调研竞品".to_string()];
        let b = view("调研竞品", 0, None, TaskStatus::Pending, None);
        let s = format_blocked_reply(&[a, b]);
        assert!(s.contains("被卡的 task 1 条"), "header: {s}");
        assert!(s.contains("🟢 写决策文档"), "{s}");
        assert!(s.contains("等：调研竞品"), "{s}");
    }

    #[test]
    fn blocked_reply_skips_when_blocker_already_done() {
        // blockedBy 引用了一条 done 的任务 — 视作"已解决"，不显
        let mut a = view("写决策文档", 0, None, TaskStatus::Pending, None);
        a.blocked_by = vec!["调研竞品".to_string()];
        let b = view("调研竞品", 0, None, TaskStatus::Done, None);
        let s = format_blocked_reply(&[a, b]);
        assert!(s.contains("暂无被卡的 task"), "{s}");
    }

    #[test]
    fn blocked_reply_skips_done_task_even_with_unresolved_blocker() {
        // 自己已 done 的 task 不算"被卡" — 即使它的 blockedBy 还指向 active task
        let mut a = view("写决策文档", 0, None, TaskStatus::Done, None);
        a.blocked_by = vec!["调研竞品".to_string()];
        let b = view("调研竞品", 0, None, TaskStatus::Pending, None);
        let s = format_blocked_reply(&[a, b]);
        assert!(s.contains("暂无被卡的 task"), "{s}");
    }

    #[test]
    fn blocked_reply_multi_blockers_per_task_listed() {
        let mut a = view("写文档", 0, None, TaskStatus::Pending, None);
        a.blocked_by = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let av = view("A", 0, None, TaskStatus::Pending, None);
        let bv = view("B", 0, None, TaskStatus::Pending, None);
        // C 不在列表（typo / 已删 — 视作已解决，宽容语义）
        let s = format_blocked_reply(&[a, av, bv]);
        assert!(s.contains("被卡的 task 1 条"), "{s}");
        assert!(s.contains("等：A"), "{s}");
        assert!(s.contains("等：B"), "{s}");
        // C 视作已解决，不出现
        assert!(!s.contains("等：C"), "{s}");
    }

    #[test]
    fn blocked_reply_error_state_also_blocks() {
        // 一条 error task 的 blockedBy 引用了 active task — 也算被卡
        let mut a = view("写文档", 0, None, TaskStatus::Error, Some("LLM 拒"));
        a.blocked_by = vec!["调研".to_string()];
        let b = view("调研", 0, None, TaskStatus::Pending, None);
        let s = format_blocked_reply(&[a, b]);
        assert!(s.contains("⚠️ 写文档"), "{s}");
    }

    // -------- /snoozed parse + format --------

    #[test]
    fn snoozed_parses_no_arg() {
        assert_eq!(parse_tg_command("/snoozed"), Some(TgCommand::Snoozed));
        assert_eq!(parse_tg_command("/snoozed  "), Some(TgCommand::Snoozed));
        assert_eq!(parse_tg_command("/snoozed now"), Some(TgCommand::Snoozed));
    }

    #[test]
    fn snoozed_reply_empty_friendly_with_command_hint() {
        let now = chrono::NaiveDateTime::parse_from_str(
            "2026-05-17T10:00",
            "%Y-%m-%dT%H:%M",
        )
        .unwrap();
        let s = format_snoozed_reply(&[], now);
        assert!(s.contains("💤"), "{s}");
        assert!(s.contains("暂无被暂存"), "{s}");
        assert!(s.contains("/snooze"), "hint: {s}");
    }

    #[test]
    fn snoozed_reply_skips_views_without_snoozed_until() {
        let a = view("无 snooze", 0, None, TaskStatus::Pending, None);
        let now = chrono::NaiveDateTime::parse_from_str(
            "2026-05-17T10:00",
            "%Y-%m-%dT%H:%M",
        )
        .unwrap();
        let s = format_snoozed_reply(&[a], now);
        assert!(s.contains("暂无"), "{s}");
    }

    #[test]
    fn snoozed_reply_minutes_label() {
        let mut a = view("等下个 sprint", 0, None, TaskStatus::Pending, None);
        a.snoozed_until = Some("2026-05-17T10:45".to_string());
        let now = chrono::NaiveDateTime::parse_from_str(
            "2026-05-17T10:00",
            "%Y-%m-%dT%H:%M",
        )
        .unwrap();
        let s = format_snoozed_reply(&[a], now);
        assert!(s.contains("45 分后醒"), "{s}");
        assert!(s.contains("等下个 sprint"), "{s}");
        assert!(s.contains("（05-17 10:45）"), "until_short: {s}");
    }

    #[test]
    fn snoozed_reply_hours_minutes_label() {
        let mut a = view("写文档", 0, None, TaskStatus::Pending, None);
        a.snoozed_until = Some("2026-05-17T12:30".to_string());
        let now = chrono::NaiveDateTime::parse_from_str(
            "2026-05-17T10:00",
            "%Y-%m-%dT%H:%M",
        )
        .unwrap();
        let s = format_snoozed_reply(&[a], now);
        assert!(s.contains("2 时 30 分后醒"), "{s}");
    }

    #[test]
    fn snoozed_reply_days_label() {
        let mut a = view("整理 Downloads", 0, None, TaskStatus::Pending, None);
        a.snoozed_until = Some("2026-05-20T15:00".to_string());
        let now = chrono::NaiveDateTime::parse_from_str(
            "2026-05-17T10:00",
            "%Y-%m-%dT%H:%M",
        )
        .unwrap();
        let s = format_snoozed_reply(&[a], now);
        assert!(s.contains("3 天 5 时后醒"), "{s}");
    }

    #[test]
    fn snoozed_reply_orders_by_wake_time_asc() {
        let mut later = view("后醒的", 0, None, TaskStatus::Pending, None);
        later.snoozed_until = Some("2026-05-17T15:00".to_string());
        let mut sooner = view("先醒的", 0, None, TaskStatus::Pending, None);
        sooner.snoozed_until = Some("2026-05-17T11:00".to_string());
        let now = chrono::NaiveDateTime::parse_from_str(
            "2026-05-17T10:00",
            "%Y-%m-%dT%H:%M",
        )
        .unwrap();
        let s = format_snoozed_reply(&[later, sooner], now);
        let pos_sooner = s.find("先醒的").expect("sooner present");
        let pos_later = s.find("后醒的").expect("later present");
        assert!(pos_sooner < pos_later, "sooner first: {s}");
    }

    // -------- /mute parse + format --------

    #[test]
    fn mute_parses_default_30_when_no_arg() {
        assert_eq!(
            parse_tg_command("/mute"),
            Some(TgCommand::Mute { minutes: 30 })
        );
        assert_eq!(
            parse_tg_command("/mute   "),
            Some(TgCommand::Mute { minutes: 30 })
        );
    }

    #[test]
    fn mute_parses_explicit_n() {
        assert_eq!(
            parse_tg_command("/mute 60"),
            Some(TgCommand::Mute { minutes: 60 })
        );
        assert_eq!(
            parse_tg_command("/mute 0"),
            Some(TgCommand::Mute { minutes: 0 })
        );
    }

    #[test]
    fn mute_clamps_to_0_10080_range() {
        // 负数 → 0；> 7 天 → 10080
        assert_eq!(
            parse_tg_command("/mute -10"),
            Some(TgCommand::Mute { minutes: 0 })
        );
        assert_eq!(
            parse_tg_command("/mute 99999"),
            Some(TgCommand::Mute { minutes: 10080 })
        );
    }

    #[test]
    fn mute_garbage_arg_falls_back_to_default() {
        assert_eq!(
            parse_tg_command("/mute abc"),
            Some(TgCommand::Mute { minutes: 30 })
        );
    }

    #[test]
    fn format_mute_reply_zero_says_cleared() {
        let s = format_mute_reply(0, None);
        assert!(s.contains("🔊"), "{s}");
        assert!(s.contains("解除"), "{s}");
    }

    #[test]
    fn format_mute_reply_minutes_label() {
        let until = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 10, 30, 0)
            .unwrap();
        let s = format_mute_reply(45, Some(until));
        assert!(s.contains("🔕"), "{s}");
        assert!(s.contains("45 分钟"), "{s}");
        assert!(s.contains("10:30"), "{s}");
    }

    #[test]
    fn format_mute_reply_hours_minutes_label() {
        let until = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 12, 30, 0)
            .unwrap();
        let s = format_mute_reply(150, Some(until));
        // 150 分钟 = 2 小时 30 分钟
        assert!(s.contains("2 小时 30 分钟"), "{s}");
        assert!(s.contains("12:30"), "{s}");
    }

    #[test]
    fn format_mute_reply_days_label() {
        let until = chrono::Local
            .with_ymd_and_hms(2026, 5, 20, 9, 0, 0)
            .unwrap();
        // 3 天 = 4320 分钟
        let s = format_mute_reply(4320, Some(until));
        assert!(s.contains("3 天"), "{s}");
    }

    // -------- /note parse + format --------

    #[test]
    fn note_parses_text_arg() {
        assert_eq!(
            parse_tg_command("/note 周末跑 5km"),
            Some(TgCommand::Note {
                text: "周末跑 5km".to_string()
            })
        );
    }

    #[test]
    fn note_parses_empty_text() {
        assert_eq!(
            parse_tg_command("/note"),
            Some(TgCommand::Note {
                text: String::new()
            })
        );
        assert_eq!(
            parse_tg_command("/note   "),
            Some(TgCommand::Note {
                text: String::new()
            })
        );
    }

    #[test]
    fn note_reply_empty_shows_usage_hint() {
        let s = format_note_reply("", Ok(""));
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/note <text>"), "{s}");
        assert!(s.contains("general memory item"), "{s}");
    }

    #[test]
    fn note_reply_whitespace_treated_as_empty() {
        let s = format_note_reply("   \t\n  ", Ok(""));
        assert!(s.contains("用法"), "{s}");
    }

    #[test]
    fn note_reply_success_shows_title_and_preview() {
        let s = format_note_reply(
            "周末跑 5km 后腿酸；下次先热身",
            Ok("note-2026-05-17T10-30-15"),
        );
        assert!(s.contains("📝"), "{s}");
        assert!(s.contains("general/note-2026-05-17T10-30-15"), "{s}");
        assert!(s.contains("周末跑 5km"), "preview: {s}");
    }

    #[test]
    fn note_reply_long_text_truncates_preview() {
        let long = "x".repeat(100);
        let s = format_note_reply(&long, Ok("note-test"));
        // preview cap 60 chars
        assert!(s.contains("…"), "should truncate: {s}");
    }

    #[test]
    fn note_reply_save_failure_shows_error() {
        let s = format_note_reply("test note", Err("disk full"));
        assert!(s.contains("保存失败"), "{s}");
        assert!(s.contains("disk full"), "{s}");
    }

    // -------- /show parse + format --------

    #[test]
    fn show_parses_title_arg() {
        assert_eq!(
            parse_tg_command("/show 整理 Downloads"),
            Some(TgCommand::Show {
                title: "整理 Downloads".to_string()
            })
        );
    }

    #[test]
    fn show_parses_empty_title() {
        assert_eq!(
            parse_tg_command("/show"),
            Some(TgCommand::Show {
                title: String::new()
            })
        );
    }

    #[test]
    fn show_reply_renders_title_with_status_emoji_per_state() {
        let s = format_show_reply("写周报", "[task pri=3] 写周报", "", TaskStatus::Pending);
        assert!(s.contains("⏳"), "pending should show hourglass: {s}");
        assert!(s.contains("写周报"), "{s}");
        let s = format_show_reply("写周报", "[task pri=3] 写周报", "", TaskStatus::Done);
        assert!(s.contains("✅"), "done should show check: {s}");
        let s = format_show_reply("写周报", "[task pri=3] 写周报", "", TaskStatus::Error);
        assert!(s.contains("⚠️"), "error should show warning: {s}");
        let s = format_show_reply("写周报", "[task pri=3] 写周报", "", TaskStatus::Cancelled);
        assert!(s.contains("🚫"), "cancelled should show cross: {s}");
    }

    #[test]
    fn show_reply_shows_raw_description_full() {
        let raw = "[task pri=5 due=2026-05-20] 写 Q2 周报 [pinned] [silent]";
        let s = format_show_reply("写周报", raw, "", TaskStatus::Pending);
        assert!(s.contains(raw), "should include full raw: {s}");
        assert!(!s.contains("截断"), "short raw should not be truncated: {s}");
    }

    #[test]
    fn show_reply_truncates_long_raw_description() {
        let long_raw = "a".repeat(SHOW_RAW_DESC_CAP + 100);
        let s = format_show_reply("t", &long_raw, "", TaskStatus::Pending);
        assert!(s.contains("截断"), "should mark truncation: {s}");
        assert!(s.contains(&format!("共 {} 字符", SHOW_RAW_DESC_CAP + 100)), "{s}");
    }

    #[test]
    fn show_reply_includes_detail_md_preview_when_present() {
        let detail = "## 进度\n\n- 收集了 5 篇参考\n- 写了 outline";
        let s = format_show_reply("t", "[task pri=3] body", detail, TaskStatus::Pending);
        assert!(s.contains("📝 detail.md"), "{s}");
        assert!(s.contains("收集了 5 篇参考"), "preview: {s}");
        // length hint
        let detail_chars: usize = detail.chars().count();
        assert!(s.contains(&format!("{} 字符", detail_chars)), "{s}");
    }

    #[test]
    fn show_reply_omits_detail_section_when_empty() {
        let s = format_show_reply("t", "[task pri=3] body", "", TaskStatus::Pending);
        assert!(!s.contains("📝 detail.md"), "should not show empty section: {s}");
    }

    #[test]
    fn show_reply_truncates_long_detail_md_with_ellipsis() {
        let long_detail = "x".repeat(SHOW_DETAIL_PREVIEW_CHARS + 50);
        let s = format_show_reply("t", "raw", &long_detail, TaskStatus::Pending);
        assert!(s.contains("…"), "should truncate detail with ellipsis: {s}");
        assert!(
            s.contains(&format!("{} 字符", SHOW_DETAIL_PREVIEW_CHARS + 50)),
            "{s}"
        );
    }

    #[test]
    fn show_reply_handles_empty_raw_description_gracefully() {
        let s = format_show_reply("t", "", "", TaskStatus::Pending);
        assert!(s.contains("raw_description 为空"), "should hint empty raw: {s}");
        assert!(!s.contains("📝"), "no detail section either: {s}");
    }

    // -------- /reflect parse + format --------

    #[test]
    fn reflect_parses_text_arg() {
        assert_eq!(
            parse_tg_command("/reflect 今天回顾：接受中断太多"),
            Some(TgCommand::Reflect {
                text: "今天回顾：接受中断太多".to_string()
            })
        );
    }

    #[test]
    fn reflect_parses_empty_text() {
        assert_eq!(
            parse_tg_command("/reflect"),
            Some(TgCommand::Reflect {
                text: String::new()
            })
        );
        assert_eq!(
            parse_tg_command("/reflect   "),
            Some(TgCommand::Reflect {
                text: String::new()
            })
        );
    }

    #[test]
    fn reflect_reply_empty_shows_usage_hint() {
        let s = format_reflect_reply("", Ok(""));
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/reflect <text>"), "{s}");
        assert!(s.contains("ai_insights"), "must name category: {s}");
        // 对比 /note：让 owner 知道不要选错入口
        assert!(s.contains("/note"), "should compare with /note: {s}");
    }

    #[test]
    fn reflect_reply_success_shows_category_and_title() {
        let s = format_reflect_reply(
            "今天观察：长 task 拆细后完成率明显提升",
            Ok("reflect-2026-05-17T13-44-00"),
        );
        assert!(s.contains("🪞"), "{s}");
        assert!(
            s.contains("ai_insights/reflect-2026-05-17T13-44-00"),
            "{s}"
        );
        assert!(s.contains("长 task 拆细"), "preview: {s}");
    }

    #[test]
    fn reflect_reply_long_text_truncates_preview() {
        let long = "x".repeat(100);
        let s = format_reflect_reply(&long, Ok("reflect-test"));
        assert!(s.contains("…"), "should truncate: {s}");
    }

    #[test]
    fn reflect_reply_save_failure_shows_error() {
        let s = format_reflect_reply("ref text", Err("disk full"));
        assert!(s.contains("保存失败"), "{s}");
        assert!(s.contains("disk full"), "{s}");
    }

    // -------- /edit parse + format --------

    #[test]
    fn edit_parses_title_and_desc_split_on_double_colon() {
        assert_eq!(
            parse_tg_command("/edit 整理 Downloads :: 新的 description 一段"),
            Some(TgCommand::Edit {
                title: "整理 Downloads".to_string(),
                new_desc: "新的 description 一段".to_string(),
            })
        );
    }

    #[test]
    fn edit_splits_on_first_double_colon() {
        // 新 desc 本身含 `::` 不能被吞掉 — split_once 只切首个。
        assert_eq!(
            parse_tg_command("/edit task A :: body has :: inside"),
            Some(TgCommand::Edit {
                title: "task A".to_string(),
                new_desc: "body has :: inside".to_string(),
            })
        );
    }

    #[test]
    fn edit_no_separator_yields_empty_desc() {
        // 没 `::` separator → 整体当 title，new_desc 空让 handler 走 usage hint
        assert_eq!(
            parse_tg_command("/edit 写周报"),
            Some(TgCommand::Edit {
                title: "写周报".to_string(),
                new_desc: String::new(),
            })
        );
    }

    #[test]
    fn edit_empty_title_or_desc_after_split() {
        // 仅 `::` → 两端都空
        assert_eq!(
            parse_tg_command("/edit ::"),
            Some(TgCommand::Edit {
                title: String::new(),
                new_desc: String::new(),
            })
        );
        // title 空 desc 有
        assert_eq!(
            parse_tg_command("/edit :: 新 body"),
            Some(TgCommand::Edit {
                title: String::new(),
                new_desc: "新 body".to_string(),
            })
        );
    }

    #[test]
    fn edit_reply_missing_arg_shows_usage_hint() {
        let s = format_edit_reply("", "", Ok(()));
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/edit"), "{s}");
        assert!(s.contains("::"), "must show separator: {s}");
        assert!(s.contains("全量覆写") || s.contains("覆写"), "{s}");
    }

    #[test]
    fn edit_reply_partial_missing_arg_also_shows_hint() {
        // 仅 title 给了，desc 空 → usage hint
        let s = format_edit_reply("写周报", "", Ok(()));
        assert!(s.contains("用法"), "{s}");
        // 仅 desc 给了，title 空 → usage hint
        let s2 = format_edit_reply("", "新 body", Ok(()));
        assert!(s2.contains("用法"), "{s2}");
    }

    #[test]
    fn edit_reply_success_shows_title_and_preview() {
        let s = format_edit_reply("写周报", "完整新 body 一段 abc", Ok(()));
        assert!(s.contains("✏️"), "{s}");
        assert!(s.contains("已覆写"), "{s}");
        assert!(s.contains("写周报"), "{s}");
        assert!(s.contains("完整新 body 一段 abc"), "preview: {s}");
    }

    #[test]
    fn edit_reply_long_desc_truncates_preview() {
        let long = "x".repeat(120);
        let s = format_edit_reply("t", &long, Ok(()));
        // preview cap 80 chars
        assert!(s.contains("…"), "should truncate: {s}");
    }

    #[test]
    fn edit_reply_save_failure_shows_error() {
        let s = format_edit_reply("t", "new body", Err("not found"));
        assert!(s.contains("覆写失败"), "{s}");
        assert!(s.contains("not found"), "{s}");
    }

    // -------- /digest parse + format --------

    #[test]
    fn digest_parses_default_5() {
        assert_eq!(
            parse_tg_command("/digest"),
            Some(TgCommand::Digest { n: 5 })
        );
        assert_eq!(
            parse_tg_command("/digest  "),
            Some(TgCommand::Digest { n: 5 })
        );
    }

    #[test]
    fn digest_parses_explicit_n() {
        assert_eq!(
            parse_tg_command("/digest 10"),
            Some(TgCommand::Digest { n: 10 })
        );
        assert_eq!(
            parse_tg_command("/digest 1"),
            Some(TgCommand::Digest { n: 1 })
        );
    }

    #[test]
    fn digest_clamps_to_1_20() {
        assert_eq!(
            parse_tg_command("/digest 0"),
            Some(TgCommand::Digest { n: 1 })
        );
        assert_eq!(
            parse_tg_command("/digest 999"),
            Some(TgCommand::Digest { n: 20 })
        );
    }

    #[test]
    fn digest_garbage_arg_falls_back_to_default() {
        assert_eq!(
            parse_tg_command("/digest abc"),
            Some(TgCommand::Digest { n: 5 })
        );
    }

    #[test]
    fn digest_reply_empty_done_friendly() {
        let s = format_digest_reply(&[], 5);
        assert!(s.contains("✨"), "{s}");
        assert!(s.contains("暂无完成记录"), "{s}");
        assert!(s.contains("/digest"), "{s}");
    }

    #[test]
    fn digest_reply_orders_done_desc_with_result_summary() {
        let mut a = view("跑步", 0, None, TaskStatus::Done, Some("5km"));
        a.updated_at = "2026-05-13T10:00:00+08:00".to_string();
        let mut b = view(
            "整理 Downloads",
            0,
            None,
            TaskStatus::Done,
            Some("挪了 30 个文件"),
        );
        b.updated_at = "2026-05-14T11:00:00+08:00".to_string();
        let s = format_digest_reply(&[a, b], 5);
        let pos_b = s.find("整理 Downloads").expect("b present");
        let pos_a = s.find("跑步").expect("a present");
        assert!(pos_b < pos_a, "latest first: {s}");
        assert!(s.contains("— 5km"), "result attached: {s}");
        assert!(s.contains("— 挪了 30 个文件"), "result attached: {s}");
        assert!(s.contains("共 2"), "header: {s}");
        assert!(s.contains("05-14 11:00"), "ts format: {s}");
    }

    #[test]
    fn digest_reply_skips_non_done_status() {
        let mut p = view("pending 的", 0, None, TaskStatus::Pending, None);
        p.updated_at = "2026-05-14T11:00:00+08:00".to_string();
        let mut d = view("done 的", 0, None, TaskStatus::Done, Some("ok"));
        d.updated_at = "2026-05-14T10:00:00+08:00".to_string();
        let s = format_digest_reply(&vec![p, d], 5);
        assert!(s.contains("done 的"), "done present: {s}");
        assert!(!s.contains("pending 的"), "pending skipped: {s}");
        assert!(s.contains("— ok"), "result: {s}");
    }

    #[test]
    fn digest_reply_done_without_result_shows_no_em_dash() {
        let mut a = view("跑步", 0, None, TaskStatus::Done, None);
        a.updated_at = "2026-05-14T10:00:00+08:00".to_string();
        let s = format_digest_reply(&[a], 5);
        assert!(s.contains("跑步"), "{s}");
        assert!(!s.contains("跑步 —"), "no em dash: {s}");
    }

    #[test]
    fn digest_reply_truncates_long_result_to_80_chars() {
        let long = "x".repeat(120);
        let mut a = view("done", 0, None, TaskStatus::Done, Some(&long));
        a.updated_at = "2026-05-14T10:00:00+08:00".to_string();
        let s = format_digest_reply(&[a], 5);
        assert!(s.contains("…"), "should truncate: {s}");
    }

    #[test]
    fn digest_reply_overflow_hint_when_more_than_n() {
        let mut views = Vec::new();
        for i in 0..7 {
            let mut v = view(&format!("done-{}", i), 0, None, TaskStatus::Done, None);
            v.updated_at = format!("2026-05-14T1{}:00:00+08:00", i);
            views.push(v);
        }
        let s = format_digest_reply(&views, 3);
        assert!(s.contains("最近 3 条完成（共 7）"), "{s}");
        assert!(s.contains("还有 4 条"), "overflow hint: {s}");
    }

    // -------- /reset parse + format --------

    #[test]
    fn parses_reset() {
        assert_eq!(parse_tg_command("/reset"), Some(TgCommand::Reset));
    }

    #[test]
    fn parses_reset_ignores_trailing() {
        assert_eq!(parse_tg_command("/reset now"), Some(TgCommand::Reset));
    }

    #[test]
    fn reset_reply_mentions_persona_kept() {
        let s = format_reset_reply();
        assert!(s.contains("已重置"), "reset reply: {s}");
        assert!(s.contains("人设") || s.contains("系统"), "reset reply: {s}");
    }

    // -------- /version parse + format --------

    #[test]
    fn parses_version() {
        assert_eq!(parse_tg_command("/version"), Some(TgCommand::Version));
    }

    #[test]
    fn parses_version_ignores_trailing() {
        assert_eq!(parse_tg_command("/version please"), Some(TgCommand::Version));
    }

    #[test]
    fn version_reply_includes_app_and_schema() {
        let s = format_version_reply("0.1.0", 4);
        assert!(s.contains("pet v0.1.0"), "version reply: {s}");
        assert!(s.contains("schema v4"), "version reply: {s}");
    }

    #[test]
    fn version_reply_omits_schema_when_zero() {
        let s = format_version_reply("0.1.0", 0);
        assert!(s.contains("pet v0.1.0"), "version reply: {s}");
        assert!(!s.contains("schema"), "version reply: {s}");
    }

    #[test]
    fn version_reply_handles_missing_version() {
        let s = format_version_reply("", 4);
        assert!(s.contains("版本号缺失"), "version reply: {s}");
        assert!(s.contains("schema v4"), "version reply: {s}");
    }

    #[test]
    fn today_reply_overflow_renders_more_hint() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        let mut views: Vec<TaskView> = Vec::new();
        for i in 0..8 {
            let mut t = view(
                &format!("待办-{i}"),
                3,
                Some("2026-05-14T18:00"),
                TaskStatus::Pending,
                None,
            );
            t.updated_at = "2026-05-14T11:00:00+08:00".to_string();
            views.push(t);
        }
        let s = format_today_reply(&views, today);
        assert!(s.contains("今日到期（8）"), "today reply: {s}");
        assert!(s.contains("…还有 3 条"), "today reply: {s}");
    }
}
