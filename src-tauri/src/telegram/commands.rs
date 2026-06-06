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


#[path = "due_preset.rs"]
mod due_preset;
pub use due_preset::*;


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
    /// `/buckets` —— 本 chat 派单中 active task（pending / error）按
    /// priority 分桶计数 P0..P9 一行式 dump。与 /stats（状态分桶 — 待
    /// 办 / 逾期 / done / error / 取消）互补 — /buckets 是 priority
    /// 分桶维度，让 owner 看「我各档高优各有几条」分布。无参；多余尾
    /// 部忽略。
    Buckets,
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
    /// `/pinned_due` —— 列出本 chat 派单中同时 pinned + 含 due 的 active task
    /// （pending / error）。与 /pinned（仅 pinned）/ /due（仅 due）双重收
    /// 紧 — owner 紧急 audit「我钉了的 + 有截止时间的」高优清单。按 due
    /// 升序排（最近到期在前 — owner 关心"下一个 deadline 是哪条"）。
    /// 无参；多余尾部一律忽略。空 → 友好兜底提示 /pinned + /due 看更宽
    /// 视角。
    PinnedDue,
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
    /// `/tags` —— 列本 chat 派单中所有用过的 `#tag` + 各 tag 任务数（按
    /// 数量降序）。让 owner audit "我标过哪些自定义 tag"。与 /markers
    /// 对偶 —— 那个是系统 marker（pinned / silent），这个是 owner 自定
    /// 义 #tag 维度。无参；多余尾部忽略；最多列 15 tag，余下汇总"其它 N 个"。
    Tags,
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
    /// `/oldest_n [N]` —— 列本 chat oldest pending N 条（按 created_at
    /// 升序），audit「堆积最久的活」。N 缺省 5，clamp 1..=20。与 /recent
    /// 反向（recent = 最新 done；oldest_n = 最老 pending），让 owner 看
    /// 哪些 task 长期没动 — 决定是否 /pri / /cancel / 重组优先级。
    OldestN { n: u32 },
    /// `/active_recent [N]` —— 列本 chat 最近 N 条新创建的 active（pending /
    /// error）task — 与 /recent done 反向。N 缺省 5，clamp 1..=20。按
    /// created_at 倒序（最新创建在前），让 owner 在 TG 上扫读「我最近塞
    /// 了哪些活到队列」— 比 /last 多看几条；比 /tasks 更聚焦（只活动段
    /// 且按创建时序而非 compare_for_queue）。
    ActiveRecent { n: u32 },
    /// `/find <keyword>` —— 在本 chat 派单中搜 keyword（命中标题 / 描述子
    /// 串，case-insensitive），返回最多 10 条命中行（status emoji + 标题 +
    /// 命中点 hint）。空 keyword 由 handler 走 missing-argument。
    Find { keyword: String },
    /// `/find_in_detail <keyword>` —— 在本 chat 派单的 detail.md 内容里
    /// 搜 keyword（case-insensitive 子串），与 /find（仅扫标题 +
    /// raw_description）互补。让 owner audit「我笔记里写过 X」场景 — pet
    /// 在 detail.md 里写过相关进度 / 决策 / 复盘但标题没体现时本命令命
    /// 中。返回最多 8 条命中行（status emoji + 标题 + 命中点附近 60 字
    /// snippet）。空 keyword → missing-argument hint。
    FindInDetail { keyword: String },
    /// `/blocked` —— 列出本 chat 派单中被 `[blockedBy: ...]` 锁住的 active
    /// task（pending / error 状态）+ 每条仍未解决的 blocker 标题列表。无参；
    /// 多余尾部忽略（与 /tasks / /today 同容忍策略）。给 owner audit "我哪
    /// 些任务卡住了 / 卡在等什么" 用。
    Blocked,
    /// `/forks <title>` —— 反向 audit：列被 `[blockedBy: <title>]` 引用的 active
    /// task 们 — 让 owner 知道「这条 task 解锁后会让谁动起来」。与 /blocked
    /// （列被卡的）对偶。空 title → handler 走 missing-arg；title resolve 三
    /// 层（数字 index → fuzzy → 错误候选）与 /done /cancel /show 同源。
    Forks { title: String },
    /// `/blocked_by <title>` —— 单条 task 的 blocker audit：列 title 的
    /// `[blockedBy: ...]` markers 中**仍未解决**的 blocker（即仍 active
    /// 的引用对象）。与 /blocked（全 chat 视图）对比 — 那个跨任务列所
    /// 有被卡的，本命令聚焦单条「我这条卡在等什么」。与 /forks 反向 —
    /// /forks 列「谁等我」，/blocked_by 列「我等谁」。空 title → handler
    /// 走 missing-arg；title resolve 三层。
    BlockedBy { title: String },
    /// `/snoozed` —— 列出本 chat 派单中当前在 `[snooze: …]` 中的 task + 显
    /// 还多久醒。与 /silenced / /pinned 对偶。无参；多余尾部忽略。owner 想
    /// audit "我哪些任务被暂存了 / 还多久回到队列" 用。
    Snoozed,
    /// `/mute [N]` —— 临时静音 proactive 主动开口 N 分钟（缺省 30；0 = 解
    /// 除）。复用 `proactive::set_mute_minutes` 同后端 — 与桌面 PanelDebug
    /// "⚙️ mute" 按钮等价。让 owner 在 TG 上"嘿宠物先安静半小时"一句话搞定。
    /// clamp 0..=10080（≤ 7 天）。
    Mute { minutes: i64 },
    /// `/snooze_until <title> <HH:MM>` —— 把任务 snooze 到指定本地时
    /// 刻（与 /sleep_until 对偶 — 那个静音 proactive 整体到 HH:MM，本
    /// 命令仅 snooze 单条 task 到 HH:MM）。空 title / 时刻解析失败由
    /// handler 走 usage hint。HH:MM 解析 / 跨日规则与 /sleep_until 一致
    /// （目标 ≤ now 落明日同时刻）。
    SnoozeUntil {
        title: String,
        time: Option<(u8, u8)>,
    },
    /// `/sleep_until <HH:MM>` —— 静音到指定本地时刻（与 /mute N 互补；
    /// 「安静到 8 点」更自然）。raw arg 由 handler 解析；目标时刻 ≤ now
    /// → 落到明日同时刻（owner 说"早上 8 点"，凌晨 1 点收到 → 视为今早
    /// 8:00 即可，否则要 24h+ 反直觉）。clamp 1..=10080 分钟。
    SleepUntil { raw: String },
    /// `/note <text>` —— 把任意文本作 general memory item 存（owner 在外
    /// 面随手"记一笔"）。title 自动生成 `note-YYYY-MM-DDTHH-MM-SS`（秒级
    /// 唯一）；description = trim 后的 text。空 text → missing-arg friendly
    /// hint。与 桌面 PanelMemory "新建 general item" 同后端，状态一致。
    Note { text: String },
    /// `/digest [N]` —— 最近 N 条 done task 标题 + [result:] 摘要一行式
    /// dump。与 /recent 只显标题互补 — owner 想"扫读最近做了啥 + 产物"
    /// 时用 /digest，纯标题用 /recent。N 缺省 5，clamp 1..=20。
    Digest { n: u32 },
    /// `/cancel_all_error confirm` —— 批量 cancel 本 chat 所有 error 状态
    /// 的任务。必须带 `confirm` token 防误触（与 /reset 不同 — reset 走
    /// 单击但语义轻，本命令一次破坏 N 条 task 状态）。`confirmed` 字段
    /// 由 parser 据 trailing token 决定；handler 在 !confirmed 时走 usage
    /// hint 要求 token，confirmed 时执行批量 cancel + 返计数 reply。
    CancelAllError { confirmed: bool },
    /// `/promote_all_p7 confirm` —— 紧急 sprint 模式：把本 chat 所有 active
    /// (pending / error) task priority +1 到 P7 上限。已 ≥ P7 的不动，已
    /// done / cancelled 跳过。与 /cancel_all_error 同 confirm 模板 — 必须
    /// 带 confirm token 防误触（一次改 N 条 priority）。仅 owner 在突发
    /// deadline / sprint 收尾时用 — 把所有挂着的任务都拉到高优让 LLM
    /// 立即优先。`confirmed` 字段 parser 决定；handler 在 !confirmed 时
    /// 走 usage hint，confirmed 时执行批量 +1 + 返计数 reply。
    PromoteAllP7 { confirmed: bool },
    /// `/touch_all_p7 confirm` —— 批量 touch 所有 P7+ active task — 让
    /// pet 立即重新关注高优清单。与 /promote_all_p7 互补：那个升 priority
    /// 让低优变高优；本命令仅刷 updated_at 让本已 P7+ 的"挂着没动"task
    /// 重新冒头 proactive 选单。与 /cancel_all_error 同 confirm token
    /// 防误触模板。已 done / cancelled 跳过；priority < 7 跳过。
    TouchAllP7 { confirmed: bool },
    /// `/consolidate_now confirm` —— TG 端手动触发一次 consolidate sweep
    /// （与桌面 PanelMemory「立即整理」/ PanelDebug「🧹 force consolidate」
    /// 同后端 trigger_consolidate）。consolidate 是 LLM-heavy 操作（多
    /// 步 sweep + LLM call，~30s..2min；烧 token），必须带 `confirm` token
    /// 防误触。无 confirm → usage hint；confirmed → 跑后返摘要文案
    /// （含 elapsed_ms / LLM summary snippet）。owner 在 sprint / 整理
    /// 后想立即 audit consolidate 行为时用，不必切桌面。
    ConsolidateNow { confirmed: bool },
    /// `/pin_all_p7 confirm` —— 批量给本 chat 所有 P7+ active task（pending
    /// / error）加 `[pinned]` marker — sprint 收尾「把高优清单全钉住」一
    /// 键。与 /touch_all_p7（刷 updated_at）/ /promote_all_p7（升 priority）
    /// 组成 P7+ 批量族。已 [pinned] 跳过避免无意义写；priority < 7 跳过。
    /// confirm token 防误触模板与族内其他批量命令一致。
    PinAllP7 { confirmed: bool },
    /// `/promote <title>` —— priority +1（clamp 9）— 一步操作不必算具体 P
    /// 值。已是 P9 时不动 + 友好 reply。复用 task_set_priority 后端。空
    /// title → missing-arg。
    Promote { title: String },
    /// `/demote <title>` —— priority -1（clamp 0）— 与 /promote 对偶。已是
    /// P0 时不动 + 友好 reply。复用 task_set_priority 后端保其它 markers
    /// 不动。
    Demote { title: String },
    /// `/feedback <text>` —— owner 主动给 pet 写反馈到 feedback_history.log
    /// （FeedbackKind::Comment 中性 kind）。让 LLM 在下次 proactive cycle
    /// 看到 owner 原话调整行为。正向 / 负向 / 中性建议都可走此入口。空
    /// text → missing-arg hint。
    Feedback { text: String },
    /// `/transient <text> [minutes]` —— 写一条 N 分钟有效的 transient_note
    /// 给宠物（owner 临时上下文如"我在开会"、"集中写文档不要打扰"、
    /// "今晚 9 点后回我"等）。与 /note（写 general memory 永久存盘）/
    /// /reflect（ai_insights）/ /feedback（feedback_history.log 改行为）
    /// 三个写入命令对偶 —— 本命令**不存盘**，只挂当前 in-memory，到时
    /// 自动清除（复用 proactive::set_transient_note）。minutes 缺省 60；
    /// clamp 1..=10080（≤ 7 天）。空 text → missing-arg hint。
    /// 与 /mute 区别：/mute 直接静音 proactive；/transient 不阻塞，只
    /// 加上下文让 pet 开口时读到这条 [临时指示]。
    Transient { text: String, minutes: i64 },
    /// `/feedback_history [N]` —— 列最近 N 条 feedback_history.log 条目
    /// （含 owner 写过的 /feedback comment + bubble dismiss / 主动点赞
    /// / 沉默忽略 等系统记录的隐性反馈）。让 owner audit "我给 pet 留
    /// 过什么 / pet 接收了哪些信号"。与 /feedback（写）对偶。N 缺省
    /// 5，clamp 1..=20（与 /recent / /digest 同上限）。
    FeedbackHistory { n: u32 },
    /// `/silent_all [minutes]` —— 批量给 butler_tasks 加 [silent] marker
    /// N 分钟，N 后 backend tokio timer 自动撤回。与桌面 PanelMemory
    /// 「⏸ 全部 silent 1h」按钮（iter #366，frontend timer）对偶 — 让
    /// 手机端 owner 开会 / 集中写作时一键挡住 task picker。minutes 缺
    /// 省 60；0 = 立即释放当前 active 窗口（与 /mute 0 同协议）；clamp
    /// 0..=10080。与 /mute 区别：mute 让 pet 整体不开口；本命令只清
    /// 空 task 候选池，pet 仍可主动聊天。
    SilentAll { minutes: i64 },
    /// `/alarms [N]` —— 列最近 N 条 todo 段 pending reminders（含
    /// `[remind: ...]` 协议条目）— 目标时刻 + 剩余分钟。与桌面
    /// PanelMemory ⏰ alarm chip（iter #372）对偶 audit — 手机端
    /// 一眼看 "我还设了哪些一次性提醒、何时到点"。N 缺省 5，clamp
    /// 1..=20。按 target 升序排（最近 fire 在前）；已过期 entry
    /// 也显（"已逾期 N 分"）便于 owner 知道哪些被 LLM 错过。
    Alarms { n: u32 },
    /// `/recent_chats [N]` —— 列最近 N 条 active session 内 user ↔
    /// pet 聊天往返（user / assistant items，过滤 tool calls）。手机
    /// 端回顾上下文 — owner 想"我刚才让 pet 做啥来着" 不必回桌面
    /// 滚 ChatMini。N 缺省 5，clamp 1..=20。session 级 updated_at 一
    /// 起呈现（per-item ts 不在后端 schema 里，session 级时刻是最接
    /// 近的"何时活跃过"信号）。
    RecentChats { n: u32 },
    /// `/aware` —— pet 自述当前感知到的上下文：transient_note、active
    /// butler_task 数、心情 emoji + text、当前时间、陪伴天数。一句话
    /// debug pet 决策上下文（"为啥它没主动开口 / 选了那条 task"）。
    /// 与 /now（一行时间快查）/ /whoami（多行画像）互补 —— /aware
    /// 是"pet 当前感知"snapshot，含 transient_note 这条 /now 不显的
    /// 关键调度信号。无参；多余尾部一律忽略。
    Aware,
    /// `/here` —— owner 视角 dump：列当前 owner 留下的状态信号
    /// transient_note + mute 剩余 + 最近 feedback band（high_negative /
    /// low_negative / mid / insufficient_samples）。与 /aware 对偶 —
    /// /aware 看 pet 感知到的，/here 看 owner 输入侧。让 owner audit
    /// "我现在给 pet 什么信号" — 若 high_negative + 还没 mute，可主
    /// 动决定"让 ta 安静会儿"。无参；多余尾部一律忽略。
    Here,
    /// `/tag <name>` —— 列含某 #tag 的所有 task（含 status emoji + due）。
    /// name 可带 / 不带 `#` 前缀，case-insensitive 匹配。与桌面 PanelTasks
    /// #tag chip click filter 对偶 audit。空 name → missing-arg。无命中
    /// → 友好兜底 + 提示 /tags 看所有可用 tag 名。
    Tag { name: String },
    /// `/tags_for <title>` —— 单条 task 的 #tags 清单（与 /tags 全列表
    /// 对偶但单条聚焦）。owner 想「这条 task 标了哪些 tag」audit 单点
    /// 入口。空 title → missing-arg；title resolve 与 /show 同三层。
    TagsFor { title: String },
    /// `/touch <title>` —— 刷 updated_at 不改内容 — 让老 task 重新冒
    /// 头 proactive 选单。机制：rewrite 同 description → memory_edit 自
    /// 动 stamp updated_at 到 now（与 task_skip_once 同 backend helper
    /// 但 decision_log 标 TaskTouch 做 audit 区分）。done / cancelled
    /// 拒（终态 task touch 无意义）。空 title → missing-arg；title
    /// resolve 与 /done /cancel /show 同三层。
    Touch { title: String },
    /// `/edit_due <title> <preset>` —— 改任务 due 为 preset 解出的时刻。
    /// preset 接 tonight/tomorrow/monday/next_monday/+30m/+2h/+1d/clear 等
    /// 友好词 — 免手敲 ISO 日期。preset 是 last whitespace token，余作
    /// title（与 /pri / /promote / /demote 同 parser 模板）。空 title /
    /// 无法识别的 preset → usage hint。复用 task_set_due 后端。
    EditDue {
        title: String,
        preset: Option<EditDuePreset>,
    },
    /// `/pri <title> <N>` —— 单改任务 priority（0..=9），不走 /edit 全量覆写。
    /// title 含空格 / 中文标点不被破坏 — parser 把"最后一个 whitespace
    /// token 作为 N 解析为 u8 ≤ 9"，剩余作 title。N 缺失 / 越界 → handler
    /// 走 usage hint；title 缺失 → missing-arg。
    Pri {
        title: String,
        priority: Option<u8>,
    },
    /// `/streak` —— 本聊天连续有 done 完成的天数 + 近 7 天 / 近 30 天 done
    /// 总数。给 owner audit 「我最近完成节奏怎么样 / 有没有 streak 在保」。
    /// streak 末端：今日有 done → 今日；否则若 yesterday 有 → yesterday；
    /// 否则 streak = 0。无参；多余尾部一律忽略。
    Streak,
    /// `/yesterday` —— 列昨日 done 任务标题 + `[result:]` 摘要。与 `/today`
    /// 互补 —— 那个看今日 due/done 切片，这个 audit 昨日产出。无参；多余
    /// 尾部一律忽略。空 → "昨日无完成记录"。
    Yesterday,
    /// `/today_done` —— 列今日 done 任务标题 + `[result:]` 摘要。与 /today
    /// 互补 —— 那个含 due 段 + done 段（双视图但 done 段无 result 摘要），
    /// 本命令纯 done 切片 + result 一行式（与 /yesterday 同模板但 scope 是
    /// 今日）。owner 想"今天做完啥 + 各条产物"一行扫读时用。无参；多余
    /// 尾部一律忽略。
    TodayDone,
    /// `/quick <text>` —— 与 `/task` 同后端但 reply 极短（仅 ✓ + title），
    /// 适合 owner 想"快速 dump 个 task 不被长 reply 打扰"的场景。priority
    /// 始终 P3（不解析 !! / !!!）— 想精细化走 `/task !!` 或 `/task !!!`。
    /// 空 text 由 handler 走 missing-argument hint。
    Quick { text: String },
    /// `/sleep` —— 一键让宠物 mute 8 小时 + 友好"晚安"语气 reply。比手敲
    /// `/mute 480` 更直觉 — owner 睡前 / 长会议时一句话搞定。无参；多余
    /// 尾部忽略。内部走 `set_mute_minutes(480)` 同后端，与 /mute 等价但
    /// 文案温和。
    Sleep,
    /// `/random` —— 从本 chat 派单的 active 任务（pending / error）里随机抽
    /// 一条让宠物推荐 — 给 owner "选择困难" 时让 pet 决定下一步。pure 实现
    /// 走调用方传入的 `index_seed: usize` 模 candidate.len() 索引，便于
    /// 单测稳定；bot.rs 端用 system time nanos 当 seed 拿到非确定性体验。
    /// 无 active 任务 → 兜底文案。无参；多余尾部忽略。
    Random,
    /// `/last` —— 显本 chat 派单中最近创建的一条 task：title + status emoji +
    /// 相对创建时间 + raw_description 前 200 字符预览。让 owner 在 TG 端
    /// 闪查"我刚 enqueue 的那条对不对"，不必走 /tasks 全表扫。无参；多
    /// 余尾部忽略。本 chat 派单空 → 友好兜底文案。
    Last,
    /// `/now` —— 一句话快速状态 check：当前本地时间 + tz 偏移 + 陪伴天数 +
    /// 当前 mood emoji + 心情文本。比 `/whoami`（多行画像）更简短，适合
    /// owner "现在几点 / 宠物啥状态" 闪查。无参；多余尾部一律忽略。
    Now,
    /// `/last_speech` —— 显 pet 最近一条主动开口（speech_history.log 末
    /// 条），含 ts + text + 相对时间（"N 分钟前" / "N 小时前"）。与
    /// ChatMini 顶部「⏱ pet 沉默 N 分」chip 对偶 — 那个显沉默时长，本
    /// 命令显具体说了什么。空 history 时友好兜底。无参；多余尾部忽略。
    LastSpeech,
    /// `/show_speech [N]` —— 显 pet 最近 N 条主动开口（speech_history.log
    /// 末 N 条，倒序最新在前）。与 /last_speech 单条对偶。N 缺省 5；
    /// clamp 1..=20（与 /recent / /digest 等 N-cap 命令统一上限）；非
    /// 数字尾部一律忽略走默认。
    ShowSpeech { n: u32 },
    /// `/show <title>` —— 显示指定任务的 raw_description（含全部 markers）
    /// + detail.md 内容预览（前 300 字符），让 owner 在 TG 端 audit 单条
    /// 任务详情不必回桌面。空 title 走 missing-arg；title resolve 三层
    /// （数字 index → fuzzy → 错误候选）与 /done /cancel /edit 同源。
    Show { title: String },
    /// `/peek <title>` —— 一行紧凑视图：status emoji + 标题 + schedule 摘要
    /// （every / once / deadline 解析）+ 关键 markers（📌 pinned / 🔇 silent
    /// / 💤 snooze / 🔒 blockedBy / P{priority}）。与 /show 显完整 raw +
    /// detail 互补 — owner 想"快瞄一眼这条状态"用 /peek，要看完整内容走
    /// /show。空 title 走 missing-arg；title resolve 三层与 /show 同源。
    Peek { title: String },
    /// `/dup <title>` —— 复制 task 为新 P3 实例，新 title 自动加 `(副本)`
    /// 后缀（unique-filename 兜底由 memory_edit 处理 — 多次 dup 同源会
    /// 变 `_1` / `_2`）。继承 schedule / pinned / silent / blockedBy /
    /// reminderMin / tags；剥 done / error / result / cancelled / snooze /
    /// origin terminal-state markers — 副本回 pending。priority + due 继承
    /// 源 task。
    Dup { title: String },
    /// `/snippets` —— 列含 `[snippet]` / `[snippet: <label>]` marker 的 task
    /// 一行紧凑视图：title + 可选 label + body 前 80 字预览。让 owner 把
    /// 可复用片段（prompt 模板 / 决策清单 / 常用回复 / 流程 checklist）
    /// 标记后集中 audit — 用 /show 看完整内容、/dup 克隆改装。
    Snippets,
    /// `/recent_events <title> [N]` —— 给单条 task 最近 N 个 butler_history
    /// 事件的紧凑一行视图（与 /timeline 完整视图互补）。owner 想「这条
    /// task 最近发生了啥」TL;DR 时用本命令；想看完整演化走 /timeline。
    /// N 缺省 5；clamp 1..=20（与 /recent / /digest / /show_speech 同上限
    /// 协议）。空 title → missing-arg；title resolve 三层与 /show 同源。
    RecentEvents { title: String, n: u32 },
    /// `/touched_today` —— 列今日 updated_at 命中的 task（任意状态），按
    /// 时间倒序。与 /today_done（done-only）/ /today（today due）互补 —
    /// 本命令回答「我今天动过哪些 task」（含 promote / snooze / silent
    /// 等 owner-action 引发的 update + LLM update）。无参。
    TouchedToday,
    /// `/touched_yesterday` —— 昨日 updated_at 命中的 task — /touched_today
    /// 的昨日对偶。复盘视角：「昨天我动过哪些」。无参。
    TouchedYesterday,
    /// `/touched_thisweek` —— 本周（自周一 00:00 起到 now）updated_at
    /// 命中 task — 周维度复盘。与 /touched_today / /touched_yesterday
    /// 三件套（today × yesterday × thisweek）。无参。
    TouchedThisweek,
    /// `/oldest_done [N]` —— 列**最早**完成的 N 条 done task（按 updated_at
    /// asc）。与 /recent done（最近完成）反向 — owner 想看「这条任务我做
    /// 了多久 / 哪些是 ancient backlog 终于完成」时用。N 缺省 5；clamp
    /// 1..=20（与 /recent / /digest 同协议）。
    OldestDone { n: u32 },
    /// `/edit_title <title> :: <new title>` —— 仅改 task 标题不动 description
    /// / detail.md / markers。前端 PanelTasks 已有 double-click inline
    /// rename；本命令是 TG 端对偶。复用既有 `memory_rename` Tauri 命令
    /// — index 项改名 + .md 文件 move + 重名 `_N` 冲突兜底。
    EditTitle {
        title: String,
        new_title: String,
    },
    /// `/cascade_rename <title> :: <new title>` —— 与 /edit_title 同模板但
    /// 额外扫所有 categories 的 detail.md 文件把 `「<old>」` token 替换为
    /// `「<new>」`。让 cross-doc task ref 自动跟随 rename，避免 owner 在
    /// 多份 detail.md 内手动维护引用。reply 含 rename 主操作 + cascade
    /// 命中文件数。
    CascadeRename {
        title: String,
        new_title: String,
    },
    /// `/mute_today` —— 一键静音到本地午夜（next 00:00），免 owner 算
    /// 「还多少分钟到午夜」。与 /mute N（相对分钟）/ /sleep_until <HH:MM>
    /// （任意目标时刻）互补 — 本命令是「今夜不打扰」常用预设。无参。
    MuteToday,
    /// `/digest_yesterday [N]` —— 昨日 done task 标题 + [result:] 一行式 —
    /// /digest 的昨日对偶（那个是「最近 N 条 done」按 updated_at 倒序，
    /// 本命令限定昨日 calendar day）。N 缺省 5，clamp 1..=20。
    DigestYesterday { n: u32 },
    /// `/digest_thisweek [N]` —— 本周 done task 标题 + [result:] 一行式
    /// — /digest 的本周对偶（周报场景）。N 缺省 5，clamp 1..=20。
    DigestThisweek { n: u32 },
    /// `/search_today <kw>` —— 限定今日 updated_at 的 task 内 fuzzy 搜
    /// title / raw_description（case-insensitive）。/find（全量）/
    /// /touched_today（无 kw，列今日全）/ 本命令（今日 + kw）三件套。
    /// 「今天我做的与 X 相关的」精准 audit 入口。
    SearchToday { keyword: String },
    /// `/search_yesterday <kw>` —— /search_today 的昨日对偶。「昨天我
    /// 做的与 X 相关的」精准 audit — 早会回顾 / 复盘场景。
    SearchYesterday { keyword: String },
    /// `/search_thisweek <kw>` —— /search_today 的本周对偶。「本周与
    /// X 相关的」精准 audit — 周报 / 周复盘场景。完成 search × date
    /// 三件套（today × yesterday × thisweek）。
    SearchThisweek { keyword: String },
    /// `/find_in_detail_today <kw>` —— /find_in_detail 的今日切片：
    /// 限定今日 updated_at 的 task 扫 detail.md 内容。「我今天在某主题
    /// 写过什么笔记」精准 audit — 日记 / 进度笔记复盘场景。
    FindInDetailToday { keyword: String },
    /// `/find_in_detail_yesterday <kw>` —— /find_in_detail_today 的昨日
    /// 对偶 — 限昨日 updated_at task 的 detail.md 内容搜。复盘视角。
    FindInDetailYesterday { keyword: String },
    /// `/alarms_today` —— /alarms 的今日切片：仅显本地今日触发的 reminder。
    /// 让 owner 一眼看「今天还会响哪些 alarm / 已逾期的还没消」。无参 —
    /// 今日范围天然小，不需 N cap。
    AlarmsToday,
    /// `/alarms_thisweek` —— /alarms_today 的本周对偶 — 本周内触发的
    /// reminder 集中视图。无参，与 alarms 系列同。
    AlarmsThisweek,
    /// `/peek_pinned` —— /pinned 的紧凑版 + /peek 的批量版：列所有 pinned
    /// task 一行紧凑视图（status emoji + 标题 + schedule + 关键 markers）。
    /// 让 owner 一眼批量看「我钉的 N 条状态如何」— 比 /pinned（仅标题）
    /// 信息密度高，比 /tasks（全量）scope 窄。无参。
    PeekPinned,
    /// `/random_pinned` —— /random 的 pinned 子集 — 从 pinned task 里随
    /// 机抽 1 条让 pet 推荐。owner「这几条钉的都重要 / 不知先做哪条」
    /// 时让 pet 决定。无参。
    RandomPinned,
    /// `/cat_top [N]` —— 按 cat items 总量 desc 列前 N — 跨 cat 容量对比
    /// audit。N 缺省 5，clamp 1..=20。
    CatTop { n: u32 },
    /// `/audit_summary` —— 单命令聚合 audit 信号 — sprint kickoff
    /// 一键视图：pin streak / 当前 pinned 数 / idle 7d+ 数 / 今日
    /// touched 数 / 近期 rename 数。每行单数 + 对应单命令的 deep dive
    /// 入口。无参。
    AuditSummary,
    /// `/help_table [family]` —— 按 audit family 分组的命令速查表。
    /// - 无参 → 全表（13 family + 每行命令清单）— navigation aid
    /// - 有参 → 仅该 family 详细 list（每行 cmd + 一行描述）— family
    ///   focused 视图，省 owner 翻全表
    /// family 关键字 case-insensitive，常用：pin / cat / rename / idle /
    /// streak / find / tag / speech / alarm / status / batch / system。
    HelpTable { family: Option<String> },
    /// `/recent_pins [N]` —— 列近 N 条 [pinned] sighting event（ts + title）。
    /// 每 title 取 history 内最早 [pinned] sighting（= owner 首次钉它）后按
    /// ts desc 排。N 缺省 5，clamp 1..=20。
    RecentPins { n: u32 },
    /// `/idle_7d` —— 列 pending 且 updated_at ≥ 7 天前的 task — stale backlog
    /// audit。PanelTasks「💤 N 条 7d+ idle」chip 的 TG 端对偶。无参，按 idle
    /// 天数 desc 排（最老 stale 在上 — owner 先看最该处理的）。
    Idle7d,
    /// `/tags_today` —— /tags 的今日切片：仅列今日 updated_at 的 task
    /// 含的 #tag 计数。让 owner 看「今天我在做什么主题」audit。无参。
    TagsToday,
    /// `/tags_yesterday` —— /tags_today 的昨日对偶 — 昨日 task 含的
    /// #tag 计数。复盘视角。无参。
    TagsYesterday,
    /// `/tags_thisweek` —— /tags_today 的本周对偶 — 本周（自周一起）
    /// task 含的 #tag 计数。周报场景。无参。
    TagsThisweek,
    /// `/timeline <title>` —— 时间线视图：扫 butler_history.log 取这条
    /// task 的所有 create / update / delete 事件，按时序展开每个事件含
    /// 哪些"状态变化"markers（[done] / [error:] / [snooze:] / [result:]
    /// / [cancelled:] / [pinned] / [silent] / [blockedBy:] / [archived:]）—
    /// 让 owner audit "这条 task 经历了啥"。与 /show 显当前 snapshot 互
    /// 补；本命令是 historical 视角。空 title 走 missing-arg；title
    /// resolve 与 /done /cancel /show 同三层。
    Timeline { title: String },
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
    /// `/swap_priority <a> :: <b>` —— 互换两 task 的 priority，与 /pri 单
    /// 改互补（sprint 重组场景 — owner 想「A 和 B 的优先级换一下」一步
    /// 完成不必算具体 P 值）。`::` 是必填 separator（让 title 含空格 /
    /// 中文标点也能精确切，与 /edit 同模板）。任一端 trim 后为空 → handler
    /// 走 missing-arg；任一 title 找不到 → handler 走错误反馈。复用
    /// task_set_priority 后端，对称写两次。
    SwapPriority { title_a: String, title_b: String },
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
    /// `/recall <query>` — GOAL 038：跨数据源 retrieval（debug + power user
    /// 入口）。等价 LLM tool `retrieve_memory` 直调，top_n=10 / sources=all。
    Recall { query: String },
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
            TgCommand::Buckets => "buckets",
            TgCommand::Mood => "mood",
            TgCommand::Whoami => "whoami",
            TgCommand::Recall { .. } => "recall",
            TgCommand::Snooze { .. } => "snooze",
            TgCommand::Unsnooze { .. } => "unsnooze",
            TgCommand::Pin { .. } => "pin",
            TgCommand::Unpin { .. } => "unpin",
            TgCommand::Pinned => "pinned",
            TgCommand::PinnedDue => "pinned_due",
            TgCommand::Silent { .. } => "silent",
            TgCommand::Unsilent { .. } => "unsilent",
            TgCommand::Silenced => "silenced",
            TgCommand::Markers => "markers",
            TgCommand::Tags => "tags",
            TgCommand::Today => "today",
            TgCommand::Recent { .. } => "recent",
            TgCommand::OldestN { .. } => "oldest_n",
            TgCommand::ActiveRecent { .. } => "active_recent",
            TgCommand::Find { .. } => "find",
            TgCommand::FindInDetail { .. } => "find_in_detail",
            TgCommand::Blocked => "blocked",
            TgCommand::Forks { .. } => "forks",
            TgCommand::BlockedBy { .. } => "blocked_by",
            TgCommand::Snoozed => "snoozed",
            TgCommand::Mute { .. } => "mute",
            TgCommand::SleepUntil { .. } => "sleep_until",
            TgCommand::SnoozeUntil { .. } => "snooze_until",
            TgCommand::Note { .. } => "note",
            TgCommand::Digest { .. } => "digest",
            TgCommand::Edit { .. } => "edit",
            TgCommand::SwapPriority { .. } => "swap_priority",
            TgCommand::Reflect { .. } => "reflect",
            TgCommand::Due { .. } => "due",
            TgCommand::Show { .. } => "show",
            TgCommand::Peek { .. } => "peek",
            TgCommand::Dup { .. } => "dup",
            TgCommand::Snippets => "snippets",
            TgCommand::RecentEvents { .. } => "recent_events",
            TgCommand::TouchedToday => "touched_today",
            TgCommand::TouchedYesterday => "touched_yesterday",
            TgCommand::TouchedThisweek => "touched_thisweek",
            TgCommand::OldestDone { .. } => "oldest_done",
            TgCommand::EditTitle { .. } => "edit_title",
            TgCommand::CascadeRename { .. } => "cascade_rename",
            TgCommand::MuteToday => "mute_today",
            TgCommand::DigestYesterday { .. } => "digest_yesterday",
            TgCommand::DigestThisweek { .. } => "digest_thisweek",
            TgCommand::SearchToday { .. } => "search_today",
            TgCommand::SearchYesterday { .. } => "search_yesterday",
            TgCommand::SearchThisweek { .. } => "search_thisweek",
            TgCommand::FindInDetailToday { .. } => "find_in_detail_today",
            TgCommand::FindInDetailYesterday { .. } => "find_in_detail_yesterday",
            TgCommand::AlarmsToday => "alarms_today",
            TgCommand::AlarmsThisweek => "alarms_thisweek",
            TgCommand::PeekPinned => "peek_pinned",
            TgCommand::RandomPinned => "random_pinned",
            TgCommand::Idle7d => "idle_7d",
            TgCommand::RecentPins { .. } => "recent_pins",
            TgCommand::HelpTable { .. } => "help_table",
            TgCommand::AuditSummary => "audit_summary",
            TgCommand::CatTop { .. } => "cat_top",
            TgCommand::TagsToday => "tags_today",
            TgCommand::TagsYesterday => "tags_yesterday",
            TgCommand::TagsThisweek => "tags_thisweek",
            TgCommand::Timeline { .. } => "timeline",
            TgCommand::Now => "now",
            TgCommand::LastSpeech => "last_speech",
            TgCommand::ShowSpeech { .. } => "show_speech",
            TgCommand::Last => "last",
            TgCommand::Random => "random",
            TgCommand::Sleep => "sleep",
            TgCommand::Quick { .. } => "quick",
            TgCommand::Yesterday => "yesterday",
            TgCommand::TodayDone => "today_done",
            TgCommand::Streak => "streak",
            TgCommand::Pri { .. } => "pri",
            TgCommand::Feedback { .. } => "feedback",
            TgCommand::Transient { .. } => "transient",
            TgCommand::FeedbackHistory { .. } => "feedback_history",
            TgCommand::SilentAll { .. } => "silent_all",
            TgCommand::Alarms { .. } => "alarms",
            TgCommand::RecentChats { .. } => "recent_chats",
            TgCommand::Aware => "aware",
            TgCommand::Here => "here",
            TgCommand::Tag { .. } => "tag",
            TgCommand::TagsFor { .. } => "tags_for",
            TgCommand::Touch { .. } => "touch",
            TgCommand::EditDue { .. } => "edit_due",
            TgCommand::CancelAllError { .. } => "cancel_all_error",
            TgCommand::PromoteAllP7 { .. } => "promote_all_p7",
            TgCommand::TouchAllP7 { .. } => "touch_all_p7",
            TgCommand::PinAllP7 { .. } => "pin_all_p7",
            TgCommand::ConsolidateNow { .. } => "consolidate_now",
            TgCommand::Promote { .. } => "promote",
            TgCommand::Demote { .. } => "demote",
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
            | TgCommand::SearchToday { keyword: title }
            | TgCommand::SearchYesterday { keyword: title }
            | TgCommand::SearchThisweek { keyword: title }
            | TgCommand::FindInDetail { keyword: title }
            | TgCommand::FindInDetailToday { keyword: title }
            | TgCommand::FindInDetailYesterday { keyword: title }
            | TgCommand::Tag { name: title }
            | TgCommand::TagsFor { title }
            | TgCommand::Touch { title }
            | TgCommand::Note { text: title }
            | TgCommand::Reflect { text: title }
            | TgCommand::Show { title }
            | TgCommand::Peek { title }
            | TgCommand::Dup { title }
            | TgCommand::RecentEvents { title, .. }
            | TgCommand::Timeline { title }
            | TgCommand::Forks { title }
            | TgCommand::BlockedBy { title }
            | TgCommand::Quick { text: title }
            | TgCommand::Pri { title, .. }
            | TgCommand::EditDue { title, .. }
            | TgCommand::Feedback { text: title }
            | TgCommand::Transient { text: title, .. }
            | TgCommand::Promote { title }
            | TgCommand::Demote { title }
            | TgCommand::SleepUntil { raw: title }
            | TgCommand::SnoozeUntil { title, .. } => title.as_str(),
            TgCommand::Edit { title, .. } => title.as_str(),
            TgCommand::EditTitle { title, .. } => title.as_str(),
            TgCommand::CascadeRename { title, .. } => title.as_str(),
            TgCommand::SwapPriority { title_a, .. } => title_a.as_str(),
            TgCommand::Task { title, .. } => title.as_str(),
            TgCommand::Tasks
            | TgCommand::Pinned
            | TgCommand::PinnedDue
            | TgCommand::Silenced
            | TgCommand::Markers
            | TgCommand::Snippets
            | TgCommand::TouchedToday
            | TgCommand::TouchedYesterday
            | TgCommand::TouchedThisweek
            | TgCommand::MuteToday
            | TgCommand::Tags
            | TgCommand::Stats
            | TgCommand::Buckets
            | TgCommand::Mood
            | TgCommand::Whoami
            | TgCommand::Today
            | TgCommand::Recent { .. }
            | TgCommand::RecentPins { .. }
            | TgCommand::CatTop { .. }
            | TgCommand::HelpTable { .. }
            | TgCommand::AuditSummary
            | TgCommand::OldestN { .. }
            | TgCommand::OldestDone { .. }
            | TgCommand::ActiveRecent { .. }
            | TgCommand::Blocked
            | TgCommand::Snoozed
            | TgCommand::Mute { .. }
            | TgCommand::Digest { .. }
            | TgCommand::DigestYesterday { .. }
            | TgCommand::DigestThisweek { .. }
            | TgCommand::FeedbackHistory { .. }
            | TgCommand::SilentAll { .. }
            | TgCommand::Alarms { .. }
            | TgCommand::AlarmsToday
            | TgCommand::AlarmsThisweek
            | TgCommand::PeekPinned
            | TgCommand::RandomPinned
            | TgCommand::Idle7d
            | TgCommand::TagsToday
            | TgCommand::TagsYesterday
            | TgCommand::TagsThisweek
            | TgCommand::RecentChats { .. }
            | TgCommand::Due { .. }
            | TgCommand::Now
            | TgCommand::LastSpeech
            | TgCommand::ShowSpeech { .. }
            | TgCommand::Aware
            | TgCommand::Here
            | TgCommand::Last
            | TgCommand::Random
            | TgCommand::Sleep
            | TgCommand::Yesterday
            | TgCommand::TodayDone
            | TgCommand::Streak
            | TgCommand::CancelAllError { .. }
            | TgCommand::PromoteAllP7 { .. }
            | TgCommand::TouchAllP7 { .. }
            | TgCommand::PinAllP7 { .. }
            | TgCommand::ConsolidateNow { .. }
            | TgCommand::Reset
            | TgCommand::Version
            | TgCommand::Help { .. }
            | TgCommand::Recall { .. }
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
            ("buckets", "Priority bucket counts (P0..P9) for active tasks — complement to /stats status view"),
            ("done", "Mark a task as done"),
            ("cancel", "Cancel a task"),
            ("retry", "Retry a failed task"),
            ("snooze", "Snooze a task (30m / 2h / tonight / tomorrow / monday)"),
            ("unsnooze", "Clear a task's snooze"),
            ("pin", "Mark a task as pinned (key task)"),
            ("unpin", "Clear a task's pinned mark"),
            ("pinned", "List currently pinned tasks dispatched from this chat"),
            ("pinned_due", "List active tasks that are BOTH pinned AND have due — high-priority deadline audit"),
            ("silent", "Mark a task as [silent] (LLM won't auto-pick; manual fire still works)"),
            ("unsilent", "Clear a task's [silent] mark"),
            ("silenced", "List currently silent tasks dispatched from this chat"),
            ("markers", "List all owner-intent markers in one shot (pinned + silent)"),
            ("tags", "List all custom #tags used in this chat's tasks with counts (top 15)"),
            ("mood", "Show the pet's current mood"),
            ("whoami", "Show pet's whoami digest (companionship / mood / persona / top tools)"),
            ("today", "Today's due / done task titles"),
            ("now", "One-line status check: time + tz + companionship days + mood emoji"),
            ("last_speech", "Show pet's most recent proactive utterance + ts — pairs with ChatMini's ⏱ silent chip"),
            ("show_speech", "List recent N proactive utterances (default 5, cap 20) — generalizes /last_speech"),
            ("last", "Show the most recently created task (this chat) with raw description preview"),
            ("random", "Pick a random active (pending / error) task — for owner's choice paralysis moments"),
            ("sleep", "Mute proactive for 8 hours with a friendly good-night reply (= /mute 480)"),
            ("quick", "Silently create a P3 task with minimal ack — for brain-dump without long reply"),
            ("yesterday", "List yesterday's done tasks with result summaries (complement to /today)"),
            ("today_done", "Today's done tasks with [result:] summary one-liner (done-only slice of /today)"),
            ("streak", "Consecutive done-days streak + 7d / 30d done totals"),
            ("pri", "Set a task's priority (0..=9) without rewriting the rest"),
            ("feedback", "Send owner feedback to feedback_history (influences pet's next proactive turn)"),
            ("transient", "Set a transient note for N minutes — in-memory only context for the pet (default 60m, cap 7d)"),
            ("feedback_history", "List recent N feedback_history.log entries (replied / liked / comment / ignored / dismissed / puzzled; default 5, cap 20)"),
            ("silent_all", "Bulk-silence all butler_tasks for N minutes — auto-releases on timer (default 60m; 0 = release now)"),
            ("alarms", "List recent N pending reminders in the todo category with target time + remaining minutes (default 5, cap 20)"),
            ("recent_chats", "List recent N user ↔ pet chat exchanges from the active session (default 5, cap 20)"),
            ("aware", "Pet's current awareness snapshot: transient_note + active tasks + mood emoji + time + companionship days"),
            ("here", "Owner-side signals snapshot: transient_note + mute state + recent feedback band (counterpart to /aware)"),
            ("tag", "List all tasks with a given #tag (exact match, case-insensitive; counterpart to /tags which lists tag names)"),
            ("tags_for", "List the #tags on a specific task (single-task focus; counterpart to /tags whole-chat view)"),
            ("touch", "Bump task's updated_at without changing content — bring an old task back to proactive selection"),
            ("edit_due", "Edit a task's due time using friendly preset (tonight / tomorrow / monday / next_friday / +30m / +2h / clear ...)"),
            ("cancel_all_error", "Batch cancel all error tasks in this chat (requires `confirm` token)"),
            ("promote_all_p7", "Sprint mode: batch +1 priority on all active tasks (clamp 7) — requires `confirm`"),
            ("touch_all_p7", "Batch touch all P7+ active tasks (refresh updated_at) — requires `confirm`"),
            ("pin_all_p7", "Batch pin all P7+ active tasks (add [pinned] marker) — requires `confirm`"),
            ("consolidate_now", "Manually trigger a consolidate sweep — requires `confirm` (LLM-heavy, costs tokens)"),
            ("promote", "Promote a task's priority by +1 (clamped to 9)"),
            ("demote", "Demote a task's priority by -1 (clamped to 0)"),
            ("due", "List pending tasks due in a window (preset: tomorrow / thisweek / nextweek; default tomorrow)"),
            ("recent", "List recent N done tasks (default 5, cap 20)"),
            ("oldest_n", "List oldest N pending tasks (created_at asc) — audit longest-stale backlog"),
            ("active_recent", "List most recently created N active tasks (pending / error, created_at desc) — reverse of /recent"),
            ("find", "Search this chat's tasks by keyword (title / description substring)"),
            ("find_in_detail", "Search this chat's tasks by keyword inside detail.md content (complements /find which scans title/description)"),
            ("show", "Show full raw description (with markers) + detail.md preview of a task"),
            ("peek", "One-line compact view: status + schedule + key markers (complements /show full detail)"),
            ("dup", "Duplicate a task to a new pending instance (preserves schedule / pinned / silent / tags; strips terminal markers)"),
            ("snippets", "List tasks marked [snippet] / [snippet: <label>] — reusable templates / checklists / canned replies"),
            ("recent_events", "Compact last-N butler_history events for a task (default 5, cap 20) — complements /timeline full view"),
            ("touched_today", "List tasks whose updated_at is today (any status) — audit what you moved today; complements /today_done done-only"),
            ("edit_title", "Rename a task: /edit_title <old> :: <new> — preserves description / detail.md / markers"),
            ("touched_yesterday", "Yesterday's counterpart to /touched_today — any-status retrospective audit"),
            ("touched_thisweek", "This week's (Mon 00:00 → now) any-status touched task list — week-scope retrospective"),
            ("oldest_done", "List oldest N done tasks (updated_at asc) — reverse of /recent; longest-running completions"),
            ("cascade_rename", "Rename + auto-update 「<old>」 refs in every detail.md across categories"),
            ("mute_today", "Mute proactive until local midnight — one-shot 'no more pings tonight'"),
            ("digest_yesterday", "Yesterday's done tasks with [result:] summaries (default 5, cap 20) — /digest counterpart"),
            ("digest_thisweek", "This week's done tasks with [result:] summaries (default 5, cap 20) — weekly review"),
            ("search_today", "Search tasks whose updated_at is today by keyword (title / description substring) — narrowed /find"),
            ("search_yesterday", "Yesterday's counterpart to /search_today — yesterday + keyword intersection audit"),
            ("search_thisweek", "This week's counterpart to /search_today — week + keyword intersection (weekly review)"),
            ("find_in_detail_today", "Today's counterpart to /find_in_detail — today task's detail.md content + keyword intersection"),
            ("find_in_detail_yesterday", "Yesterday's counterpart to /find_in_detail_today — detail.md content × yesterday axis"),
            ("alarms_today", "Show today's pending reminders (today slice of /alarms; no N param — today's scope is small)"),
            ("alarms_thisweek", "This week's counterpart to /alarms_today — reminders firing within Mon→now (no N cap)"),
            ("peek_pinned", "All pinned tasks in one-line compact form — /pinned 's denser sibling using /peek 's row format"),
            ("random_pinned", "Pick a random pinned task — /random restricted to pinned subset (decision-fatigue helper)"),
            ("idle_7d", "Pending tasks idle ≥ 7 days (updated_at desc) — stale backlog audit; PanelTasks 💤 chip's TG dual"),
            ("recent_pins", "Recent N pin decisions (per-title earliest [pinned] sighting, desc)"),
            ("help_table", "Audit family-grouped command navigator — sibling to /help (flat list)"),
            ("audit_summary", "Sprint kickoff one-shot — aggregates pin streak / cat / idle / today / 7d-done audit signals"),
            ("cat_top", "Top N cats by total item count — capacity axis (orthogonal to growth/decay activity axis)"),
            ("tags_today", "Today's active #tag counts (today's touched tasks slice of /tags)"),
            ("tags_yesterday", "Yesterday's counterpart to /tags_today — yesterday's touched task tag counts"),
            ("tags_thisweek", "This week's counterpart to /tags_today — week-touched task tag counts"),
            ("timeline", "Timeline view: each butler_history event for a task with state-change markers"),
            ("blocked", "List active tasks blocked by [blockedBy: …] with their unresolved blockers"),
            ("forks", "Reverse: list active tasks that reference [blockedBy: <this>] — unlock impact audit"),
            ("blocked_by", "Focused: list unresolved blockers that <title> is waiting on"),
            ("snoozed", "List tasks currently in [snooze: …] with time until wake"),
            ("mute", "Mute proactive for N minutes (default 30; 0 to clear)"),
            ("sleep_until", "Mute proactive until an absolute local time (HH:MM) — complements /mute N relative minutes"),
            ("snooze_until", "Snooze a task until an absolute local time (HH:MM) — complements /snooze relative presets"),
            ("note", "Save arbitrary text as a general memory item (quick brain-dump)"),
            ("reflect", "Save arbitrary text as an ai_insights memory item (reflection / self-observation)"),
            ("digest", "Recent N done tasks with [result:] summary one-liner (default 5, cap 20)"),
            ("edit", "Overwrite a butler task's description: /edit <title> :: <new desc>"),
            ("swap_priority", "Swap priority of two tasks: /swap_priority <a> :: <b> (sprint reorder)"),
            ("reset", "Clear LLM chat context (keep persona)"),
            ("version", "Show pet app version + SQLite schema version"),
            ("help", "Show command help"),
        ],
        _ => vec![
            ("task", "把单条任务塞进队列（!! P5 / !!! P7）"),
            ("tasks", "列出本会话派出的任务清单"),
            ("stats", "状态计数：待办 / 逾期 / 今日完成 等"),
            ("buckets", "active task 按 priority 分桶计数（P0..P9）— /stats 状态维度的 priority 维度对偶"),
            ("done", "把指定任务标 done"),
            ("cancel", "取消指定任务"),
            ("retry", "把失败任务重置回 pending"),
            ("snooze", "暂停任务（30m / 2h / tonight / tomorrow / monday，缺省 30m）"),
            ("unsnooze", "解除任务暂停"),
            ("pin", "钉住任务（标 [pinned]）"),
            ("unpin", "取消任务钉住（剥 [pinned]）"),
            ("pinned", "列出本聊天派单中所有钉住任务（与桌面「📌 N」chip 同源）"),
            ("pinned_due", "列同时 pinned + 含 due 的 active task（高优截止清单 — 紧急 audit）"),
            ("silent", "标静默（LLM 不主动选；面板 / 手动触发不受影响）"),
            ("unsilent", "解除静默（剥 [silent] marker）"),
            ("silenced", "列出本聊天派单中所有 silent 任务（与「🔇 N silent」面板同源）"),
            ("markers", "一次列出所有 owner-intent markers（pinned + silent）"),
            ("tags", "列本聊天派单中所有用过的 #tag + 各 tag 任务数（top 15）"),
            ("mood", "查看宠物当前心情"),
            ("whoami", "宠物自我介绍（陪伴 / 心情 / 自我画像 / 近常用工具）"),
            ("today", "今日到期 / 已完成的任务标题清单"),
            ("now", "一句话快速状态：当前时间 + 时区 + 陪伴天数 + 心情 emoji"),
            ("last_speech", "pet 最近一条主动开口 + ts — 与 ChatMini「⏱ 沉默 N 分」chip 对偶"),
            ("show_speech", "列最近 N 条 pet 主动开口（默认 5，上限 20）— 与 /last_speech 单条对偶"),
            ("last", "显本聊天最近创建的一条 task（含 raw 描述预览）— 闪查刚 enqueue 的"),
            ("random", "随机抽 1 条 active 任务（pending / error）— 选择困难时让宠物决定"),
            ("sleep", "一键 mute proactive 8 小时 + 友好「晚安」reply（= /mute 480）"),
            ("quick", "静默创 P3 task + 极短 reply — 适合快速 dump 不被长回复打扰"),
            ("yesterday", "列昨日 done 任务标题 + result 摘要（与 /today 互补）"),
            ("today_done", "今日已 done 任务标题 + result 摘要一行式（/today 的 done 切片 + result）"),
            ("streak", "连续有 done 完成的天数 + 近 7 天 / 30 天 done 总数"),
            ("pri", "单改任务 priority（0..=9）不走 /edit 全量覆写"),
            ("feedback", "给 pet 留反馈（写 feedback_history，影响下次 proactive turn）"),
            ("transient", "设 N 分钟有效的临时上下文给 pet（不存盘 in-memory；缺省 60m，上限 7 天）"),
            ("feedback_history", "列最近 N 条 feedback 记录（回复 / 点赞 / 评论 / 忽略 / 点掉 / 困惑；缺省 5，上限 20）"),
            ("silent_all", "批量给所有 butler_tasks 加 [silent] N 分钟，到期 backend timer 自动撤回（缺省 60；0 = 立即解除）"),
            ("alarms", "列最近 N 条 todo 段 pending reminders 含目标时间 + 剩余分钟（缺省 5，上限 20）"),
            ("recent_chats", "列最近 N 条 active session 内 user ↔ pet 聊天往返（缺省 5，上限 20）"),
            ("aware", "pet 当前感知 snapshot：transient_note + active tasks + mood + 时间 + 陪伴天数"),
            ("here", "owner 视角信号 snapshot：transient_note + mute 剩余 + 最近 feedback band（与 /aware 对偶）"),
            ("tag", "列含某 #tag 的所有 task（exact 等值；与 /tags 列 tag 名互补）"),
            ("tags_for", "列单条 task 标的所有 #tag（与 /tags 全聊天视图对偶 — 单条聚焦）"),
            ("touch", "刷 task 的 updated_at 不改内容 — 让老 task 重新冒头 proactive 选单"),
            ("edit_due", "用友好 preset 改 due（tonight / 明天 / 周一 / next_friday / +30m / +1d / clear ...）"),
            ("cancel_all_error", "批量 cancel 本聊天所有 error 状态任务（需带 `confirm` token 防误触）"),
            ("promote_all_p7", "紧急 sprint：批量给本聊天 active task priority +1（clamp 7）— 需带 `confirm`"),
            ("touch_all_p7", "批量 touch 所有 P7+ active task — 刷 updated_at 让高优重新冒头（需带 `confirm`）"),
            ("pin_all_p7", "批量给所有 P7+ active task 加 [pinned] marker — sprint 一键钉（需带 `confirm`）"),
            ("consolidate_now", "TG 端手动触发一次 consolidate sweep — 与桌面「立即整理」对偶（需带 `confirm` — LLM-heavy / 烧 token）"),
            ("promote", "任务 priority +1（clamp 9）— 一步升优先级不必算具体 P 值"),
            ("demote", "任务 priority -1（clamp 0）— 一步降优先级，与 /promote 对偶"),
            ("due", "列指定时段 due 的 pending 任务（preset: tomorrow / thisweek / nextweek，缺省 tomorrow）"),
            ("recent", "最近 N 条已完成任务标题（默认 5，上限 20）"),
            ("oldest_n", "本 chat 最老 N 条 pending（created_at asc）— audit「堆积最久的活」（默认 5，上限 20）"),
            ("active_recent", "本 chat 最近 N 条新建 active task（pending / error，created_at desc）— 与 /recent done 反向（默认 5，上限 20）"),
            ("find", "按 keyword 搜本聊天派单（命中标题或描述子串，至多 10 条）"),
            ("find_in_detail", "按 keyword 搜本聊天派单的 detail.md 内容（含命中点 snippet，至多 8 条）— 与 /find 互补"),
            ("show", "显单条任务完整 raw description（含 markers）+ detail.md 预览"),
            ("peek", "一行紧凑视图：status + 标题 + schedule + 关键 markers（与 /show 完整视图互补）"),
            ("dup", "复制 task 为新 pending 实例（保 schedule / pinned / silent / tags；剥终态 markers）"),
            ("snippets", "列含 [snippet] / [snippet: <label>] marker 的 task — 可复用模板 / 流程 / 常用回复 audit"),
            ("recent_events", "单 task 最近 N 个 butler_history 事件紧凑视图（默认 5，上限 20）— 与 /timeline 完整视图互补"),
            ("touched_today", "列今日 updated_at 命中 task（任意状态）— audit「我今天动过哪些」；与 /today_done done-only 互补"),
            ("edit_title", "改 task 标题：/edit_title <old> :: <new> — 不动 description / detail.md / markers"),
            ("touched_yesterday", "/touched_today 的昨日对偶 — 任意状态、昨日 updated_at 命中 task（复盘视角）"),
            ("touched_thisweek", "本周（自周一 00:00 起）任意状态、updated_at 命中 task — 周维度复盘"),
            ("oldest_done", "最早完成的 N 条 done task（updated_at asc）— /recent 反向；audit「老 backlog 终于完成」"),
            ("cascade_rename", "改 task 标题 + 自动同步所有 detail.md 内 「<old>」 ref 替换（cross-doc ref 维护）"),
            ("mute_today", "静音 proactive 到本地午夜 — 一键「今夜不打扰」预设"),
            ("digest_yesterday", "昨日 done 任务 + [result:] 一行式（默认 5，上限 20）— /digest 的昨日对偶"),
            ("digest_thisweek", "本周 done 任务 + [result:] 一行式（默认 5，上限 20）— 周报场景"),
            ("search_today", "限定今日 updated_at 的 task 内 fuzzy 搜 keyword — 「今天我做的与 X 相关的」精准 audit"),
            ("search_yesterday", "/search_today 的昨日对偶 — 「昨天我做的与 X 相关的」精准 audit（复盘视角）"),
            ("search_thisweek", "/search_today 的本周对偶 — 「本周与 X 相关的」精准 audit（周报场景）"),
            ("find_in_detail_today", "/find_in_detail 的今日切片 — 限今日 updated_at task 的 detail.md 内容搜"),
            ("find_in_detail_yesterday", "/find_in_detail_today 的昨日对偶 — 昨日 task 的 detail.md 内容搜"),
            ("alarms_today", "今日待触发 alarm（/alarms 的 today 切片；无 N 参 — 今日范围天然小）"),
            ("alarms_thisweek", "/alarms_today 的本周对偶 — 本周内触发 alarm 集中视图（无 N 参）"),
            ("peek_pinned", "所有 pinned task 一行紧凑视图 — /pinned 的密集版 + /peek 的批量版"),
            ("random_pinned", "从 pinned task 中随机抽 1 条 — /random 的 pinned 子集（选择困难时让 pet 决定）"),
            ("idle_7d", "pending 且 updated_at ≥ 7 天前的 task（idle 天数 desc）— stale backlog audit；PanelTasks 💤 chip TG 对偶"),
            ("recent_pins", "近 N 条 pin 决策（每 title 取最早 [pinned] sighting desc）"),
            ("help_table", "audit family 分组速查表 — /help（flat 全表）的分组兄弟，命令爆炸后 navigation aid"),
            ("audit_summary", "聚合 5 大 audit 信号 — sprint kickoff 一键视图（pin streak / cat / idle / today / 7d done）"),
            ("cat_top", "按 cat items 总量 desc 列前 N — 跨 cat 容量对比（与 growth/decay 活跃度 axis 正交）"),
            ("tags_today", "今日动过 task 含的 #tag 计数（/tags 的 today 切片）"),
            ("tags_yesterday", "/tags_today 的昨日对偶 — 昨日动过 task 含的 #tag 计数"),
            ("tags_thisweek", "/tags_today 的本周对偶 — 本周动过 task 含的 #tag 计数（周报场景）"),
            ("timeline", "时间线：列出某任务历经的所有 butler_history 事件 + 当时的状态变化 markers"),
            ("blocked", "列出被 [blockedBy: …] 锁住的活跃 task + 仍未解决的 blocker 标题"),
            ("forks", "反向 audit：列引用 [blockedBy: <this>] 的活跃 task — 这条解锁后会让谁动起来"),
            ("blocked_by", "单条 audit：列 title 仍未解决的 blockers（与 /forks 反向 — 我在等谁）"),
            ("snoozed", "列出当前在 [snooze: …] 中的 task + 还多久醒"),
            ("mute", "临时静音 proactive N 分钟（默认 30；0 = 解除）"),
            ("sleep_until", "静音到指定本地时刻（HH:MM）— 与 /mute N 互补；目标时刻 ≤ now 时落明日同时"),
            ("snooze_until", "把任务 snooze 到指定本地时刻（HH:MM）— 与 /snooze 相对预设互补"),
            ("note", "把任意文本作 general memory item 存（owner 随手记一笔）"),
            ("reflect", "把任意文本作 ai_insights memory item 存（反思 / 自我洞察）"),
            ("digest", "最近 N 条 done task 标题 + result 一行式（默认 5，上限 20）"),
            ("edit", "覆写 butler task 描述：/edit <title> :: <new desc>"),
            ("swap_priority", "互换两 task 的 priority：/swap_priority <a> :: <b>（sprint 重组场景）"),
            ("reset", "清掉 LLM 对话上下文（保留人设）"),
            ("version", "查看 pet 版本 + schema 版本"),
            ("help", "显示完整命令帮助"),
        ],
    }
}

/// GOAL 061：Telegram setMyCommands 单 scope 上限 100；本项目命令矩阵已
/// >100 触发 `BOT_COMMANDS_TOO_MUCH`。本表是**日常高频 + 用户必知**的精选
/// ≤ 20 条命令名，仅这些走 setMyCommands 让 TG / 补全 弹窗保持可用；其它
/// 命令仍能通过 `parse_tg_command` 文字解析执行，只是不在 / 候选 dropdown。
///
/// 选择原则（spec 对应「日常高频 + 用户必知」）：
/// - 核心 task lifecycle（task / tasks / done / cancel / snooze）
/// - 高频 audit（stats / today / pinned / recent / digest）
/// - 信号查询（mood / now / aware）
/// - 控制 / 帮助（mute / sleep / note / transient / find / help）
///
/// 命名为 `&[&str]`（非 const 数组）便于将来 user-customizable 时换实现。
/// 顺序即弹窗呈现顺序——TG 客户端按数组顺序展示。
pub const ESSENTIAL_TG_COMMAND_NAMES: &[&str] = &[
    "task", "tasks", "done", "cancel", "snooze",
    "stats", "today", "pinned", "recent", "digest",
    "mood", "now", "aware",
    "mute", "sleep", "note", "transient", "find",
    "help",
];

/// custom 命令在 setMyCommands 列表里的剩余预算。20 essential + 20 custom +
/// 留 60 余量给 TG API 的真实 100 上限——既给 user 自定义留空间，也防 user
/// 配过头再次踩 100 限制。
pub const ESSENTIAL_TG_CUSTOM_BUDGET: usize = 20;

/// GOAL 061：只把 essential 子集 + 限量 custom 注册给 Telegram setMyCommands。
/// 全套命令仍通过 [`parse_tg_command`] 工作；用户记忆命令 / 文字打全名即可
/// 命中——只是 `/` 弹窗补全只显这 ≤40 条避免触顶。
///
/// 实现：先把 `merged_command_registry` 的全集拿到，按 [`ESSENTIAL_TG_COMMAND_NAMES`]
/// 过滤 hardcoded 段；custom 段按 [`ESSENTIAL_TG_CUSTOM_BUDGET`] 截断；
/// 二者合并保 essential 在前、custom 在后。
pub fn essential_tg_command_registry(
    custom: &[crate::commands::settings::TgCustomCommand],
    lang: &str,
) -> Vec<(String, String)> {
    let full = merged_command_registry(custom, lang);
    let essential_set: std::collections::HashSet<&str> =
        ESSENTIAL_TG_COMMAND_NAMES.iter().copied().collect();
    // 第一段：hardcoded 命中 essential 名单的——保 merged_command_registry
    // 原顺序，让 essential 名按 [`ESSENTIAL_TG_COMMAND_NAMES`] 当前顺序无关，
    // 改取 hardcoded 自然顺序更稳定（与既有 hardcoded 矩阵顺序一致）
    let hardcoded_filtered: Vec<(String, String)> = full
        .iter()
        .filter(|(n, _)| essential_set.contains(n.as_str()))
        .cloned()
        .collect();
    // 第二段：custom 命令（merged_command_registry 已过滤了非法 / 冲突项）—
    // 取 hardcoded names 之外的尾部，再按 budget 截断
    let hardcoded_names: std::collections::HashSet<String> = full
        .iter()
        .filter(|(n, _)| !essential_set.contains(n.as_str()))
        // hardcoded 区里被 essential filter 排除的也是 hardcoded 名，仍要去
        // 重；走 merged_command_registry 内部分两段时， custom 一定在 essential
        // 之后，因此本 filter 拿到的是 (hardcoded - essential) ∪ custom。
        // 需要再区分：用 custom 自带 name 集合
        .map(|(n, _)| n.clone())
        .collect();
    let custom_set: std::collections::HashSet<&str> =
        custom.iter().map(|c| c.name.trim()).collect();
    let custom_filtered: Vec<(String, String)> = hardcoded_names
        .iter()
        .filter(|n| custom_set.contains(n.as_str()))
        .take(ESSENTIAL_TG_CUSTOM_BUDGET)
        .cloned()
        .filter_map(|name| {
            full.iter()
                .find(|(n, _)| *n == name)
                .map(|(n, d)| (n.clone(), d.clone()))
        })
        .collect();
    let mut out = hardcoded_filtered;
    out.extend(custom_filtered);
    out
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
        // `/peek <title>`：与 /show 同 single-title 模板。空 title 由 handler
        // 走 missing-argument。pure formatter 在 handler 端只读 raw_description
        // + status，不读 detail.md（紧凑视图不需要）。
        "peek" => Some(TgCommand::Peek { title }),
        // `/dup <title>`：与 /show 同 single-title 模板。空 title 由 handler
        // 走 missing-argument。复制成新 P3 task，title 加 `(副本)` 后缀。
        "dup" => Some(TgCommand::Dup { title }),
        // `/snippets`：无参 — 列含 [snippet] / [snippet: label] marker 的
        // task。与 /pinned / /silenced 同 chat-scope filter 模板。
        "snippets" => Some(TgCommand::Snippets),
        // `/touched_today`：无参 — 列今日 updated_at 命中 task。多余尾部
        // 忽略（与 /today / /yesterday / /today_done 同 tolerant trailing
        // 协议）。
        "touched_today" => Some(TgCommand::TouchedToday),
        // `/touched_yesterday`：与 /touched_today 同模板，scope 昨日。
        "touched_yesterday" => Some(TgCommand::TouchedYesterday),
        // `/touched_thisweek`：本周（自周一起）updated_at 命中 task。
        "touched_thisweek" => Some(TgCommand::TouchedThisweek),
        // `/recent_events <title> [N]`：trailing N 解析与 /snooze 同
        // 「最后 token 命中预设 → 剥」模板，但本命令 token 是数字。
        // 仅当 2+ tokens 且最后 token 是 1..=20 数字时剥下作 N；只 1 token
        // 视作 title（避免「/recent_events 5」被误判为 N=5 无 title）。
        // N 缺省 5，与 /recent / /digest / /show_speech 同协议。
        "recent_events" => {
            let trimmed = title.trim().to_string();
            let toks: Vec<&str> = trimmed.split_whitespace().collect();
            let (title_str, n) = if toks.len() >= 2 {
                let last = toks[toks.len() - 1];
                match last.parse::<u32>() {
                    Ok(v) if (1..=20).contains(&v) => {
                        let head_end = trimmed.rfind(last).unwrap_or(trimmed.len());
                        (
                            trimmed[..head_end].trim_end().to_string(),
                            v,
                        )
                    }
                    _ => (trimmed, 5),
                }
            } else {
                (trimmed, 5)
            };
            Some(TgCommand::RecentEvents {
                title: title_str,
                n,
            })
        }
        // `/timeline <title>`：与 /show 同 single-title 模板。空 title 由
        // handler 走 missing-argument 反馈。butler_history.log 扫描在
        // handler 端做（IO），parser 仅切 title。
        "timeline" => Some(TgCommand::Timeline { title }),
        // `/forks <title>`：与 /show / /timeline 同 single-title 模板。空
        // title 由 handler 走 missing-argument。反向 blockedBy 扫描在
        // formatter 端做（pure），parser 仅切 title。
        "forks" => Some(TgCommand::Forks { title }),
        // `/blocked_by <title>`：与 /forks 同 single-title 模板，单条
        // 反向 audit。snake_case 避开 dash drift-defense。
        "blocked_by" => Some(TgCommand::BlockedBy { title }),
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
        // `/buckets`：无参；多余尾部忽略（与 /stats 同容忍模板）
        "buckets" => Some(TgCommand::Buckets),
        // `/mood` 同 /tasks：无参；多余尾部忽略（让 "/mood now?" 也能命中）
        "mood" => Some(TgCommand::Mood),
        // `/whoami` 同上：无参；多余尾部忽略（让 "/whoami please" 也能命中）
        "whoami" => Some(TgCommand::Whoami),
        // `/recall <query>`（GOAL 038）：所有参数 = query。空 query 由
        // handler 反馈用法。
        "recall" => Some(TgCommand::Recall { query: title }),
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
        // `/pinned_due`：无参；多余尾部忽略（与 /pinned / /silenced 同
        // 容忍）。snake_case 命名避开 dash drift-defense。
        "pinned_due" => Some(TgCommand::PinnedDue),
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
        // `/tags`：无参；多余尾部忽略（与 /markers / /pinned 同容忍策略）。
        "tags" => Some(TgCommand::Tags),
        // `/tags_for <title>`：与 /show / /timeline / /forks 同 single-title
        // 模板。空 title 由 handler 走 missing-arg。snake_case 避开
        // dash drift-defense。
        "tags_for" => Some(TgCommand::TagsFor { title }),
        // `/touch <title>`：与 /show / /done 同 single-title 模板。
        "touch" => Some(TgCommand::Touch { title }),
        // `/today` 同上无参语义
        "today" => Some(TgCommand::Today),
        // `/now` 无参；多余尾部忽略（与 /today / /mood / /version 同容忍）
        "now" => Some(TgCommand::Now),
        // `/last_speech`：无参；多余尾部一律忽略（与 /now / /aware / /here
        // 同 "tolerant trailing" 模板）。
        "last_speech" => Some(TgCommand::LastSpeech),
        // `/show_speech [N]`：N clamp 1..=20，缺省 5。非数字尾部一律
        // 忽略走默认（与 /recent / /digest 同前向兼容策略）。
        "show_speech" => {
            let n = title
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .map(|n| n.clamp(1, 20))
                .unwrap_or(5);
            Some(TgCommand::ShowSpeech { n })
        }
        // `/aware` 无参；多余尾部一律忽略（与 /now 同模板）
        "aware" => Some(TgCommand::Aware),
        // `/here` 无参；多余尾部一律忽略（与 /aware 同模板）
        "here" => Some(TgCommand::Here),
        // `/tag <name>`：name 可带 / 不带 `#` 前缀；多余尾部一律 trim 后
        // 用第一个 token 作 name（含空格的 tag 不合法 — 与 parse_task_tags
        // 边界一致）。空 name → handler 走 usage hint。
        "tag" => {
            let raw = title.trim().trim_start_matches('#').trim();
            let name = raw.split_whitespace().next().unwrap_or("").to_string();
            Some(TgCommand::Tag { name })
        }
        // `/edit_due <title> <preset>`：与 /pri 同 parser 模板 — rsplit
        // 末 whitespace token 作 preset 字符串走 parse_edit_due_preset；
        // 剩余作 title。preset 无法识别 → None 让 handler 走 usage hint
        // （含 list of valid presets）。空 title / 仅 preset 单 token →
        // (title="", preset=parsed_or_none) 让 handler 走 missing-arg。
        "edit_due" => {
            let s = title.trim();
            if s.is_empty() {
                return Some(TgCommand::EditDue {
                    title: String::new(),
                    preset: None,
                });
            }
            let (title_out, preset_out) = match s.rfind(char::is_whitespace) {
                Some(pos) => {
                    let left = s[..pos].trim();
                    let right = s[pos..].trim();
                    let preset = parse_edit_due_preset(right);
                    match preset {
                        Some(p) => (left.to_string(), Some(p)),
                        // preset 不识别 → 整段当 title，preset=None
                        None => (s.to_string(), None),
                    }
                }
                None => {
                    // 单 token：可能"仅 title"或"仅 preset"。后者更可能
                    // （title 单字罕见）— 仅 title 路径 owner 想清 due
                    // 也会写 `<title> clear`。试 preset 解析；解出来视
                    // 为"仅 preset 缺 title"。
                    let preset = parse_edit_due_preset(s);
                    if preset.is_some() {
                        (String::new(), preset)
                    } else {
                        (s.to_string(), None)
                    }
                }
            };
            Some(TgCommand::EditDue {
                title: title_out,
                preset: preset_out,
            })
        }
        // `/last` 无参；多余尾部忽略
        "last" => Some(TgCommand::Last),
        // `/random` 无参；多余尾部忽略
        "random" => Some(TgCommand::Random),
        // `/sleep` 无参；多余尾部忽略
        "sleep" => Some(TgCommand::Sleep),
        // `/quick <text>`：与 /task 同 silent ack 模式 — 所有 arg 当 text
        // （保空格 / 不解析 !! / !!! 前缀）。空 text 由 handler 走 missing-
        // argument 反馈。
        "quick" => Some(TgCommand::Quick { text: title }),
        // `/yesterday` 无参；多余尾部忽略
        "yesterday" => Some(TgCommand::Yesterday),
        // `/today_done`：无参，多余尾部忽略（与 /today / /yesterday 同
        // 容忍策略）。注：name 必须 lowercase ASCII / digit / `_`（TG
        // 客户端约束），`today_done` 是 snake_case 不用 dash 避免被
        // drift-defense 拦截（dash 在 parse_tg_command 内部走 reject）。
        "today_done" => Some(TgCommand::TodayDone),
        // `/streak` 无参；多余尾部忽略
        "streak" => Some(TgCommand::Streak),
        // `/recent_pins [N]`：与 /recent 同 clamp 1..=20。
        "recent_pins" => {
            let n = title
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .map(|n| n.clamp(1, 20))
                .unwrap_or(5);
            Some(TgCommand::RecentPins { n })
        }
        // `/pri <title> <N>`：rsplit 末尾 whitespace token 作 priority u8
        // (≤ 9)；解析失败 → priority=None 让 handler 走 usage hint。title
        // 含空格 / 中文标点都保（与 /snooze trailing token 同模板）。
        "pri" => {
            let s = title.trim();
            if s.is_empty() {
                return Some(TgCommand::Pri {
                    title: String::new(),
                    priority: None,
                });
            }
            let (title_out, pri_out) = match s.rfind(char::is_whitespace) {
                Some(pos) => {
                    let left = s[..pos].trim();
                    let right = s[pos..].trim();
                    match right.parse::<u8>() {
                        Ok(n) if n <= 9 => (left.to_string(), Some(n)),
                        _ => (s.to_string(), None),
                    }
                }
                None => {
                    // 无空白 — 仅 1 个 token；可能是"仅 title"或"仅 N"。前
                    // 者更常见；若是纯数字 0-9 也归入 title（owner 想 set
                    // priority 但漏了 title）。统一返 priority=None handler
                    // 走 usage hint。
                    (s.to_string(), None)
                }
            };
            Some(TgCommand::Pri {
                title: title_out,
                priority: pri_out,
            })
        }
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
        // `/oldest_n [N]`：与 /recent 同 clamp 1..=20，缺省 5。snake_case
        // 命名避开 dash drift-defense。
        "oldest_n" => {
            let n = title
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .map(|n| n.clamp(1, 20))
                .unwrap_or(5);
            Some(TgCommand::OldestN { n })
        }
        // `/oldest_done [N]`：与 /recent 同 N 处理但反向 — 最早完成的 N 条
        // done。snake_case 命名一致。
        "oldest_done" => {
            let n = title
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .map(|n| n.clamp(1, 20))
                .unwrap_or(5);
            Some(TgCommand::OldestDone { n })
        }
        // `/active_recent [N]`：与 /recent / /oldest_n 同 clamp 1..=20，缺省 5。
        // snake_case 命名避开 dash drift-defense。
        "active_recent" => {
            let n = title
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .map(|n| n.clamp(1, 20))
                .unwrap_or(5);
            Some(TgCommand::ActiveRecent { n })
        }
        // `/find <keyword>`：所有 arg 作 keyword（含空格也保留 — 让 "/find
        // 整理 Downloads" 命中标题含"整理 Downloads"的 task）。空 keyword
        // 由 handler 走 missing-argument。
        "find" => Some(TgCommand::Find { keyword: title }),
        // `/search_today <keyword>`：与 /find 同 single-arg 模板。空 keyword
        // 由 handler / formatter 走 missing-argument。
        "search_today" => Some(TgCommand::SearchToday { keyword: title }),
        // `/search_yesterday <keyword>`：与 /search_today 同模板，scope 昨日。
        "search_yesterday" => Some(TgCommand::SearchYesterday { keyword: title }),
        // `/search_thisweek <keyword>`：与 /search_today 同模板，scope 本周。
        "search_thisweek" => Some(TgCommand::SearchThisweek { keyword: title }),
        // `/alarms_today`：无参 — 多余尾部一律忽略（与 /touched_today /
        // /mute_today 同协议）。handler 走同 /alarms backend 但 formatter
        // 限定今日 target。
        "alarms_today" => Some(TgCommand::AlarmsToday),
        // `/alarms_thisweek`：与 /alarms_today 同模板，scope 本周。
        "alarms_thisweek" => Some(TgCommand::AlarmsThisweek),
        // `/peek_pinned`：无参 — /pinned 紧凑版 + /peek 批量版。handler
        // 内部 filter pinned + 每条调 format_peek_reply 拼总输出。
        "peek_pinned" => Some(TgCommand::PeekPinned),
        // `/random_pinned`：无参 — /random 的 pinned 子集。
        "random_pinned" => Some(TgCommand::RandomPinned),
        // `/idle_7d`：无参 — 列 pending + updated_at ≥ 7d 前的 task。
        "idle_7d" => Some(TgCommand::Idle7d),
        // `/tags_today`：无参 — /tags 的今日切片。
        "tags_today" => Some(TgCommand::TagsToday),
        // `/tags_yesterday`：与 /tags_today 同模板，scope 昨日。
        "tags_yesterday" => Some(TgCommand::TagsYesterday),
        // `/tags_thisweek`：与 /tags_today 同模板，scope 本周。
        "tags_thisweek" => Some(TgCommand::TagsThisweek),
        // `/find_in_detail <keyword>`：所有 arg 作 keyword（含空格保留）。
        // 空 keyword 由 handler 走 missing-argument。snake_case 命名避开
        // dash drift-defense（与 /oldest_n / /active_recent 同模板）。
        "find_in_detail" => Some(TgCommand::FindInDetail { keyword: title }),
        // `/find_in_detail_today <keyword>`：与 /find_in_detail 同模板，
        // scope 限今日 updated_at。
        "find_in_detail_today" => Some(TgCommand::FindInDetailToday { keyword: title }),
        // `/find_in_detail_yesterday <keyword>`：与 /find_in_detail_today
        // 同模板，scope 昨日。
        "find_in_detail_yesterday" => Some(TgCommand::FindInDetailYesterday { keyword: title }),
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
        // `/snooze_until <title> <HH:MM>`：rsplit 末尾 whitespace token
        // 作 HH:MM；解析失败 → time=None 让 handler 走 usage hint。
        // title 含空格 / 中文标点都保（与 /pri / /promote 同模板）。
        "snooze_until" => {
            let s = title.trim();
            if s.is_empty() {
                return Some(TgCommand::SnoozeUntil {
                    title: String::new(),
                    time: None,
                });
            }
            let (title_out, time_out) = match s.rfind(char::is_whitespace) {
                Some(pos) => {
                    let left = s[..pos].trim();
                    let right = s[pos..].trim();
                    if let Some(hm) = parse_sleep_until_time(right) {
                        (left.to_string(), Some(hm))
                    } else {
                        (s.to_string(), None)
                    }
                }
                None => (s.to_string(), None),
            };
            Some(TgCommand::SnoozeUntil {
                title: title_out,
                time: time_out,
            })
        }
        // `/sleep_until <HH:MM>`：raw arg 由 handler 走 parse_sleep_until_time
        // 解析 + 计算"到 target 剩多少分钟"。空 / 无效格式由 handler 走
        // missing-arg 反馈。snake_case 命名避开 dash drift-defense。
        "sleep_until" => Some(TgCommand::SleepUntil { raw: title }),
        // `/note <text>`：所有 arg 当 text（含空格保留）。空 text 由
        // handler 走 missing-arg 反馈。
        "note" => Some(TgCommand::Note { text: title }),
        // `/reflect <text>`：与 /note 同模板但写入 ai_insights category。
        // 空 text 由 handler 走 missing-arg。
        "reflect" => Some(TgCommand::Reflect { text: title }),
        // `/feedback <text>`：与 /note / /reflect 同模板。所有 arg 作 text
        // 写到 feedback_history.log（FeedbackKind::Comment）。空 text 由
        // handler 走 missing-arg。
        "feedback" => Some(TgCommand::Feedback { text: title }),
        // `/transient <text> [minutes]`：末 whitespace token 若 parse 为
        // i64 ∈ 1..=10080 → minutes；否则 default 60。剩余 / 全部 text。
        // 仅 1 个 token 时全部当 text（与 /pri 同模板）。空 text 由 handler
        // 走 missing-arg hint。
        "transient" => {
            let s = title.trim();
            if s.is_empty() {
                Some(TgCommand::Transient {
                    text: String::new(),
                    minutes: 60,
                })
            } else {
                let (text_out, mins_out) = match s.rfind(char::is_whitespace) {
                    Some(pos) => {
                        let left = s[..pos].trim();
                        let right = s[pos..].trim();
                        match right.parse::<i64>() {
                            Ok(n) if (1..=10080).contains(&n) => {
                                (left.to_string(), n)
                            }
                            _ => (s.to_string(), 60),
                        }
                    }
                    None => (s.to_string(), 60),
                };
                Some(TgCommand::Transient {
                    text: text_out,
                    minutes: mins_out,
                })
            }
        }
        // `/cancel-all-error [confirm]`：带 "confirm" 才执行。case-insensitive
        // trim 后 == "confirm" 才算确认；任何其它 trailing token 都视作
        // 未确认（owner 误敲 `/cancel-all-error yes` 不该被算作确认）。
        "cancel_all_error" => {
            let confirmed = title.trim().eq_ignore_ascii_case("confirm");
            Some(TgCommand::CancelAllError { confirmed })
        }
        // `/promote_all_p7 [confirm]`：与 /cancel_all_error 同 confirm
        // 模板。case-insensitive trim == "confirm" 才算确认。
        "promote_all_p7" => {
            let confirmed = title.trim().eq_ignore_ascii_case("confirm");
            Some(TgCommand::PromoteAllP7 { confirmed })
        }
        // `/touch_all_p7 [confirm]`：与 /promote_all_p7 同 confirm
        // 模板。仅 trailing "confirm" token 算确认；其它 trailing
        // 视作未确认（防误触）。
        "touch_all_p7" => {
            let confirmed = title.trim().eq_ignore_ascii_case("confirm");
            Some(TgCommand::TouchAllP7 { confirmed })
        }
        // `/pin_all_p7 [confirm]`：与 /touch_all_p7 / /promote_all_p7
        // 同 confirm 模板。仅 trailing "confirm" token 算确认。
        "pin_all_p7" => {
            let confirmed = title.trim().eq_ignore_ascii_case("confirm");
            Some(TgCommand::PinAllP7 { confirmed })
        }
        // `/consolidate_now [confirm]`：与 P7+ 批量族 confirm 模板一致；
        // consolidate 是 LLM-heavy + token-burning 操作，必须带 token。
        "consolidate_now" => {
            let confirmed = title.trim().eq_ignore_ascii_case("confirm");
            Some(TgCommand::ConsolidateNow { confirmed })
        }
        // `/promote <title>`：priority +1 — title 全段保（含空格 / 标点）。
        // 空 title 由 handler 走 missing-arg。
        "promote" => Some(TgCommand::Promote { title }),
        // `/demote <title>`：priority -1 — 与 /promote 同模板。
        "demote" => Some(TgCommand::Demote { title }),
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
        // `/edit_title <title> :: <new title>`：first-occurrence `::` 切两
        // title。两端 trim；任一空由 handler 走 missing-arg。snake_case 避
        // dash drift-defense。
        "edit_title" => {
            let (t, nt) = match title.split_once("::") {
                Some((lhs, rhs)) => (lhs.trim().to_string(), rhs.trim().to_string()),
                None => (title, String::new()),
            };
            Some(TgCommand::EditTitle {
                title: t,
                new_title: nt,
            })
        }
        // `/cascade_rename <title> :: <new title>`：与 /edit_title 同 `::`
        // 模板。差异在 handler — 额外扫 detail.md 内 `「<old>」` ref token
        // 同步替换。
        "cascade_rename" => {
            let (t, nt) = match title.split_once("::") {
                Some((lhs, rhs)) => (lhs.trim().to_string(), rhs.trim().to_string()),
                None => (title, String::new()),
            };
            Some(TgCommand::CascadeRename {
                title: t,
                new_title: nt,
            })
        }
        // `/mute_today`：无参 — 多余尾部一律忽略（与 /today / /sleep
        // 同协议）。handler 内算「now → 次日 00:00 的分钟数」+ 调
        // set_mute_minutes。
        "mute_today" => Some(TgCommand::MuteToday),
        // `/digest_yesterday [N]`：与 /digest / /recent 同 N 处理 — 缺省
        // 5，clamp 1..=20。
        "digest_yesterday" => {
            let n = title
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .map(|n| n.clamp(1, 20))
                .unwrap_or(5);
            Some(TgCommand::DigestYesterday { n })
        }
        // `/digest_thisweek [N]`：与 /digest_yesterday 同 N 处理。
        "digest_thisweek" => {
            let n = title
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .map(|n| n.clamp(1, 20))
                .unwrap_or(5);
            Some(TgCommand::DigestThisweek { n })
        }
        // `/swap_priority <a> :: <b>`：first-occurrence `::` 切两 title。
        // 任一端 trim 后为空 → handler 走 missing-arg（在 formatter 内
        // 做兜底）。snake_case 命名避开 dash drift-defense。
        "swap_priority" => {
            let (a, b) = match title.split_once("::") {
                Some((lhs, rhs)) => (lhs.trim().to_string(), rhs.trim().to_string()),
                None => (title, String::new()),
            };
            Some(TgCommand::SwapPriority {
                title_a: a,
                title_b: b,
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
        // `/feedback_history [N]`：与 /digest / /recent 同 clamp 模板 —
        // N 缺省 5，clamp 1..=20。非数字 / 空尾部走默认。
        "feedback_history" => {
            let n = title
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .map(|n| n.clamp(1, 20))
                .unwrap_or(5);
            Some(TgCommand::FeedbackHistory { n })
        }
        // `/silent_all [minutes]`：与 /mute 同 clamp 模板。minutes 缺省
        // 60；0 = 立即释放当前 active 窗口（/mute 0 同协议）；clamp
        // 0..=10080（≤ 7 天）。非数字尾部走默认 60。
        "silent_all" => {
            let minutes = title
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<i64>().ok())
                .map(|n| n.clamp(0, 10080))
                .unwrap_or(60);
            Some(TgCommand::SilentAll { minutes })
        }
        // `/alarms [N]`：与 /digest / /feedback_history 同 clamp 模板。
        // N 缺省 5，clamp 1..=20。非数字走默认。
        "alarms" => {
            let n = title
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .map(|n| n.clamp(1, 20))
                .unwrap_or(5);
            Some(TgCommand::Alarms { n })
        }
        // `/recent_chats [N]`：与 /alarms / /digest 同 clamp 模板。
        // N 缺省 5，clamp 1..=20。非数字走默认。
        "recent_chats" => {
            let n = title
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .map(|n| n.clamp(1, 20))
                .unwrap_or(5);
            Some(TgCommand::RecentChats { n })
        }
        // `/reset` 无参；多余尾部忽略
        "reset" => Some(TgCommand::Reset),
        // `/version` 无参；多余尾部忽略
        "version" => Some(TgCommand::Version),
        // `/help` 同 /tasks：无参，多余尾部忽略
        // `/help` 无参 = 显全表；`/help <cmd>` = 显该命令详细用法。topic
        // 可以带 `/` 前缀或不带，大小写不敏感 — 都在 format helper 内规整。
        // `/help_table [family]`：无参全表；有参单 family 详细 list。
        "help_table" => Some(TgCommand::HelpTable {
            family: if title.is_empty() { None } else { Some(title) },
        }),
        // `/audit_summary`：无参 — 聚合 5 大 audit 信号 sprint kickoff。
        "audit_summary" => Some(TgCommand::AuditSummary),
        // `/cat_top [N]`：与 /recent 同 clamp 1..=20，缺省 5。
        "cat_top" => {
            let n = title
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .map(|n| n.clamp(1, 20))
                .unwrap_or(5);
            Some(TgCommand::CatTop { n })
        }
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

#[path = "command_utils.rs"]
mod command_utils;
pub use command_utils::*;


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
/// 长版说明书 topic 名单。`format_help_for_topic("all", ...)` 与 drift-
/// defense 测试都引用这同一矩阵，保两侧不漂移。顺序也用于"all"渲染时
/// 段次序 — 高频创建命令在前、兜底 help 在末，与 /help 全表同节奏。
pub const ALL_HELP_TOPICS: &[&str] = &[
    "task", "tasks", "stats", "buckets", "done", "cancel", "retry", "snooze",
    "unsnooze", "pin", "unpin", "pinned", "pinned_due", "silent", "unsilent",
    "silenced", "silent_all", "markers", "tags", "tag", "tags_for", "touch", "mood",
    "whoami", "today", "today_done", "yesterday", "streak", "now", "last_speech", "show_speech",
    "aware", "here",
    "last", "random", "sleep", "sleep_until", "snooze_until", "quick", "due", "recent", "oldest_n", "active_recent", "recent_chats",
    "digest", "alarms", "edit", "edit_due", "pri", "promote", "demote", "swap_priority",
    "reflect", "feedback", "feedback_history", "transient",
    "cancel_all_error", "promote_all_p7", "touch_all_p7", "pin_all_p7", "consolidate_now", "find", "find_in_detail", "find_in_detail_today", "find_in_detail_yesterday", "search_today", "search_yesterday", "search_thisweek", "show", "peek", "peek_pinned", "dup", "snippets", "recent_events", "touched_today", "touched_yesterday", "touched_thisweek", "oldest_done", "edit_title", "cascade_rename", "mute_today", "digest_yesterday", "digest_thisweek", "alarms_today", "alarms_thisweek", "tags_today", "tags_yesterday", "tags_thisweek", "random_pinned", "idle_7d", "recent_pins", "help_table", "audit_summary", "cat_top", "timeline",
    "blocked", "forks", "blocked_by", "snoozed", "reset", "version", "help",
];

/// pure：`/help search <kw>` 实现 — 扫 ALL_HELP_TOPICS 内每条命令的
/// (name, registry_desc, full_detail) 三处文本，case-insensitive 子串
/// 命中即收录。返列表 "· /<name> — <registry_desc>"。空 kw → usage
/// hint。无命中 → 友好兜底 + 提示 `/help all` 看全文。
pub fn format_help_search(
    kw: &str,
    custom: &[crate::commands::settings::TgCustomCommand],
) -> String {
    if kw.is_empty() {
        return "🔍 用法：/help search <keyword>\n\n在所有命令名 / 描述 / 详细文案里搜 keyword（case-insensitive），返命中清单。\n\n例：/help search done\n例：/help search 复制\n例：/help search snooze\n\n相关：/help <cmd>（看单条详细）；/help all（长版说明书）。".to_string();
    }
    let kw_lower = kw.to_lowercase();
    // 构建 zh registry 索引（最常见用户语言；en 也匹配但 zh 含中文文案
    // 命中率更高）
    let zh_registry = tg_command_registry_localized("zh");
    let mut hits: Vec<(String, String)> = Vec::new();
    for tname in ALL_HELP_TOPICS {
        let detail = format_help_for_topic(tname, custom);
        let reg_desc = zh_registry
            .iter()
            .find(|(n, _)| n == tname)
            .map(|(_, d)| *d)
            .unwrap_or("");
        let in_name = tname.to_lowercase().contains(&kw_lower);
        let in_desc = reg_desc.to_lowercase().contains(&kw_lower);
        let in_detail = detail.to_lowercase().contains(&kw_lower);
        if in_name || in_desc || in_detail {
            hits.push((tname.to_string(), reg_desc.to_string()));
        }
    }
    if hits.is_empty() {
        return format!(
            "🔍 /help search「{}」\n\n未在任何命令中命中。\n试 /help（全表）/ /help all（长版说明书）。",
            kw
        );
    }
    let mut out = format!("🔍 /help search「{}」命中 {} 条：\n", kw, hits.len());
    for (name, desc) in &hits {
        out.push_str(&format!("\n· /{} — {}", name, desc));
    }
    out.push_str("\n\n（用 /help <cmd> 看单条详细 / /help all 看长版）");
    out
}

pub fn format_help_for_topic(
    topic: &str,
    custom: &[crate::commands::settings::TgCustomCommand],
) -> String {
    let name = topic.trim().trim_start_matches('/').to_lowercase();
    if name.is_empty() {
        return format_help_text(custom);
    }
    // "all" → 长版说明书：把 ALL_HELP_TOPICS 内每条命令的详细文案串起。
    // 比 /help 全表（每命令一行简述）更详细 — 适合 owner 离线 audit /
    // 学习曲线场景。bot 端走既有 format_split_chunks 自动切块发多条 TG
    // 消息（TG 4096 字符限制）。
    if name == "all" {
        let mut out = String::new();
        out.push_str("📚 /help all（长版说明书）\n\n");
        let mut first = true;
        for t in ALL_HELP_TOPICS {
            let detail = format_help_for_topic(t, &[]);
            if detail.is_empty() {
                continue;
            }
            if !first {
                out.push_str("\n\n────\n\n");
            }
            first = false;
            out.push_str(&detail);
        }
        return out;
    }
    // "search <kw>" → 在 ALL_HELP_TOPICS 内扫命令名 + 详细文案 + registry
    // 描述，case-insensitive 子串匹配，返命中清单（每条 `· /name — 一行
    // 描述`）。让 owner 自助查命令 — 不必记 30+ 命令名。空 kw 走 usage
    // hint。
    if let Some(kw) = name
        .strip_prefix("search ")
        .or_else(|| if name == "search" { Some("") } else { None })
    {
        return format_help_search(kw.trim(), custom);
    }
    let detail = match name.as_str() {
        "task" => "📝 /task <title>\n\n用法：把单条任务塞进队列。\n  · 默认优先级 P3\n  · 前缀 `!!` → P5（紧迫）\n  · 前缀 `!!!` → P7（最高）\n\n示例：\n  /task 整理 Downloads\n  /task !! 写周报\n  /task !!! 修复线上 bug\n\n创建后 chat 自动收到确认 + origin marker [origin:tg:<chat_id>]，桌面 watcher 完成时也回传通知。",
        "tasks" => "📋 /tasks\n\n用法：列出本会话派出的任务清单（按 compare_for_queue 排序 + 按状态分组）。无参；多余尾部忽略。\n\n示例：\n  /tasks\n\n相关：/stats（数字汇总）/ /today（今日切片）/ /recent（近完成）/ /find（关键词搜）。",
        "stats" => "📊 /stats\n\n用法：一行汇总当前 chat 派单的状态计数 — 待办 / 逾期 / 今日完成 / 出错 / 今日取消。无参。\n\n示例：\n  /stats\n\n与 /tasks 互补：/stats 看数字汇总，/tasks 看具体清单。相关：/buckets（priority 分桶维度而非状态维度）。",
        "buckets" => "🎯 /buckets\n\n用法：本 chat active task（pending / error）按 priority 分桶计数 — 与 /stats（按状态分桶 — 待办 / 逾期 / done / error）互补的「priority 维度 dump」。无参；多余尾部忽略。\n\n输出格式：\n  🎯 priority 分桶（N 条 active）\n  P7+: 3 · P5-6: 7 · P3-4: 12 · P1-2: 5 · P0: 2\n\n分组与桌面 PanelTasks priorityBands chip 一致（5 段 — P7+ / P5-6 / P3-4 / P1-2 / P0），让 owner 一眼看「我各档高优各有几条」分布。\n\n示例：\n  /buckets\n\n相关：/stats（状态维度汇总）；/pinned（pinned 单维度）；/pinned_due（pinned + due 交集）。",
        "done" => "✅ /done <title>\n\n用法：把 pending / error 任务标 done。已 done / cancelled 拒绝重复操作。\n\n示例：\n  /done 整理 Downloads\n\n注意：TG 端不支持 `[result: ...]` 摘要；想加 result 回桌面板单条 mark-done dialog。",
        "cancel" => "🚫 /cancel <title>\n\n用法：取消一条 pending / error 任务（终态）。\n\n示例：\n  /cancel 整理 Downloads\n  /cancel 1   （/tasks 输出第 1 条）\n\n相关：/retry 把 error 重置回 pending；二者可来回切。",
        "retry" => "🔄 /retry <title>\n\n用法：把 status==Error 的任务重置为 pending，剥所有 [error: ...] / [done] markers。\n\n示例：\n  /retry 跑步\n\n限制：仅 error 状态可 retry；pending / done / cancelled 拒。",
        "snooze" => "💤 /snooze <title> [preset]\n\n用法：暂停任务到指定时刻（preset 缺省 30m）。\n\nPreset：\n  · 30m / 2h / Nm / Nh（Nm ≤ 7 天）\n  · tonight（今晚 18:00）\n  · tomorrow（明早 09:00）\n  · monday（下周一 09:00）\n  · 今晚 / 明早 / 明天 / 周一 / 下周一 CJK 同义词\n\n示例：\n  /snooze 写周报\n  /snooze 跑步 tonight\n  /snooze 读论文 2h\n\n过点后 marker 自动失效，任务回到 proactive 选单。",
        "unsnooze" => "💤 /unsnooze <title>\n\n用法：清掉任务的 [snooze: ...] marker，立即回到 proactive 选单。\n\n示例：\n  /unsnooze 写周报",
        "pin" => "📌 /pin <title>\n\n用法：钉住关键任务（写 [pinned] marker）。pinned task 在桌面任务面板浮顶 + 「📌 N」chip 计数同源。\n\n示例：\n  /pin 季度规划\n\n相关：/pinned 列所有 pinned；/unpin 取消。",
        "unpin" => "📌 /unpin <title>\n\n用法：清掉任务的 [pinned] marker。\n\n示例：\n  /unpin 季度规划",
        "pinned" => "📌 /pinned\n\n用法：列出本聊天派单中所有 pinned 任务（按状态分组）。无参。\n\n示例：\n  /pinned\n\n相关：/markers 一次列 pinned + silent 联合；/pinned_due 收紧到 pinned + 含 due 的 active task（高优截止 audit）。",
        "pinned_due" => "🔥 /pinned_due\n\n用法：列出本聊天派单中同时 pinned + 含 due 的 active task（pending / error），按 due 升序排（最近到期在前）。无参；多余尾部忽略。owner 紧急 audit「我钉了的 + 有截止时间的」高优清单 — 一行看完「下一个 deadline 是哪条」。\n\n双重 filter 比 /pinned 或 /due 单维度更聚焦：done / cancelled 跳过（已离开活跃池）；pinned=false 跳过（不算「关键 task」）；due=None 跳过（无截止压力的不算紧急清单一员）。\n\n输出格式：\n  🔥 pinned + due 任务（共 N 条，按 due 升序）\n  ⏳ P<n> <title> — 截至 <MM/DD HH:MM>\n  ⚠️ P<n> <title> — 截至 <MM/DD HH:MM>\n\n空 → 友好兜底 + 建议 /pinned（仅 pinned）/ /due（按窗口看 due）拿更宽视角。\n\n示例：\n  /pinned_due\n\n对比：/pinned（仅 pinned，不限 due）；/due [preset]（按时段，含 unpinned）；/markers（pinned + silent 联合，无 due 维度）。本命令是「pin 高优 + 有截止」交集 — 紧急冲刺时 owner 优先扫这条。",
        "silent" => "🔇 /silent <title>\n\n用法：标静默 — LLM 不主动选此任务，但面板 / 手动触发仍可。\n\n示例：\n  /silent 周末家务\n\n相关：/silenced 列所有 silent；/unsilent 取消。owner 不想让 pet 主动 pick 某条时用。",
        "unsilent" => "🔇 /unsilent <title>\n\n用法：清掉 [silent] marker，任务回到 LLM auto-pick 池。\n\n示例：\n  /unsilent 周末家务",
        "silenced" => "🔇 /silenced\n\n用法：列出本聊天派单中所有 silent 任务（按状态分组）。无参。\n\n示例：\n  /silenced",
        "markers" => "🏷 /markers\n\n用法：一次列 pinned + silent 两段（与 /pinned + /silenced 组合等价）。无参。\n\n示例：\n  /markers\n\n给 owner audit 「我标过哪些 owner-intent」用。",
        "tags" => "🏷 /tags\n\n用法：列本聊天派单中所有用过的 `#tag` + 各 tag 任务数（按数量降序，top 15）。无参。无 #tag 任务的不计入。\n\n示例：\n  /tags\n\n相关：/markers（pinned + silent 系统 marker 维度）；/find #健身（按某 tag 搜任务清单）。让 owner audit 「我自定义 tag 矩阵长什么样」。",
        "mood" => "🐾 /mood\n\n用法：查看宠物当前心情（与桌面 MoodWidget 同 mood state 文件）。无参。\n\n示例：\n  /mood",
        "whoami" => "🐾 /whoami\n\n用法：宠物自我介绍 — 陪伴天数 / 当前心情 / 自我画像首段 / 近常用工具 top 3。无参。\n\n示例：\n  /whoami",
        "today" => "📅 /today\n\n用法：今日叙事视图 — 今日到期 (pending + due 在今天) + 今日已完成 (done + updated_at 在今天) 两段标题清单。无参。\n\n示例：\n  /today\n\n相关：/recent（不限今日 done）；/blocked（被 [blockedBy:] 锁住的）；/due（更远视角 — tomorrow / thisweek / nextweek）。",
        "now" => "🐾 /now\n\n用法：一句话快速状态 check — 当前本地时间 + tz 偏移 + 陪伴天数 + 心情 emoji + 心情文本。无参。比 /whoami 多行画像简短，适合 owner 在 TG 想「现在几点 / 宠物啥状态」闪查。\n\n示例：\n  /now\n\n相关：/whoami（多行画像）；/mood（心情详情）；/last_speech（pet 最近主动开口）。",
        "last_speech" => "🗣 /last_speech\n\n用法：显 pet 最近一条主动开口（speech_history.log 末条），含 ts + 文本 + 相对时间「N 分前 / N 小时前 / N 天前」。无参；多余尾部忽略。\n\n与 ChatMini 顶部「⏱ pet 沉默 N 分」chip 对偶 — 那个显沉默时长触发关心；本命令显具体最近说了啥（原话 + 从那时起的分钟数）。\n\n输出格式：\n  🗣 pet 最近主动开口 · MM-DD HH:MM（N 分前）：\n  「<text 前 N 字 cap>」\n\n空 history（pet 还没主动开口过 / 刚 reset） → 友好兜底。\n\n示例：\n  /last_speech\n\n相关：/show_speech [N]（最近 N 条）；/aware（pet 当前感知）；/here（owner 信号 snapshot）；/feedback_history（pet 接收的反馈）。",
        "show_speech" => "🗣 /show_speech [N]\n\n用法：列 pet 最近 N 条主动开口（speech_history.log 末 N 条，倒序最新在前）。N 缺省 5；clamp 1..=20。与 /last_speech 单条对偶 — 那个看「上次说了啥」，本命令看「最近一段时间说过啥」。\n\n输出格式：\n  🗣 pet 最近 N 条主动开口（共 M）：\n  · MM-DD HH:MM · <text 80 字 cap>\n  · MM-DD HH:MM · <text>\n  ...\n\ntext 80 字截断（per-row 紧凑 vs /last_speech 200 字单条完整）；超长 + …。\n\n场景：owner 想 audit「pet 最近一波主动开口节奏 / 内容多样性」时用。\n\n示例：\n  /show_speech\n  /show_speech 10\n  /show_speech 20\n\n相关：/last_speech（最近 1 条 + 完整 200 字）；/recent_chats（user↔pet 对话）。",
        "last" => "🆕 /last\n\n用法：显本聊天派单中最近 created_at 的一条 task — title + status emoji + 相对创建时间 + raw_description 前 200 字符预览。无参。owner 想「我刚 /task 创的那条对不对」闪查时用 — 不必走 /tasks 全表扫。\n\n示例：\n  /last\n\n相关：/show <title>（看完整 raw + detail）；/recent（最近 N 条 done）；/tasks（全状态清单）。",
        "random" => "🎲 /random\n\n用法：从本聊天派单的 active 任务（pending / error）里随机抽 1 条让宠物推荐 — 给 owner「选择困难」/「不知道先做哪个」时让 pet 决定下一步。无参；多次调用会得到不同 task。无 active 任务时给兜底文案。\n\n示例：\n  /random\n\n相关：/tasks（看全清单）；/blocked（被锁住的）；/today（今日到期）。",
        "sleep" => "🌙 /sleep\n\n用法：一键让宠物 mute proactive 8 小时 + 友好「晚安」reply。无参。比手敲 `/mute 480` 更直觉 — owner 睡前 / 长会议 / 想 deep work 时一句话搞定。\n\n示例：\n  /sleep\n\n相关：/mute [N]（精确控制 N 分钟）；/sleep_until HH:MM（静音到指定时刻）；/mute 0（立刻解除静音）。",
        "sleep_until" => "🌙 /sleep_until <HH:MM>\n\n用法：静音 proactive 到指定本地时刻（HH:MM 24 小时制；H:MM / HH / H 也接受 — 单数字视为 HH:00）。与 /mute N（相对分钟数）/ /sleep（固定 8h）互补 — owner 想「安静到 8 点」/「安静到中午」更自然。\n\n语义：目标时刻 ≤ now → 落到明日同时刻（owner 凌晨 1 点说「到 8 点」视为今早 8:00，非次日 8:00 反直觉）；clamp 1..=10080 分钟（≤ 7 天）。\n\n示例：\n  /sleep_until 8:00    （静音到 8 点）\n  /sleep_until 22:30   （静音到 22:30）\n  /sleep_until 14      （静音到下午 2 点）\n\n相关：/mute [N]（相对分钟数）；/sleep（一键 8h）；/snooze_until <title> <HH:MM>（单条 task 同模板）；/mute 0（立刻解除）。",
        "snooze_until" => "💤 /snooze_until <title> <HH:MM>\n\n用法：把指定 task snooze 到本地时刻（HH:MM 24 小时制；H:MM / HH / H 也接受）。与 /sleep_until 对偶 — 那个静音 proactive 整体到 HH:MM，本命令仅 snooze 单条 task 到 HH:MM。与既有 /snooze relative preset（tonight / tomorrow / 30m / 等）互补。\n\n语义：目标时刻 ≤ now → 落到明日同时刻；title resolve 与 /done / /cancel / /show 同三层（数字 index → fuzzy → 错误候选）。\n\n示例：\n  /snooze_until 整理 Downloads 18:00   （今晚 6 点醒）\n  /snooze_until 写周报 9:00              （明早 9 点醒）\n  /snooze_until 1 14                      （/tasks 第 1 条到下午 2 点）\n\n相关：/snooze <title> [preset]（相对预设）；/unsnooze <title>；/snoozed（列被暂停的）；/sleep_until（静音 proactive 整体）。",
        "quick" => "⚡ /quick <text>\n\n用法：静默创建一条 P3 task — 后端走 /task 同路径，但 reply 极短（仅 ✓ + title），适合 owner 想「快速 dump 想法 / 灵感不被长 reply 打扰」时用。priority 始终 P3；想精细化（!! / !!!）走 /task。空 text 由 handler 走 missing-arg hint。\n\n示例：\n  /quick 整理 ~/Downloads\n  /quick 写周报\n\n相关：/task <title>（带 !! P5 / !!! P7 前缀 + 完整确认 reply）；/note（杂项 brain-dump，不进 butler_tasks）。",
        "yesterday" => "📅 /yesterday\n\n用法：列本聊天派单中昨日完成的任务标题 + result 摘要（按 updated_at 倒序）。无参。owner 想 audit 「昨天做完了啥」时用。\n\n示例：\n  /yesterday\n\n相关：/today（今日切片）；/today_done（今日 done + result）；/recent（不限日期最近 N）；/digest（含 result 摘要的最近 N）。",
        "today_done" => "📅 /today_done\n\n用法：列今日完成的任务标题 + `[result:]` 摘要一行式（按 updated_at 倒序）。无参；多余尾部忽略。owner 想 audit「我今天做完啥 + 各条产物」一行扫读时用。\n\n输出格式：\n  📅 今日（YYYY-MM-DD）完成 N 条：\n  · ✅ <title> — <result preview 40 字截断>\n  · ✅ ...\n\n对比 /today：那个含 due 段（pending + 今日 due）+ done 段（标题清单无 result）—— 完整今日叙事；本命令是「纯 done 切片 + result 摘要」分流入口，与 /yesterday 同模板但 scope 是今日。\n\n示例：\n  /today_done\n\n相关：/today（含 due 双视图）；/yesterday（昨日 done + result）；/digest [N]（不限日期最近 N done + result）；/streak（连续 done 天数）。",
        "streak" => "🔥 /streak\n\n用法：显本聊天 done 完成节奏数据：连续完成天数 + 近 7 天 / 30 天 done 总数。无参。owner audit 「最近完成节奏怎么样 / 有没有 streak 在保」时用。streak 末端：今日有 done → 今日；否则若昨日有 → 昨日；否则 streak = 0。\n\n示例：\n  /streak\n\n相关：/today（今日切片）；/yesterday（昨日产出）；/stats（pending / overdue 等汇总）。",
        "pri" => "🎯 /pri <title> <N>\n\n用法：单改任务 priority（0..=9）— 不走 /edit 全量覆写，保留所有其它 markers（[every:] / [pinned] / [silent] / [snooze:] / [blockedBy:] / detail.md 等）。N 必须是 0-9 整数。title 含空格 / 中文标点也保（parser 取末 whitespace token 当 N）。\n\n示例：\n  /pri 整理 Downloads 5\n  /pri 写周报 7\n  /pri 跑步 0  （降到 P0 = idea 抽屉）\n\nTitle resolve 与 /done / /cancel 同三层（数字 index → fuzzy → 错误候选）。相关：/swap_priority（两条互换）；/promote / /demote（±1）。",
        "swap_priority" => "🔄 /swap_priority <a> :: <b>\n\n用法：互换两 task 的 priority — sprint 重组场景（owner 想「A 和 B 的优先级换一下」一步完成不必算具体 P 值）。`::` 是必填 separator（让 title 含空格 / 中文标点也精确切，与 /edit 同模板）。\n\n两 title 各自走三层 resolve（数字 index → fuzzy → 错误候选）；resolve 失败显具体哪条没找到。复用 task_set_priority 后端：先读两 pre-swap priority → 对称写两次（a → pre_b, b → pre_a）。保留 due / body / 其它 markers 不动。\n\n示例：\n  /swap_priority 整理 Downloads :: 写周报\n  /swap_priority 1 :: 3   （/tasks 输出第 1 与第 3 条互换）\n\n输出格式：\n  🔄 已互换 priority：「A」P3 → P7 · 「B」P7 → P3\n\n部分失败时（极少 — 写盘竞态）显具体哪条失败哪条成功。同一 title `a == b` → 无需互换兜底。\n\n相关：/pri <title> <N>（绝对设值）；/promote / /demote（±1）；/promote_all_p7（紧急批量 +1）。",
        "feedback" => "💬 /feedback <text>\n\n用法：给 pet 留反馈到 feedback_history.log（FeedbackKind::Comment）。LLM 在下次 proactive cycle 会读到 owner 原话调整行为。正向 / 负向 / 中性建议都可走此入口。\n\n示例：\n  /feedback 你最近说话太啰嗦，请精炼点\n  /feedback 这次主动选 task 选得很到位！\n  /feedback 周末别那么主动开口，让我休息\n\n相关：/note（杂项记到 general memory）；/reflect（反思记到 ai_insights）；二者按存储目的分流。本命令直接影响 pet 行为，是与 pet 沟通调整的快速通道。",
        "transient" => "📝 /transient <text> [minutes]\n\n用法：写一条 N 分钟有效的临时上下文给宠物（owner 当前状态如「在开会」「集中写文档」「今晚 9 点后回我」等）。**不存盘**，只挂当前 in-memory，到时自动清除（与桌面 PanelToneStrip 显示的 [临时指示] 同源）。minutes 末 whitespace token 解析；缺省 60；clamp 1..=10080（≤ 7 天）。\n\n示例：\n  /transient 在开会，半小时别打扰我 30\n  /transient 集中写文档不要主动开口 90\n  /transient 今晚 9 点后再 ping 我 240\n  /transient 心情不好别活泼  （默认 60 分钟）\n\n对比：/note（→ general memory 永久存盘）；/reflect（→ ai_insights 永久存盘）；/feedback（写 feedback_history 改 pet 行为）；/mute（直接静音不开口）。本命令是「给 pet 临时上下文，但不阻塞它说话」— pet 仍会主动开口，只是开口时读到这条调整语气 / 选择。",
        "feedback_history" => "📜 /feedback_history [N]\n\n用法：列最近 N 条 feedback_history.log 条目，含 owner 主动写的 /feedback comment + 系统自动记录的隐性反馈（回复 / 主动点掉 bubble / 👍 点赞 / 沉默忽略 / 🤔 困惑反馈）。让 owner audit 「我给 pet 留过什么 / pet 接收了哪些信号」。N 缺省 5，clamp 1..=20。\n\n输出格式：\n  · HH:MM <emoji> <kind> | <excerpt>\n\nkind emoji 映射：\n  ✅ replied · 👍 liked · 💬 comment · 🙉 ignored · 👋 dismissed · 🤔 puzzled\n\n示例：\n  /feedback_history\n  /feedback_history 10\n  /feedback_history 20\n\n相关：/feedback（写新条目）；R7 cooldown adapter / R28 chip 用 feedback_history 调整 pet 主动开口频率与语气 — 本命令是回看 pet 接收的训练信号。",
        "silent_all" => "⏸ /silent_all [minutes]\n\n用法：批量给所有 butler_tasks pending 任务加 [silent] marker N 分钟，N 后 backend tokio timer 自动撤回。让 owner 开会 / 集中写文档 1 小时挡住 task picker — pet 仍可主动聊天，只是不会主动派任务（如「我看你 Downloads 该整理了」之类）。minutes 缺省 60；0 = 立即解除当前 active 窗口（与 /mute 0 同协议）；clamp 0..=10080（≤ 7 天）。\n\n示例：\n  /silent_all       （默认 60 分钟）\n  /silent_all 30    （半小时）\n  /silent_all 120   （2 小时）\n  /silent_all 0     （立即解除）\n\n对比：/mute（让 pet 整体不开口）；/silent <title>（单条 silent）；本命令是批量临时 + 自动撤回。\n\n限制：app restart 会丢 timer，markers 留在原地 —— 重启后用 /silent_all 重启窗口或 /silent_all 0 手动清理（实现注：与桌面 PanelMemory「⏸ 全部 silent 1h」按钮使用 frontend timer 路径独立，两个 surface 不共享状态）。",
        "alarms" => "⏰ /alarms [N]\n\n用法：列最近 N 条 todo 段 pending reminders（含 `[remind: HH:MM]` / `[remind: YYYY-MM-DD HH:MM]` 协议条目），含目标时刻 + 剩余分钟 / 已逾期分钟。按 target 升序排（最近 fire 在前）。N 缺省 5，clamp 1..=20。\n\n输出格式：\n  · MM-DD HH:MM (剩 N 分 / 已逾期 N 分) | <topic>\n\n示例：\n  /alarms\n  /alarms 10\n\n如何创建 alarm：\n  · 桌面 PanelMemory 任意 item ⏰ chip → 选 5/15/30 min preset（iter #372）\n  · 直接 /task `[remind: 18:00] 准备会议材料`（写入 todo 类目）\n  · LLM 用 todo_edit 工具自动创建（owner 说「30 分钟后提醒我喝水」时）\n\n触发后：proactive 扫到 due → ChatMini 软提醒；Absolute 条目 24h 后 consolidate sweep 自动清扫 stale。\n\n相关：/feedback_history（看 pet 接收训练信号）；/transient（写 in-memory 临时指示）；本命令是回看「我设了哪些 alarm，何时到点」audit 入口。",
        "tag" => "🏷 /tag <name>\n\n用法：列含某 #tag 的所有 task — status emoji + title + 紧凑 due（MM-DD HH:MM）。name 可带 / 不带 `#` 前缀，case-insensitive exact 等值匹配（与 /find 子串搜正交 — /find 在 title / description 内含部分字也算命中，/tag 仅匹配完整 tag token）。pending / error 先列，其次 done / cancelled；至多前 20 条 + overflow hint。\n\n示例：\n  /tag 工作\n  /tag #urgent\n  /tag 健身\n\n相关：/tags 看本聊天用过的所有 tag 名 + 各任务数（top 15）；/tags_for <title>（单条聚焦 — 列 title 自己的 tags）；/find <keyword> 子串搜任务标题 + 描述（不限 tag）；桌面 PanelTasks #tag chip click 同 filter 视图。",
        "tags_for" => "🏷 /tags_for <title>\n\n用法：列单条 task 标的所有 #tag — 与 /tags（全聊天 tag 矩阵）对偶但单条聚焦。owner 想「这条 task 标了哪些 tag」audit 单点入口。\n\n空 title → usage hint；title resolve 与 /done / /cancel / /show 同三层（数字 index → fuzzy → 错误候选）。\n\n输出格式：\n  🏷 「<title>」N 个 tag：\n  #a #b #c ...\n\n无 #tag 标记 → 「无 #tag 标记。在 description 写 `#name` 自动收录」+ 教学。\n\n示例：\n  /tags_for 整理 Downloads\n  /tags_for 1  （/tasks 输出第 1 条）\n\n相关：/tags（全聊天 tag 矩阵 + 计数）；/tag <name>（含某 tag 的所有 task 反向）；/show <title>（看 raw description 含 #tag tokens）。",
        "touch" => "✨ /touch <title>\n\n用法：刷 task 的 updated_at 不改内容 — 让老 task 重新冒头 proactive 选单。机制：rewrite 同 description → memory_edit 自动 stamp updated_at 到 now（与 task_skip_once 共享 backend helper 但 decision_log 标 `TaskTouch` 区分）。\n\n场景：一条挂了很久的 active task owner 想让 pet 重新主动关注（无需 /promote 改 priority — touch 仅刷时间序）。\n\n空 title → usage hint；title resolve 与 /done / /cancel / /show 同三层。\n\n输出格式：\n  ✨ 已 touch「<title>」— updated_at 已刷新，老任务重新冒头 proactive 选单。\n\nDone / cancelled task 拒（终态 touch 无意义 — 不会回到 proactive 选单）。\n\n示例：\n  /touch 整理 Downloads\n  /touch 1   （/tasks 输出第 1 条）\n\n相关：/oldest_n（列最老 pending — touch 候选）；/pri / /promote（重组优先级，更强语义）；/snooze（推后到时刻 — touch 反向语义）。",
        "here" => "🧑 /here\n\n用法：owner 视角 dump 当前留给 pet 的状态信号 — transient_note（临时指示）+ mute（静音剩余）+ 最近 feedback band（high_negative / low_negative / mid / insufficient_samples + 当前 cooldown factor）。让 owner audit 「我现在给 pet 什么信号、pet cooldown 会因此被放大 / 缩小多少」— 比如发现 high_negative 但还没 mute 时可主动 /sleep。无参；多余尾部忽略。\n\n输出格式：\n  🧑 当前 owner 信号：\n  📝 transient_note: 「<text>」（剩 N 分钟）/ 未设\n  🔕 mute: 剩 N 分钟 / 未静音\n  💬 最近 feedback band: <label> · <cooldown factor 说明>\n\n示例：\n  /here\n\n对比 /aware：那个看 pet 感知到的（transient + tasks + mood + 时间 + 陪伴），本命令看 owner 输入侧（transient + mute + feedback band） — 两命令配合 audit「我说啥 → pet 看啥 → pet 怎么反应」全链路。\n\n相关：/transient（写 transient_note）；/mute（设静音 / 解除）；/feedback_history（看具体反馈条目，本命令仅显聚合 band）。",
        "aware" => "🐾 /aware\n\n用法：pet 自述当前感知到的上下文 — transient_note（owner 留下的临时指示）+ active butler_tasks 数 + 当前 mood（emoji + 文本）+ 当前时间 + 陪伴天数。无参；多余尾部忽略。一句话 debug 「pet 为啥没主动开口 / 选了那条 task」。\n\n输出格式：\n  🐾 当前感知：\n  📝 transient_note: 「<text>」（剩 N 分钟） / 无\n  📋 active tasks: N 条\n  ☁ mood: <emoji> <text>\n  🕐 当前: YYYY-MM-DD HH:MM (+08:00) · 陪伴 N 天\n\n示例：\n  /aware\n\n对比：/now（仅时间 + mood emoji，最简）；/whoami（多行画像 + 自我介绍长文）；本命令是「pet 当前感知 snapshot」中等粒度，含 transient_note 这条调度关键信号。\n\n相关：/here（owner 视角对偶 — 看 owner 输入了哪些信号）；/transient（写 transient_note）；/feedback_history（看 pet 接收的训练信号）。",
        "recent_chats" => "💬 /recent_chats [N]\n\n用法：列最近 N 条 active session 内 user ↔ pet 聊天往返（仅 user / assistant items，跳过 tool_call / 系统行）。手机端回顾上下文 — owner 想「我刚才让 pet 做啥来着」一句话查回桌面 ChatMini 滚动太累时用。N 缺省 5，clamp 1..=20。\n\n输出格式：\n  💬 最近 N 条 chat · 会话「<title>」最近活动 MM-DD HH:MM：\n  🧑 <user excerpt>\n  🐾 <pet excerpt>\n  ...\n\nexcerpt cap 80 字；超长 + …。\n\n注：per-msg ts 不在后端 schema 里，仅 session 级 updated_at 一并呈现（「最近活动」信号）。pet 桌面 reset session 时本命令也会看到新空 session 提示。\n\n相关：/feedback_history（看 pet 接收训练信号）；/transient（写 in-memory 临时指示）；本命令是回看「最近 chat 上下文」audit 入口。",
        "cancel_all_error" => "🧹 /cancel_all_error confirm\n\n用法：批量 cancel 本聊天所有 error 状态的任务。**必须带 `confirm` token** 防误触 —— 不带 confirm 时显 usage hint 告诉 owner 怎么用。\n\n示例：\n  /cancel_all_error confirm\n\n场景：周末整理 task 队列 / 大批 error 累积想一次性清空。注意：仅 cancel 本 chat 派单（origin == Tg<chat_id>）；其它 chat / 桌面直接派的 error 任务不动。已 done / cancelled 任务跳过。\n\n相关：/cancel <title>（单条取消）；/retry <title>（单条重试）；/stats（看 error 数）。",
        "promote_all_p7" => "🎯 /promote_all_p7 confirm\n\n用法：紧急 sprint 模式 — 批量给本聊天所有 active（pending / error）task priority +1，clamp 7（已 ≥ P7 的不动）。**必须带 `confirm` token** 防误触 — 不带 confirm 时显 usage hint 含可升级 N 条预览。\n\n示例：\n  /promote_all_p7         （查看可升级数 + 提示带 confirm）\n  /promote_all_p7 confirm （执行批量 +1）\n\n场景：deadline 收尾 / 紧急 sprint — 让 LLM 立即优先所有挂着的活儿，把「低优先 dump」暂搁置。\n\n注意：仅本 chat 派单（origin == Tg<chat_id>）；done / cancelled 跳过；已 P7+ 的不动（避免无意义写 + 防把 P9 撞墙）。一次性操作；想精细化走 /pri <title> <N> 单条调。\n\n对比 /cancel_all_error：那个一次性 cancel error 任务（破坏性强 — 终态）；本命令一次性升优先级（重组优先级而非删 — 破坏性更轻）。\n\n相关：/pri <title> <N>（绝对设值）；/promote <title>（单条 +1）；/demote <title>（单条 -1）；/touch_all_p7（已 P7+ 但挂着没动的批量 touch 让其重新冒头）。",
        "touch_all_p7" => "✨ /touch_all_p7 confirm\n\n用法：批量 touch 所有 P7+ active task — 刷 updated_at 不改内容，让挂着没动的高优 task 重新冒头 proactive 选单。**必须带 `confirm` token** 防误触。\n\n示例：\n  /touch_all_p7         （查看可 touch 数 + 提示带 confirm）\n  /touch_all_p7 confirm （执行批量 touch）\n\n场景：sprint 中段「我的高优 P7+ 都已设好但 pet 没在主动关注」— 一键让 LLM 重新审视全部高优清单。\n\n注意：仅本 chat 派单；done / cancelled 跳过；priority < 7 跳过（不在高优集内）。\n\n对比 /promote_all_p7：那个升 priority（让 P3 → P7）；本命令仅刷 P7+ 的 updated_at（已是 P7+ 但挂着的批量唤醒）。两命令互补 — 升优先级 vs 重新冒头。\n\n相关：/touch <title>（单条 touch）；/promote_all_p7（批量升 priority）；/pin_all_p7（批量加 [pinned] marker）；/oldest_n（看堆积最久的活）。",
        "pin_all_p7" => "📌 /pin_all_p7 confirm\n\n用法：批量给本 chat 所有 P7+ active task（pending / error）加 [pinned] marker — sprint 收尾「把高优清单全钉住」一键。**必须带 `confirm` token** 防误触。\n\n示例：\n  /pin_all_p7         （查看可 pin 数 + 提示带 confirm）\n  /pin_all_p7 confirm （执行批量 pin）\n\n场景：sprint 收尾 / 周末整理时把「高优清单」整体钉到 PanelTasks「📌 N」chip 视图，让屏幕 / TG 端的「📌」filter 一眼显这批 task 是 owner 重点关注。\n\n注意：仅本 chat 派单；done / cancelled 跳过；priority < 7 跳过；已 [pinned] 跳过（避免无意义写）。\n\n对比 /promote_all_p7（升 priority 让 P3 → P7）/ /touch_all_p7（刷 P7+ updated_at）：本命令仅加 [pinned] marker。三命令 P7+ 批量族互补 — 升优先级 / 刷时序 / 钉视图。\n\n相关：/pin <title>（单条 pin）；/promote_all_p7 / /touch_all_p7（P7+ 批量族）；/pinned（看本 chat 已钉清单）。",
        "consolidate_now" => "🧹 /consolidate_now confirm\n\n用法：TG 端手动触发一次 consolidate sweep — 与桌面 PanelMemory「立即整理」/ PanelDebug「🧹 force consolidate」同后端 trigger_consolidate。consolidate 是 LLM-heavy + token-burning 操作（多步 sweep + LLM call，~30s..2min），**必须带 `confirm` token** 防误触。\n\n示例：\n  /consolidate_now         （usage hint — 提示带 confirm）\n  /consolidate_now confirm （触发 sweep）\n\n返回：完成后显「Consolidation finished in N ms · <LLM summary snippet>」（含本次 sweep 实际改了啥 — 合并了几条 / 删了几条等）。失败显错误原因。\n\n场景：owner 在 TG 端 sprint 整理 / 调 prompt 后想立即看 consolidate 行为而不等 cron（默认 6h interval）；或 audit「现在 consolidate 会怎么做」做基线。\n\n相关：/aware（pet 当前感知）；/here（owner 信号 snapshot）；桌面 PanelDebug「⏰ 下次 consolidate」chip 显 cron ETA。",
        "promote" => "🎯 /promote <title>\n\n用法：把任务 priority 升 +1（clamp 9 — 已是 P9 时不动 + 友好 reply）。一步操作不必算具体 P 值（与 /pri <title> <N> 互补 — pri 是绝对值，promote 是相对值）。保留所有其它 markers / due / body 不动（复用 task_set_priority 后端）。\n\n示例：\n  /promote 整理 Downloads\n  /promote 1   （/tasks 输出第 1 条）\n\nTitle resolve 与 /done / /cancel 同三层（数字 index → fuzzy → 错误候选）。相关：/pri（绝对设值）；/demote（-1 反方向）。",
        "demote" => "🎯 /demote <title>\n\n用法：把任务 priority 降 -1（clamp 0 — 已是 P0 时不动 + 友好 reply）。与 /promote 对偶 — owner 觉得「这条不那么急了」时一步降。保留所有其它 markers / due / body 不动。\n\n示例：\n  /demote 整理 Downloads\n  /demote 1   （/tasks 输出第 1 条）\n\nTitle resolve 与 /done / /cancel 同三层（数字 index → fuzzy → 错误候选）。相关：/pri（绝对设值）；/promote（+1 反方向）。",
        "due" => "📅 /due [preset]\n\n用法：列指定时段 due 的 pending 任务（含 due 字段 + 落在指定窗口的）。preset 缺省 tomorrow。\n\nPreset：\n  · tomorrow / tmr / tm / 明天 / 明日\n  · thisweek / this-week / week / 本周 / 这周（含 today 在内的 ISO Mon..Sun）\n  · nextweek / next-week / 下周\n\n示例：\n  /due\n  /due tomorrow\n  /due thisweek\n  /due 下周\n\n相关：/today 只看今日；/blocked 看锁住的。",
        "recent" => "🕒 /recent [N]\n\n用法：最近 N 条 done 任务标题（按 updated_at 倒序）。N 缺省 5，clamp 1..=20。\n\n示例：\n  /recent\n  /recent 10\n\n相关：/digest（同范围但含 [result:] 摘要）；/today（只看今日 done）；/tasks（全部状态）；/oldest_n（反向：最老 pending）。",
        "oldest_n" => "⌛ /oldest_n [N]\n\n用法：列本 chat 派单中最老 N 条 pending（按 created_at 升序，最早创建在前），audit「堆积最久的活」。N 缺省 5，clamp 1..=20。\n\n输出格式：\n  ⌛ 最老 N 条 pending（共 M，按 created_at 升序）：\n  · MM-DD HH:MM · <title> · N 天前\n  · ...\n\n与 /recent 反向 — recent 看「最新 done」（产出感），oldest_n 看「最老 pending」（积压感）。让 owner 觉察「我哪些活儿挂得最久 → 是否该 /pri 拉到高优 / /cancel 砍掉 / 重组」。\n\n仅 pending — error 不算（error retry 还在 active 池但属「试过失败」非「挂着没动」，语义偏弱）。\n\n示例：\n  /oldest_n\n  /oldest_n 10\n\n相关：/tasks（全状态清单）；/recent（最新 done）；/active_recent（反向：最新创建的 active）；/pri / /promote（重组优先级）；/cancel（砍掉）。",
        "active_recent" => "🆕 /active_recent [N]\n\n用法：列本 chat 派单中最近 N 条新创建的 active（pending / error）task（按 created_at 倒序，最新创建在前）。N 缺省 5，clamp 1..=20。\n\n输出格式：\n  🆕 最近 N 条新建 active（共 M，按 created_at 降序）：\n  · MM-DD HH:MM · <emoji> <title> · N 天前\n  · ...\n\n与 /recent 反向 — recent 看「最新 done」（产出感），active_recent 看「最新创建的活」（输入感）。让 owner 在 TG 上扫读「我最近塞了哪些活到队列」，比 /last（单条）多看几条，比 /tasks（全表 + compare_for_queue 排序）更聚焦活动段 + 创建时序。\n\nactive = pending + error（与 /tasks 同；error 也算「正在跑的轨道」 — 与 /oldest_n 仅 pending 不同 — 因为这里看的是「创建时序」非「堆积时长」）。\n\n示例：\n  /active_recent\n  /active_recent 10\n\n相关：/recent（最新 done — 反向）；/oldest_n（最老 pending — 创建时序反向）；/last（最近 1 条）；/tasks（全状态清单 + 智能排序）。",
        "digest" => "📋 /digest [N]\n\n用法：最近 N 条 done 任务的标题 + [result:] 摘要一行式（按 updated_at 倒序）。N 缺省 5，clamp 1..=20。\n\n示例：\n  /digest\n  /digest 10\n\n相关：/recent 同范围但只显标题（无 result 摘要时更紧凑）；/today 只看今日 done。",
        "edit" => "✏️ /edit <title> :: <new desc>\n\n用法：全量覆写指定 butler_task 的 description。`::` 是必填 separator — title 含空格 / 中文标点也能精确切。\n\n示例：\n  /edit 整理 Downloads :: 整理 Downloads [task pri=5 due=2026-05-20] [pinned]\n  /edit 写周报 :: 完整新 body 一段\n\n注意：**全量覆写**语义 — 新 desc 完全替换旧描述。想保留 `[task pri=...]` `[every: ...]` `[pinned]` 等 markers 请自行写进新 desc（命令不会自动续 markers）。Title resolve 与 /done / /cancel 同三层（数字 index → fuzzy → 错误候选）。",
        "edit_due" => "📅 /edit_due <title> <preset>\n\n用法：免手敲 ISO 日期改任务 due — preset 接友好词。preset 是 last whitespace token，剩余作 title（与 /pri / /promote / /demote 同 parser 模板，含空格 / 中文 title 也保）。复用 task_set_due 后端 — 与 ✏️ /edit 全量覆写正交，仅改 due 字段不动其它 markers。\n\nPreset 名单：\n\n  时刻类：\n    · tonight / 今晚 — 今晚 18:00（已过则明晚同点）\n    · tomorrow / 明天 / morning / 早上 / tmr — 明早 09:00\n    · monday..sunday / 周一..周日 / mon..sun — 本周（或下周如已过）该 weekday 09:00\n    · next_monday..next_sunday / 下周一..下周日 / next-mon..next-sun — 下周 weekday 09:00\n\n  相对类：\n    · +Nm — now + N 分钟\n    · +Nh — now + N 小时\n    · +Nd — N 天后 09:00（落次日早上而非「几天后此刻」避免午夜反直觉）\n\n  清除：\n    · clear / none / 0 / 清除 / 取消 — 清掉 due\n\n示例：\n  /edit_due 整理 Downloads tonight\n  /edit_due 写周报 next_friday\n  /edit_due 跑步 +30m\n  /edit_due 旧任务 clear\n\n相关：/pri <title> <N>（改 priority）；/promote / /demote（priority +/-1）；/snooze（暂停而非改 due）。",
        "reflect" => "🪞 /reflect <text>\n\n用法：把任意文本作 ai_insights memory item 存盘（反思 / 自我洞察分类，与 /note 写 general 对偶）。title 自动 `reflect-YYYY-MM-DDTHH-MM-SS`。\n\n示例：\n  /reflect 今天回顾：我对中断接受度过高，应该早点说 no\n  /reflect 观察：长 task 拆细后完成率明显提升\n\n相关：/note 写 general（杂项 brain-dump）；二者按「信号类型」分流避免 ai_insights 段被日常杂项稀释。可在 PanelMemory → AI 洞察 段查看 / 整理。",
        "find" => "🔍 /find <keyword>\n\n用法：搜本聊天派单（命中标题 / raw_description 子串，case-insensitive），至多 10 条。pending / error 浮顶。\n\n示例：\n  /find Downloads\n  /find 整理 桌面\n  /find #健身\n\n相关：/find_in_detail（搜 detail.md 内容）；/tasks（看全表）；/blocked（被锁住的）；/show（看单条详情）。",
        "find_in_detail" => "🔬 /find_in_detail <keyword>\n\n用法：搜本聊天派单的 detail.md 内容（case-insensitive 子串），至多 8 条命中。与 /find（仅扫标题 / raw_description）互补 — pet 在 detail.md 写过相关进度 / 复盘但标题没体现时本命令命中。\n\n输出格式：\n  🔬 命中「<kw>」N 条（detail.md 内容搜索）：\n  🟢 <title>\n     …<snippet 含 keyword 60 字 context>…\n  ⚠️ <title>\n     …\n  ...\n\nsnippet 取 keyword 命中点附近 60 字 context；超长 + …。\n\n示例：\n  /find_in_detail rebase\n  /find_in_detail TODO\n  /find_in_detail 决策\n\n注：每次命令读所有派单的 detail.md（IO 较重），不必过分频繁。owner 想「快速过一遍标题」走 /find；想「我笔记里写过 X」走本命令。\n\n相关：/find（扫标题 + 描述）；/show <title>（看单条 raw + detail 预览）；/timeline（看历史变化）。",
        "show" => "🔬 /show <title>\n\n用法：显单条任务完整 raw description（含 [task pri=...] / [every:] / [pinned] 等所有 markers）+ detail.md 内容预览（前 300 字符）。Title resolve 与 /done / /cancel 同三层（数字 index → fuzzy → 错误候选）。\n\n示例：\n  /show 整理 Downloads\n  /show 1  （/tasks 输出第 1 条）\n\n相关：/find 搜任务；/edit 改 description；/tasks 看清单。让 owner 在 TG 端 audit 任务详情不必回桌面。",
        "peek" => "👀 /peek <title>\n\n用法：一行紧凑视图 — status emoji + 标题 + schedule（every / once / deadline 摘要）+ 关键 markers（📌 pinned / 🔇 silent / 💤 snoozed / 🔒 blockedBy）+ P{priority}。与 /show 显完整 raw + detail.md 预览互补 — owner 想「快瞄一眼这条状态」用 /peek，要看完整 description 走 /show。\n\nTitle resolve 与 /done / /cancel / /show 同三层（数字 index → fuzzy → 错误候选）。\n\n输出格式：\n  ⏳ 「<title>」 · 🕐 every 09:00 · 📌 🔇 💤 · P3\n\nschedule 段：[every: HH:MM] / [once: YYYY-MM-DD HH:MM] / [deadline: ...] / [every: 工作日 HH:MM] 等都识别；无 schedule 前缀 → 省略。\n\nmarkers 段：仅显非空 — 没钉不显 📌；没 snoozed 不显 💤。整条 markers 都没 → 段省略。\n\nP{n}：从 [task pri=N] 提取，缺省（无 pri marker）→ 省略。\n\n示例：\n  /peek 整理 Downloads\n  /peek 1  （/tasks 输出第 1 条）\n\n相关：/show <title>（完整 raw + detail）；/tasks（清单视图）；/timeline（历史演化）。",
        "dup" => "📑 /dup <title>\n\n用法：复制一条 task 为新 pending 实例 — title 加「(副本)」后缀，priority + due 继承源 task。owner 想「以这条为模板建一条相似的」时一键完成，免「复制 raw → 编辑去掉终态 markers → /task 重建」三步。\n\n继承的：[every:] / [once:] / [deadline:] / [reminderMin:] schedule + [pinned] / [silent] / [blockedBy:] markers + #tags + priority + due + body 文本。\n\n剥掉的：[done] / [result:] / [error:] / [cancelled:] / [archived:] / [snooze:] / [origin:tg:] — 这些是「原 task 实例」专属信号，副本应回 pending 重新起跑。\n\nTitle 冲突兜底：memory_edit 内置 unique-filename — 同 title 重复 dup 会变 `<src>_(副本)_1` / `_2` ...自动加序号。\n\n空 title → usage hint；title resolve 与 /done / /cancel / /show 同三层（数字 index → fuzzy → 错误候选）。\n\n示例：\n  /dup 整理 Downloads          → 「整理 Downloads (副本)」\n  /dup 1                        （/tasks 输出第 1 条）\n  /dup 写周报                   → 「写周报 (副本)」（继承 every + reminderMin + #work）\n\n输出格式：\n  📑 已复制「<src>」→「<new>」\n  · 继承 schedule / markers / tags / priority / due\n  · 剥终态 markers（done / result / snooze / origin 等）\n\n相关：/edit <title> :: <new desc>（覆写而非复制）；/show 看 raw 验证 markers；/tasks 看新 task 入列。",
        "snippets" => "📎 /snippets\n\n用法：列本聊天派单中含 `[snippet]` 或 `[snippet: <label>]` marker 的 task — 「可复用片段」分类清单。owner 用此 marker 标 prompt 模板 / 决策清单 / 常用回复 / 流程 checklist 等想反复用的内容，本命令一眼看「我都标了哪些 snippet」+ label + body 前 80 字预览。\n\nmarker 约定：\n  [snippet]              （无 label，简单标记为「可复用」）\n  [snippet: 模板A]      （含 label — 后续 /show / /dup 时一眼识别用途）\n  [snippet: PR template]（label 可为任意非 `]` 字符）\n\n输出格式：\n  📎 snippets · N 条：\n  🟢 <title> [模板A]\n     <body 前 80 字预览>\n  🟢 <title>\n     <body 前 80 字预览>\n  ...\n\nN === 0 时友好兜底：「本聊天派单还没标 snippet — 在 /edit 中给 task 加 `[snippet]` / `[snippet: <label>]` marker 后回来 audit」+ 教学例。\n\n场景：sprint 整理常用 prompt；/dup 一个 snippet 改装为新任务模板（/dup 保 markers 含 [snippet] — 副本也是 snippet）；写决策日志时回看 last week 我标了哪些可复用的。\n\n示例：\n  /snippets\n\n相关：/show <title>（看完整 raw + detail）；/dup <title>（克隆改装）；/markers（含 pinned + silent 联合视图，未来可扩 snippets 进 markers 矩阵）。",
        "recent_events" => "📜 /recent_events <title> [N]\n\n用法：给单条 task 显最近 N 个 butler_history 事件的紧凑一行视图 — TL;DR 视角。与 /timeline 完整视图互补 — owner 想「这条 task 最近发生了啥」时本命令更快，要看完整演化走 /timeline。\n\nN 缺省 5；clamp 1..=20（与 /recent / /digest / /show_speech 同协议）。空 title → usage hint；title resolve 与 /done / /cancel / /show / /timeline 同三层（数字 index → fuzzy → 错误候选）。\n\n输出格式：\n  📜 「<title>」最近 N 个事件（共 M）：\n  📝 MM-DD HH:MM · 创建\n  ✏️ MM-DD HH:MM · [pinned]\n  ✏️ MM-DD HH:MM · [snooze: 18:00]\n  ✏️ MM-DD HH:MM · [done] [result: ok]\n  …\n\n与 /timeline 区别：\n- /timeline 显全部去重事件（cap 30）— 「这条 task 一生」audit\n- /recent_events 仅显最近 N（cap 20）— 「最近怎么样」快瞄\n- 两者底层同 butler_history → compute_timeline_entries 路径\n\n示例：\n  /recent_events 整理 Downloads          （显最近 5 条）\n  /recent_events 整理 Downloads 10       （显最近 10 条）\n  /recent_events 1                        （/tasks 第 1 条最近 5 条）\n  /recent_events 1 10                     （第 1 条最近 10 条）\n\n注意：title 仅 1 token 且是数字时一律视作 title（如 /recent_events 5 = 「第 5 条 task 最近 5 条」而非「最近 5 条无 title」）；想要带 N 显式两 token（/recent_events <title> <N>）。\n\n相关：/timeline（全 audit）；/show（当前 snapshot）；/peek（一行紧凑视图）。",
        "touched_today" => "📅 /touched_today\n\n用法：列今日 updated_at 命中的本聊天派单（任意状态），按时间倒序。回答「我今天动过哪些 task」— 含 owner action（pinned / silent / snooze / promote / touch / edit）+ LLM update（result write / detail.md 写过）+ 状态变化（done / error / cancelled）所有引发 updated_at 前进的动作。\n\n空 → 友好兜底（教学指向 /today / /today_done）。\n\n输出格式：\n  📅 今日（YYYY-MM-DD）动过 N 条（按时间倒序）：\n  · ⏳ HH:MM 整理 Downloads\n  · ✅ HH:MM 写周报 — done with result\n  · 💤 HH:MM 写报告\n  · ⏳ HH:MM review PR\n\n状态 emoji：⏳ pending · ✅ done · ⚠️ error · 🚫 cancelled · 💤 snoozed（pending 且含 [snooze:] marker）\n\n与 /today_done（仅 done）/ /today（今日 due）区别：\n- /today_done：done + updated_at 在今日 — 只看「完成产出」\n- /today：pending + due 在今日 + done 在今日两段 — 「今日叙事」视图\n- /touched_today：任意状态 + updated_at 在今日 — 「我今天动过」全谱（含 pending 调整 / snooze / pin / silent 等 owner action）\n\n场景：sprint 复盘「我今天到底做了 / 调了 / 推后了哪些」；夜里 audit owner 今日工作量；与 /today_done 对比 audit「动了但没完成」的进度感\n\n示例：\n  /touched_today\n\n相关：/today_done（仅完成）；/today（今日 due + done 叙事）；/yesterday（昨日产出）；/recent_events <title>（单 task TL;DR）。",
        "edit_title" => "✏️ /edit_title <title> :: <new title>\n\n用法：仅改 task 标题，不动 description / detail.md / markers。`::` 是必填 separator — title 含空格 / 中文标点也能精确切。前端 PanelTasks 已有 double-click inline rename；本命令是 TG 端对偶。\n\n与 /edit（全量覆写 description）区别：\n- /edit：覆写 description body — markers 需自己写进 new desc\n- /edit_title：只换标题字符串 — 所有 markers / body / detail.md 不动\n\n后端：复用既有 `memory_rename` Tauri 命令 — index 项改 title + .md 文件 move + 同名冲突自动加 `_N` 后缀（与 /dup unique-filename 同 fallback）。\n\nTitle resolve 与 /done / /cancel / /show 同三层（数字 index → fuzzy → 错误候选）。\n\n输出格式：\n  ✏️ 已改标题：「<old>」→「<new>」\n\n注意：rename 后既有 `[task: 「<old>」]` ref / detail.md 内 `「<old>」` token 不自动跟随更新（owner 需手动改）。考虑后续 iter 加 cascade rename。\n\n示例：\n  /edit_title 整理 Downloads :: 清理桌面（更详细名）\n  /edit_title 1 :: 重命名（/tasks 第 1 条）\n  /edit_title 写周报 :: 写 2026-W20 周报\n\n相关：/edit（覆写 description）；/dup（克隆为新 task）；/show（看 rename 后的 raw）。",
        "touched_thisweek" => "📅 /touched_thisweek\n\n用法：本周（自周一 00:00 起到 now）updated_at 命中的本聊天派单（任意状态），按时间倒序。「这周我动过哪些 task」周维度复盘视角。\n\n场景：周五整理本周产出 / 周末看「这周我都做了 / 调了 / 推后了哪些」/ 写周报需要本周完整动作清单。与 /touched_today / /touched_yesterday 三件套形成日 × 周 多 scope。\n\n周边界：周一 00:00 起算（ISO weekday：1=Mon ... 7=Sun）— 周日晚上 23:59 时仍算本周；周一 00:00 起算「新本周」。\n\n输出格式：\n  📅 本周（YYYY-MM-DD 起）动过 N 条（按时间倒序）：\n  · ⏳ MM-DD HH:MM 整理 Downloads\n  · ✅ MM-DD HH:MM 写周报 — done with result\n  · 💤 MM-DD HH:MM 写报告\n  ...\n\n状态 emoji 同 /touched_today（⏳ pending · ✅ done · ⚠️ error · 🚫 cancelled · 💤 snoozed pending）。每行带 MM-DD HH:MM（跨日 scope 不能省 date）。\n\n空 → 友好兜底（教学指 /touched_today / /tasks）。\n\n示例：\n  /touched_thisweek\n\n相关：/touched_today（仅今日）；/touched_yesterday（仅昨日）；/digest_yesterday（昨日 done + result）；/yesterday（昨日 done 仅标题）。",
        "touched_yesterday" => "📅 /touched_yesterday\n\n用法：/touched_today 的昨日对偶 — 列昨日（本地日历日）updated_at 命中的本聊天派单（任意状态），按时间倒序。复盘视角：「昨天我动过哪些 task」。\n\n场景：早上 standup 前回顾「昨天做了 / 调了 / 推后了哪些」；周末 audit 工作日 backlog 变化；与 /yesterday（仅 done）/ /today_done 三件套形成完整 today-yesterday × 全谱-完成 audit 矩阵。\n\n输出格式：\n  📅 昨日（YYYY-MM-DD）动过 N 条（按时间倒序）：\n  · ⏳ HH:MM 整理 Downloads\n  · ✅ HH:MM 写周报 — done with result\n  · 💤 HH:MM 写报告\n  · ⏳ HH:MM review PR\n\n状态 emoji 同 /touched_today（⏳ pending · ✅ done · ⚠️ error · 🚫 cancelled · 💤 snoozed pending）。\n\n空 → 友好兜底（教学指向 /touched_today / /yesterday / /tasks）。\n\n示例：\n  /touched_yesterday\n\n相关：/touched_today（今日全谱）；/yesterday（昨日 done）；/today_done（今日 done）；/recent_events <title>（单 task TL;DR）。",
        "oldest_done" => "🪦 /oldest_done [N]\n\n用法：列**最早完成**的 N 条 done task（按 updated_at 升序）— 与 /recent done（最近完成）反向。让 owner 看「这些任务我做了多久 / 哪些是 ancient backlog 终于完成」的考古视角。\n\nN 缺省 5；clamp 1..=20（与 /recent / /digest / /show_speech 同协议）。无 done task → 友好兜底教学指向 /done 标完成。\n\n输出格式：\n  🪦 最早完成的 N 条（共 M done）：\n  · YYYY-MM-DD HH:MM · <title>\n  · YYYY-MM-DD HH:MM · <title>\n  ...\n\n（与 /recent 同 line 格式 — 让 owner 切换视角时心智一致）\n\n场景：\n- 「这条 task 我做了多久」考古 — 比对源 create_at（/show 含）vs 最早 done updated_at\n- audit 「最老的 done 是何时」— sprint 复盘 / quarterly review\n- 与 /recent done 形成「最近完成 vs 最早完成」镜像\n\n示例：\n  /oldest_done           （显最早 5 条）\n  /oldest_done 10        （显最早 10 条）\n\n相关：/recent（最近完成 — 与本命令反向）；/oldest_n（最老 pending — pending 维度反向）；/yesterday / /today_done（按日期范围而非「最老/最新」）。",
        "tags_thisweek" => "🏷 /tags_thisweek\n\n用法：/tags_today 的本周对偶 — 仅扫本周（自周一 00:00 起到 now）updated_at 命中的 task 含的 #tag 计数。周报场景下「本周我都在哪些主题工作」audit。无参。\n\n空 → 友好兜底（/tags 全量 / /tags_today 今日 alt）。\n\n输出格式：\n  🏷 本周（YYYY-MM-DD 起）N 个 tag\n  · #健身 ×5\n  · #API ×3\n  ...\n  \n  无 #tag 任务（本周）：N 条\n\n示例：\n  /tags_thisweek\n\n相关：/tags（全量）；/tags_today（今日）；/tags_yesterday（昨日）；/touched_thisweek（本周全谱无 tag 聚合）。",
        "tags_yesterday" => "🏷 /tags_yesterday\n\n用法：/tags_today 的昨日对偶 — 仅扫昨日 updated_at 命中的 task 含的 #tag 计数。复盘视角，写日报 / 早会回顾「昨天我在哪些主题工作」时用。无参。\n\n空 → 友好兜底（/tags 全量 / /tags_today 今日 alt）。\n\n输出格式：\n  🏷 昨日（YYYY-MM-DD）N 个 tag\n  · #健身 ×3\n  · #API ×2\n  ...\n  \n  无 #tag 任务（昨日）：N 条\n\n示例：\n  /tags_yesterday\n\n相关：/tags（全量不限日期）；/tags_today（今日）；/touched_yesterday（昨日全谱无 tag 聚合）。",
        "tags_today" => "🏷 /tags_today\n\n用法：/tags 的今日切片 — 仅扫今日 updated_at 命中的 task 含的 #tag 计数。让 owner 看「今天我在做哪些主题」audit。无参 — 今日范围天然小，不需 cap。\n\n场景：早会前看「今天我在哪些主题上工作」/ 写日报需按 tag 分类列项 / sprint 中段「我今天偏向某主题太多 / 太少」audit。\n\n输出格式：\n  🏷 今日（YYYY-MM-DD）N 个 tag：\n  · #健身 ×3\n  · #API ×2\n  · #周报 ×1\n  ...\n  \n  无 #tag 任务：N 条\n\n空 → 友好兜底「今日动过的 task 都无 #tag」+ 教学 /tags 全量入口。\n\n示例：\n  /tags_today\n\n相关：/tags（全量不限日期）；/touched_today（今日 task 全谱无 tag 聚合）；/tag <name>（按某 tag 搜任务）。",
        "random_pinned" => "🎲 /random_pinned\n\n用法：/random 的 pinned 子集 — 从 pinned task 里随机抽 1 条让 pet 推荐。owner 钉了几条都重要 / 不知先做哪条时用此命令让 pet 决定下一步。无参；多次调用得不同 pinned task。\n\n空 → 友好兜底（教学指 /pin <title> 设置 + /random 全 active 集 fallback）。\n\n输出格式（与 /random 同）：\n  🎲 抽中 ⏳ 「<title>」（共 N 条 pinned active）\n  \n  <raw_description 前 200 字>\n  \n  —— 选择困难？就先做这条吧。\n\n示例：\n  /random_pinned\n\n相关：/random（全 active 集）；/pinned（看 pinned 清单）；/peek_pinned（pinned 紧凑视图）。",
        "peek_pinned" => "👀 /peek_pinned\n\n用法：所有 pinned task 一行紧凑视图 — /pinned（仅标题）的密集版 + /peek（单条紧凑）的批量版。每行：`<status_emoji> 「<title>」 · 🕐 <schedule> · <markers>`，让 owner 一眼看「我钉的 N 条状态如何」。\n\n场景：早会前确认「我今天必须看的几条 task」状态 / sprint 中段 audit「钉的事情进度怎样」/ 晚上看「我钉了哪些没动」。\n\n空 → 友好兜底「暂无 pinned task」+ 教学指 /pin <title> 设置。\n\n输出格式：\n  📌 N 条 pinned：\n  ⏳ 「<title>」 · 🕐 every 09:00 · 📌 🔇\n  ✅ 「<title>」 · 🕐 once 2026-05-20 14:00\n  ⏳ 「<title>」 · 📌 💤\n  ...\n\n状态 emoji 与 /peek / /find 同：⏳ pending · ✅ done · ⚠️ error · 🚫 cancelled。Schedule 段 + markers 段都仅命中时显（与 /peek 行为一致）。\n\n示例：\n  /peek_pinned\n\n相关：/pinned（仅标题）；/peek <title>（单条紧凑）；/pinned_due（pinned 且有 due）；/tasks（全量含 pinned）。",
        "alarms_thisweek" => "⏰ /alarms_thisweek\n\n用法：/alarms_today 的本周对偶 — 仅显本周（自周一 00:00 起到 now）触发的 reminder（`[remind: ...]` 协议条目）。让 owner 看「本周还会响哪些 / 已逾期未消」。无 N 参 — 本周范围比 today 略广但仍可控（典型 < 30 条）。\n\n场景：周报场景看「这周我设了哪些 reminder / 哪些已 fire 哪些待响」/ 周一早会前 review 上周未消 alarm。\n\n输出格式：\n  ⏰ 本周（YYYY-MM-DD 起）N 条 alarms：\n  · MM-DD HH:MM (剩 / 已逾期 ...) | <topic>\n  · MM-DD HH:MM (剩 ...) | <topic>\n  ...\n\n跨日 scope 行带 MM-DD（与 /alarms 同；/alarms_today 行只 HH:MM 因 single day）。空 → 友好兜底指 /alarms 全量 / /alarms_today。\n\n示例：\n  /alarms_thisweek\n\n相关：/alarms（不限日期 top N）；/alarms_today（仅今日）；/touched_thisweek（本周 task 全谱）。",
        "alarms_today" => "⏰ /alarms_today\n\n用法：/alarms 的今日切片 — 仅显本地今日触发的 reminder（`[remind: HH:MM]` 协议 + 今日 `[remind: YYYY-MM-DD HH:MM]` Absolute target）。让 owner 一眼看「今天还会响哪些 / 哪些已逾期未消」。\n\n无 N 参 — 今日范围天然小（典型 < 10 条），不需 cap；与 /alarms 全量按 N（缺省 5）有意区分。\n\n输出格式：\n  ⏰ 今日（YYYY-MM-DD）N 条 alarms：\n  · HH:MM (剩 N 分 / 已逾期 N 分) | <topic>\n  · HH:MM (剩 N 分) | <topic>\n  ...\n\n空 → 友好兜底「今日暂无 alarm」+ 教学指 /alarms 看 N day window。\n\n场景：早上看「今天会响哪些 reminder」/ 中午想「下午还有几个 alarm」/ 晚上 audit 「今天有几个被我忽视的」。\n\n示例：\n  /alarms_today\n\n相关：/alarms（不限日期 N 条）；/touched_today（今日动过的 task，含 reminder）；/today（今日 due task）。",
        "cat_top" => "📊 /cat_top [N]\n\n用法：按 cat items 总量 desc 列前 N 个 cat — 跨 cat 容量对比 audit。N 缺省 5，clamp 1..=20。\n\n场景：新人看 pet 「我都积了哪类知识 / 哪 cat 主力」概览；季度规划「需 archive / consolidate 哪 cat 大」；comparing「主力 cat（item 多）vs 边缘 cat（item 少）」分布。\n\n输出格式：\n  📊 cat top N（按 items 总量 desc）：\n  · butler_tasks · 156 条\n  · decisions · 89 条\n  · general · 42 条\n  ...\n  \n  (共 M cat in memory index)\n\n空 → 友好兜底「memory index 内无 cat」。\n\n示例：\n  /cat_top        （前 5）\n  /cat_top 10     （前 10）\n  /cat_top 20     （前 20）\n\n相关：/help_table cat（cat 家族详细 list）。",
        "audit_summary" => "📋 /audit_summary\n\n用法：单命令聚合 audit 信号 — sprint kickoff / 月度复盘一键视图。无参。\n\n场景：周一早会前 30 秒看「上周怎么样 / 本周从哪起」；月末看「本月节奏整体如何」。\n\n输出格式：\n  📋 audit summary（YYYY-MM-DD）\n  · 📌 pin streak: N 天连续（当前 M 钉）\n  · 💤 idle 7d+: P 条 stale pending → /idle_7d\n  · ✅ 今日 touched: Q 条 → /touched_today\n  · 🏷 近 7d rename: R 次\n\n实现：handler 调既有 helper（compute_pin_streak / read_tg_chat_task_views / butler_history scan）的聚合。\n\n示例：\n  /audit_summary\n\n相关：/help_table（命令分组速查）；/idle_7d / /touched_today 各 deep dive 入口。",
        "help_table" => "📚 /help_table\n\n用法：按 audit family 分组列既有命令 — 命令爆炸后的 navigation aid。/help 是 flat 一行描述全表；本命令按主题分组让 owner 快定位「这个 audit 在哪个命令族」。无参。\n\n场景：新用户上手「pet 都能干啥」；老用户想用某 audit family 时 jog memory；写 onboarding 文档时按主题列举。\n\n输出格式（每组 emoji + family 名 + 命令清单一行）：\n  📚 命令分组速查表\n  📌 pin 关注度：/pin /unpin /pinned /pinned_due /...\n  📚 cat：/cat_top\n  🔁 rename 重命名：/edit_title /...\n  💤 idle / stale：/idle_7d /touched_today /...\n  🔥 streak 连续：/streak\n  🔎 find / search：/find /find_in_detail /...\n  🗣 speech / 对话：/last_speech /show_speech /...\n  ⏰ alarm / 通知：/alarms /alarms_today /mute /...\n  📊 status / overview：/tasks /stats /buckets /show /...\n  📋 增删改：/task /done /cancel /edit /...\n  ⚠️ batch / 危险：/cancel_all_error /promote_all_p7 /...\n  ⚙️ system：/version /help /help_table /reset\n\n相关：/help（flat 全表 + 一行描述）；/help <cmd>（单命令详细用法 + 示例）；/help search <kw>（关键词搜全文）。",
        "recent_pins" => "📌 /recent_pins [N]\n\n用法：列近 N 条 pin 决策（每 title 取 history 内最早 [pinned] sighting 后 desc 排）。看「最近 N 条 pin 决策」cross-task audit 不限日期。N 缺省 5，clamp 1..=20。\n\n场景：周末 / 月末 review 「最近 N 条 pin 我都钉的什么」list-up；audit 「哪些 task 我曾认真钉过」即使现已 unpin 仍可见。\n\n后端：scan butler_history.log 取所有含 [pinned] snippet 行 → dedupe 按 title 保留最早 sighting → 按 ts desc 排 → cap N。dedupe 让同 title 多次 update（pin 状态不变）只算 1 次「决策事件」。\n\n输出格式：\n  📌 近 N 条 pin 决策（共 M 条 retention 内）：\n  · MM-DD HH:MM · 「整理 Downloads」\n  · MM-DD HH:MM · 「写周报」\n  ...\n\n空 → 友好兜底「butler_history 内无 [pinned] sighting」+ 教学指 /pin / /pinned。\n\n注（best-effort 局限）：\n- snippet 80 字截断可能漏 [pinned] → false neg\n- retention 限（典型 100 entry cap）\n- 含已 unpin / done / archived 的 task — 「pin 决策」是历史事件不必当前仍 active\n\n示例：\n  /recent_pins        （近 5 条）\n  /recent_pins 10     （近 10 条）\n\n相关：/pinned（当前 pinned 清单）。",
        "idle_7d" => "💤 /idle_7d\n\n用法：列「pending 且 updated_at ≥ 7 天前」的 task — stale backlog audit。PanelTasks 「💤 N 条 7d+ idle」chip 的 TG 端对偶。无参。\n\n场景：周末整理 backlog「这周搁着的有几条 — 推 / 完 / 弃 决定」；早会前看「卡了多久的活」决定优先；月度复盘「stale 累积是否健康循环」。\n\n输出格式：\n  💤 stale backlog N 条（pending + updated_at ≥ 7 天前）：\n  · 「<title>」 · idle 14 天（last update 2026-05-04）\n  · 「<title>」 · idle 9 天（last update 2026-05-09）\n  ...\n\n按 idle 天数 desc 排（最老 stale 在上）— owner 先看最该处理的。cap 12 条。\n\n空 → 友好兜底「无 7d+ idle pending — 健康状态」+ 教学指 /touched_thisweek（看本周活跃 task）。\n\n注：本命令只看 pending 状态 — done / cancelled / error 不进 inactivity 语义。snoozed pending（含 [snooze:] marker）仍算 idle — 因为 snooze 也是 owner action，超 7d 没醒来 audit 是合理的。\n\n示例：\n  /idle_7d\n\n相关：/touched_thisweek（本周活跃 task）；/oldest_n（按 created_at 最老）；PanelTasks「💤 N 条」chip（桌面端同 audit）。",
        "find_in_detail_yesterday" => "🔬 /find_in_detail_yesterday <keyword>\n\n用法：/find_in_detail_today 的昨日对偶 — 限昨日 updated_at task 的 detail.md 内容搜（case-insensitive 子串 + 60 字 snippet）。「昨天我在某主题写过什么笔记」复盘视角。\n\n空 keyword → usage hint；无命中 → 友好兜底（/find_in_detail 全量 / /touched_yesterday 全谱 alt）。\n\n输出格式：\n  🔬 昨日（YYYY-MM-DD）命中「<kw>」N 条（detail.md 内容）：\n  🟢 <title>\n     …<snippet 60 字 context>…\n  ...\n\nsnippet 算法与 /find_in_detail 同。状态 emoji 同 /find_in_detail 系列。cap 8 条。\n\n示例：\n  /find_in_detail_yesterday rebase\n  /find_in_detail_yesterday API\n\n相关：/find_in_detail（不限日期）；/find_in_detail_today（今日）；/touched_yesterday（昨日全谱无 kw）。",
        "find_in_detail_today" => "🔬 /find_in_detail_today <keyword>\n\n用法：/find_in_detail 的今日切片 — 仅扫今日 updated_at 命中的 task 的 detail.md 内容，找含 keyword（case-insensitive 子串）的 task + 命中点 60 字 snippet。「我今天在某主题写过什么笔记」精准 audit。\n\n场景：早会前回忆「今天我在 detail.md 写过 X 相关的进度」/ 深夜 review「今天我的笔记记了哪些 API 相关」/ 决策日志 audit「今天关于 deploy 的决策点」。\n\n空 keyword → usage hint；无命中 → 友好兜底（/find_in_detail 全量 / /touched_today 全谱 alt）。\n\n输出格式：\n  🔬 今日（YYYY-MM-DD）命中「<kw>」N 条（detail.md 内容）：\n  🟢 <title>\n     …<snippet 含 keyword 60 字 context>…\n  ⚠️ <title>\n     …\n  ...\n\nsnippet 算法与 /find_in_detail 同（命中点附近 60 字，换行 flatten 单空格）。状态 emoji 同 /find_in_detail 系列：🟢 pending · ⚠️ error · ✅ done · 🚫 cancelled。cap 8 条（与 /find_in_detail 同上限）。\n\n注：每次命令读今日所有 task 的 detail.md（IO 重，但今日 scope 比 /find_in_detail 全量 IO 少）— 仍不必过分频繁。\n\n示例：\n  /find_in_detail_today rebase\n  /find_in_detail_today API\n  /find_in_detail_today 决策\n\n相关：/find_in_detail（不限日期 detail.md 内容搜）；/search_today（限今日扫标题 + description）；/touched_today（今日全谱无 kw）。",
        "search_thisweek" => "🔎 /search_thisweek <keyword>\n\n用法：/search_today 的本周对偶 — 在本周（自周一 00:00 起到 now）updated_at 命中的本聊天派单内 fuzzy 搜 title / raw_description（case-insensitive 子串）。「本周与 X 相关的」精准 audit。\n\n场景：周五写周报 / 周末整理本周产出 / 写月报需筛本周某主题 — 比 /find 全量更聚焦，比 /touched_thisweek 全谱更精准。完成 kw × today/yesterday/thisweek 三件套矩阵。\n\n输出格式：\n  🔎 本周（YYYY-MM-DD 起）命中「<kw>」N 条：\n  🟢 <title>\n  ⚠️ <title>\n  ✅ <title>\n  ...\n\n空 keyword → usage hint；无命中 → 友好兜底（/find 全量 / /touched_thisweek 全谱 alt）。状态 emoji 同 /search_today 系列。cap 10 条。\n\n示例：\n  /search_thisweek API\n  /search_thisweek 周报\n\n相关：/search_today（仅今日）；/search_yesterday（仅昨日）；/find（不限日期）；/touched_thisweek（本周全谱）。",
        "search_yesterday" => "🔎 /search_yesterday <keyword>\n\n用法：/search_today 的昨日对偶 — 在**昨日 updated_at**命中的本聊天派单内 fuzzy 搜 title / raw_description（case-insensitive 子串）。「昨天我做的与 X 相关的」复盘视角。\n\n场景：早会前回顾「昨天处理过的 API 相关 task」/ 周一回顾「上周五碰过的 deploy issue」（注：昨日 = 本地日历日，跨周末取周日为昨日）/ 写日报需要昨天进展时筛 X 相关。\n\n空 keyword → usage hint。无命中 → 友好兜底 + alt 入口（/find / /touched_yesterday）。\n\n输出格式：\n  🔎 昨日（YYYY-MM-DD）命中「<kw>」N 条：\n  🟢 <title>\n  ⚠️ <title>\n  ✅ <title>\n  ...\n\n状态 emoji 同 /search_today / /find：🟢 pending · ⚠️ error · ✅ done · 🚫 cancelled。cap 10 条。\n\n示例：\n  /search_yesterday API\n  /search_yesterday 周报\n  /search_yesterday #健身\n\n相关：/search_today（今日同模板）；/find（全量不限日期）；/touched_yesterday（昨日全谱不限 kw）；/digest_yesterday（昨日 done + result）。",
        "search_today" => "🔎 /search_today <keyword>\n\n用法：在**今日 updated_at**命中的本聊天派单内 fuzzy 搜 title / raw_description（case-insensitive 子串）。「今天我做的与 X 相关的」精准 audit 入口 — /find（全量）vs /touched_today（无 kw，列今日全谱）vs 本命令（今日 + kw）三件套。\n\n场景：早会前回顾「今早处理过的 'API' 相关 task」/ 下午找「今天碰过的 deploy 相关 issue」/ 写日报时筛「今天关于 X 的进度」。\n\n空 keyword → usage hint。无命中 → 友好兜底 + alt 入口（/find / /touched_today）。\n\n输出格式：\n  🔎 今日（YYYY-MM-DD）命中「<kw>」N 条：\n  🟢 <title>\n  ⚠️ <title>\n  ✅ <title>\n  ...\n\n状态 emoji 同 /find：🟢 pending · ⚠️ error · ✅ done · 🚫 cancelled。同状态保 views 原序（compare_for_queue 综合序）。cap 10 条（与 /find 同上限）。\n\n示例：\n  /search_today API\n  /search_today 周报\n  /search_today #健身\n\n相关：/find（不限日期 fuzzy 搜）；/touched_today（今日全谱不限 kw）；/digest_yesterday（昨日 done + result）；/show <title>（看单条 raw + detail）。",
        "digest_thisweek" => "📋 /digest_thisweek [N]\n\n用法：本周（自周一 00:00 起到 now）done task 标题 + [result:] 摘要一行式。/digest 的本周对偶 — 那个按 updated_at desc 取最近 N（可能跨周），本命令限本周 calendar range。\n\nN 缺省 5，clamp 1..=20（与 /digest / /recent 同协议）。空（本周无 done）→ 友好兜底教学指 /digest / /touched_thisweek / /yesterday。\n\n输出格式：\n  📋 本周（YYYY-MM-DD 起）完成 N 条（共 M done）：\n  · MM-DD HH:MM · <title> — <result 前 80 字>\n  · MM-DD HH:MM · <title> — <result>\n  ...\n\n跨日 scope — 行 MM-DD HH:MM（与 /digest 同；/digest_yesterday 是 HH:MM only 因 single-day scope）。result 截 80 字 + …。\n\n场景：周五写周报；周末整理本周产出；月报 / quarterly review 时按周聚合。\n\n示例：\n  /digest_thisweek          （本周 done 5 条）\n  /digest_thisweek 10       （本周 done 10 条）\n\n相关：/digest（按更新时序 N 条 done，不限日期）；/digest_yesterday（昨日 done）；/touched_thisweek（本周任意状态）。",
        "digest_yesterday" => "📋 /digest_yesterday [N]\n\n用法：昨日（本地日历日）done task 标题 + [result:] 摘要一行式。与 /digest 的区别：那个按 updated_at 倒序取最近 N 条（可能跨多日 / 今日为主），本命令限定昨日 calendar day — 「昨天我完成了哪些 + 产物是什么」复盘视角。\n\nN 缺省 5，clamp 1..=20（与 /digest / /recent 同协议）。空（昨日无 done）→ 友好兜底教学指向 /digest / /yesterday / /touched_yesterday。\n\n输出格式：\n  📋 昨日（YYYY-MM-DD）完成 N 条（共 M done）：\n  · HH:MM · <title> — <result 前 80 字>\n  · HH:MM · <title> — <result>\n  ...\n\nresult 截 80 字 + …（与 /digest / /yesterday 同 cap）。\n\n场景：早会前看「昨天我做了什么 + 怎么做的」；周五整理本周产出；与 /yesterday（昨日 done 仅标题）/ /touched_yesterday（昨日任意状态全谱）三件套形成完整 yesterday audit 矩阵。\n\n示例：\n  /digest_yesterday        （昨日 done 5 条）\n  /digest_yesterday 10     （昨日 done 10 条）\n\n相关：/digest（按更新时序 N 条 done，不限日期）；/yesterday（昨日 done 仅标题无 result）；/touched_yesterday（昨日任意状态）。",
        "mute_today" => "🌙 /mute_today\n\n用法：一键静音 proactive 到**本地午夜**（次日 00:00），免 owner 算「还多少分钟到午夜」。与 /mute N（任意分钟）/ /sleep_until <HH:MM>（任意目标时刻）互补 — 本命令是「今夜不打扰」的常用预设。\n\n后端：算 `now → 次日 00:00` 的分钟数 → 调 `set_mute_minutes(minutes)` 同既有 /mute 路径。clamp 1..=1440（绝不超过 24h）— 极端 DST 边界兜底。\n\n输出格式：\n  🌙 已静音 proactive 到本地午夜（00:00）— 还 N 分钟（M 小时）\n\n场景：晚上 10 点开始写决策日志 / 看书 / 睡前；想说「今夜别再打扰我」时不必心算「到午夜还几分钟」。\n\n注：到点后 mute 自然解除 — pet 早上首 schedule 仍触发。如想跨天静音走 /mute N 或 /sleep_until 明早时刻。\n\n示例：\n  /mute_today\n\n相关：/mute N（任意 N 分钟）；/sleep_until HH:MM（任意目标时刻，含明日）；/sleep（一键 8h）；/here（看 owner 当前 mute 状态）。",
        "cascade_rename" => "🔁 /cascade_rename <title> :: <new title>\n\n用法：与 /edit_title 同 `::` 模板，但额外扫所有 categories 的 detail.md 文件，把出现的 `「<old>」` token 替换为 `「<new>」`。让跨 doc task ref 自动跟随 rename — 避免 owner 在多份 detail.md 内手动维护。\n\n与 /edit_title 区别：\n- /edit_title：仅改 task 标题 + .md 文件 move（cross-doc ref 留 stale）\n- /cascade_rename：上述全套 + 扫所有 detail.md 替换 `「<old>」` token\n\n后端：先 `memory_rename(butler_tasks, old, new)` 做主操作；成功后扫 index 内所有 item 的 detail.md 文件，文本搜替 `「<old>」` → `「<new>」` 后 fs::write。失败的单文件 IO 不回滚主 rename（已 sealed），best-effort 语义。\n\n限制：\n- 仅扫 `「<title>」` 全角引号 token — 不触及 `[blockedBy: <title>]` 等 description markers（那些在 description 而非 detail.md，需 memory_edit re-write 路径，未来 iter 扩）\n- 不触及 description 本身的 task ref — owner 通常希望 description 保持原样作历史 snapshot\n\n空 title / new_title → usage hint。Title resolve 与 /done / /cancel / /show 同三层（数字 index → fuzzy → 错误候选）。\n\n输出格式：\n  🔁 已改标题：「<old>」→「<new>」\n  · 同步 N 份 detail.md 内的 ref token\n\nN === 0 时说「无 detail.md 需要更新」— owner 知道 cascade 扫了但没找到引用，可手动 grep 验证。\n\n示例：\n  /cascade_rename 写周报 :: 写 W21 周报\n  /cascade_rename 整理 Downloads :: 清理桌面（更详细名）\n  /cascade_rename 1 :: 重命名（/tasks 第 1 条 + cascade）\n\n相关：/edit_title（仅 rename 不 cascade — owner 想保 detail.md ref 不动时用）；/dup（克隆而非 rename）；/show（看 rename 后 raw + detail）。",
        "timeline" => "🕰️ /timeline <title>\n\n用法：扫 butler_history.log 取这条 task 的所有 create / update / delete 事件，按时序展开每个事件含哪些「状态变化」markers — audit 这条 task 经历了啥。Title resolve 与 /show / /done / /cancel 同三层（数字 index → fuzzy → 错误候选）。\n\n识别的 markers：[done] / [error: ...] / [snooze: ...] / [result: ...] / [cancelled: ...] / [pinned] / [silent] / [blockedBy: ...] / [archived: ...]。\n\n输出格式：\n  🕰️ 「<title>」时间线 · N 个事件\n  📝 MM-DD HH:MM · 创建\n  ✏️ MM-DD HH:MM · [pinned]\n  ✏️ MM-DD HH:MM · [snooze: 2026-05-17 18:00]\n  ✏️ MM-DD HH:MM · [done] [result: 已发送]\n\n示例：\n  /timeline 整理 Downloads\n  /timeline 1  （/tasks 输出第 1 条）\n\n注意：butler_history snippet 单行最多 BUTLER_HISTORY_DESC_CHARS（80 字符），靠后的 markers 可能被截断 → 不显。极长 description 末尾的 marker 在本视图里不可见，是 best-effort 视图。\n\n对比：/show 显当前 snapshot（含所有 markers），/timeline 显历史演化。两者互补 audit 维度。",
        "blocked" => "🔒 /blocked\n\n用法：列出本 chat 派单中被 [blockedBy: ...] 锁住的活跃 task（pending / error），每条下方缩进列出仍未解决的 blocker 标题。无参。\n\n示例：\n  /blocked\n\n相关：/snoozed（被 [snooze:] 暂停的）；/forks <title>（反向：哪些 task 在等这条解锁）。",
        "forks" => "🔱 /forks <title>\n\n用法：反向 audit — 列出本 chat 派单中所有 active task（pending / error）的 description 含 `[blockedBy: <title>]` marker 的，让 owner 知道「这条 task 解锁后会让谁动起来」。与 /blocked（列被卡的）对偶。空 title → usage hint；title resolve 与 /done / /cancel / /show / /timeline 同三层（数字 index → fuzzy → 错误候选）。\n\n示例：\n  /forks 整理 Downloads\n  /forks 1  （/tasks 输出第 1 条）\n\n输出格式：\n  🔱 解锁「<title>」会松开 N 条 task：\n  🟢 fork_a\n  ⚠️ fork_b\n\n无引用 → 「解锁这条不会影响其它 task」友好兜底。让 owner 在决定是否优先做某条 blocker 时，看到「这条做完会让谁动起来」做出更明智的优先级判断。\n\n相关：/blocked_by <title>（反向 — 我在等谁）。",
        "blocked_by" => "🔒 /blocked_by <title>\n\n用法：单条 audit — 列出 title 的 `[blockedBy: ...]` markers 中**仍未解决**的 blocker（即引用对象仍处于 active = pending / error 状态）。已 done / cancelled 的 blocker 视作已解决跳过。\n\n与 /forks 反向：那个列「谁等我」（解锁 title 后谁会动起来），本命令列「我等谁」（title 卡在等什么）。与 /blocked（全 chat audit）对比 — 那个跨任务列所有被卡的，本命令聚焦单条。\n\n空 title → usage hint；title resolve 与 /done / /cancel / /show 同三层（数字 index → fuzzy → 错误候选）。\n\n输出格式：\n  🔒 「<title>」被 N 条 blocker 卡住（共 M 条 marker / N 仍未解决）：\n  🟢 blocker_a\n  ⚠️ blocker_b\n\n所有 blocker 均已解决 → 「✨ 「<title>」的 M 条 blocker 均已解决 — 可以推进了」。无 blockedBy markers → 「不在等任何 blocker」。\n\n示例：\n  /blocked_by 写决策文档\n  /blocked_by 1   （/tasks 输出第 1 条）\n\n相关：/forks <title>（反向 — 谁等我）；/blocked（全 chat audit）；/show（看 raw description 含全部 markers）。",
        "snoozed" => "💤 /snoozed\n\n用法：列出当前在 [snooze: ...] 中的 task + 还多久醒（按醒时间升序）。无参。\n\n示例：\n  /snoozed\n\n相关：/snooze（暂停一条）；/unsnooze（解除）。",
        "reset" => "🔄 /reset\n\n用法：清掉 LLM 对话上下文（保留 system / 人设）。无 armed 二次确认（与桌面 `/clear` 不同 — 不同设备 / 多用户文化）。\n\n示例：\n  /reset",
        "version" => "🐾 /version\n\n用法：查看 pet app 版本 + SQLite schema 版本。无参。bug report 写「什么版本」用。\n\n示例：\n  /version",
        "help" => "❓ /help [cmd | all | search <kw>]\n\n用法：\n  · /help（无参）→ 显全表 + 一行描述\n  · /help <cmd> → 显该命令的详细用法 + 示例\n  · /help all → 长版说明书（每条命令详细用法 + 示例 + 相关命令，自动切多条 TG 消息）\n  · /help search <kw> → 在所有命令名 / 描述 / 详细文案里搜 keyword（case-insensitive）\n\n示例：\n  /help\n  /help cancel\n  /help /snooze   （`/` 前缀也接受）\n  /help all\n  /help search 复制",
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
        "/buckets  —  active task 按 priority 分桶（P7+ / P5-6 / P3-4 / P1-2 / P0 一行式）".to_string(),
        "/task <title>  —  入队（默认 P3；前缀 !! P5、!!! P7）".to_string(),
        "/done <title> | /cancel <title> | /retry <title>  —  标 done / 取消 / 重试（详细原因 / result 回桌面）".to_string(),
        "/snooze <title> [preset] | /unsnooze <title>  —  暂停 / 解除暂停（preset = 30m / 2h / tonight / tomorrow / monday）".to_string(),
        "/pin <title> | /unpin <title>  —  钉住 / 取消钉住（与桌面「📌 N」chip 过滤同源）".to_string(),
        "/silent <title> | /unsilent <title>  —  标静默 / 解除静默（LLM 不主动选；面板仍可手动触发）".to_string(),
        "/silenced  —  列出本聊天派单中所有 silent 任务（按状态分组）".to_string(),
        "/markers  —  一次列出所有 owner-intent markers（pinned + silent 两段，与 /pinned + /silenced 组合等价）".to_string(),
        "/tags  —  列本聊天派单中所有用过的 #tag + 各 tag 任务数（top 15，按数量降序）".to_string(),
        "/pinned  —  列出本聊天派单中所有钉住任务（按状态分组，含 done/error/cancelled）".to_string(),
        "/pinned_due  —  收紧 pinned + 含 due 的 active task（高优截止 audit；按 due 升序）".to_string(),
        "/mood  —  查看宠物当前心情".to_string(),
        "/whoami  —  宠物自我介绍（陪伴 / 心情 / 自我画像 / 近常用工具）".to_string(),
        "/today  —  今日到期 / 已完成的任务标题清单".to_string(),
        "/now  —  一句话快速状态：当前时间 + 时区 + 陪伴 + 心情 emoji（与 /whoami 多行画像互补）".to_string(),
        "/last_speech  —  pet 最近一条主动开口 + ts（与 ChatMini「⏱ 沉默 N 分」chip 对偶 audit）".to_string(),
        "/show_speech [N]  —  pet 最近 N 条主动开口（默认 5，上限 20）— 与 /last_speech 单条对偶".to_string(),
        "/last  —  显本聊天最近创建的一条 task（含 raw 描述预览）— 闪查刚 enqueue 的对不对".to_string(),
        "/random  —  随机抽 1 条 active 任务（pending / error）— 选择困难时让宠物决定下一步".to_string(),
        "/sleep  —  一键 mute proactive 8 小时 + 友好「晚安」reply（与 /mute 480 等价但语气温和）".to_string(),
        "/quick <text>  —  静默创 P3 task + 极短 reply（仅 ✓ + title）— 适合快速 dump 不被长回复打扰".to_string(),
        "/yesterday  —  列昨日 done 任务标题 + result 摘要（与 /today 互补 — audit 昨日产出）".to_string(),
        "/today_done  —  今日 done 任务标题 + result 摘要一行式（/today 的 done 切片 + result 摘要）".to_string(),
        "/streak  —  连续有 done 完成的天数 + 近 7 天 / 30 天 done 总数（audit 完成节奏）".to_string(),
        "/pri <title> <N>  —  单改 priority（0..=9）— 不走 /edit 全量覆写".to_string(),
        "/swap_priority <a> :: <b>  —  互换两 task 的 priority（sprint 重组场景，与 /pri 单改互补）".to_string(),
        "/feedback <text>  —  给 pet 留反馈（写 feedback_history，影响下次 proactive turn）".to_string(),
        "/transient <text> [minutes]  —  设 N 分钟有效的临时上下文（不存盘 in-memory；缺省 60m，上限 7 天）".to_string(),
        "/feedback_history [N]  —  列最近 N 条 feedback 记录（含 /feedback 写的 + 系统记录的隐性信号；缺省 5，上限 20）".to_string(),
        "/silent_all [minutes]  —  批量 silent butler_tasks N 分钟，自动撤回（缺省 60；0 = 立即解除）".to_string(),
        "/alarms [N]  —  列 todo 段 pending reminders（含目标时间 + 剩余分钟，按 target 升序；缺省 5，上限 20）".to_string(),
        "/recent_chats [N]  —  列最近 N 条 active session 内 user ↔ pet 聊天往返（缺省 5，上限 20）".to_string(),
        "/aware  —  pet 当前感知 snapshot（transient_note + active tasks + mood + 时间 + 陪伴）".to_string(),
        "/here  —  owner 视角信号 snapshot（transient_note + mute + 最近 feedback band，与 /aware 对偶）".to_string(),
        "/tag <name>  —  列含某 #tag 的所有 task（exact 等值，case-insensitive；与 /tags 列 tag 名互补）".to_string(),
        "/tags_for <title>  —  列单条 task 标的所有 #tag（与 /tags 全聊天视图对偶 — 单条聚焦）".to_string(),
        "/touch <title>  —  刷 task 的 updated_at 不改内容 — 让老 task 重新冒头 proactive 选单".to_string(),
        "/cancel_all_error confirm  —  批量 cancel 本聊天所有 error 任务（需带 confirm token 防误触）".to_string(),
        "/promote_all_p7 confirm  —  紧急 sprint：批量给本聊天 active task priority +1（clamp 7；需带 confirm）".to_string(),
        "/touch_all_p7 confirm  —  批量 touch 所有 P7+ active task 刷 updated_at（需带 confirm；与 /promote_all_p7 互补）".to_string(),
        "/pin_all_p7 confirm  —  批量给所有 P7+ active task 加 [pinned] marker（需带 confirm；与 /touch_all_p7 / /promote_all_p7 同 P7+ 批量族）".to_string(),
        "/consolidate_now confirm  —  TG 端手动触发一次 consolidate sweep（需带 confirm — LLM-heavy / 烧 token；与桌面「立即整理」对偶）".to_string(),
        "/promote <title>  —  priority +1（clamp 9）— 升一阶不必算具体 P 值".to_string(),
        "/demote <title>  —  priority -1（clamp 0）— 降一阶，与 /promote 对偶".to_string(),
        "/due [preset]  —  列指定时段 due（tomorrow / thisweek / nextweek 含中英 alias，缺省 tomorrow）".to_string(),
        "/recent [N]  —  最近 N 条已完成任务标题（默认 5，上限 20）".to_string(),
        "/oldest_n [N]  —  本 chat 最老 N 条 pending（created_at asc）— audit「堆积最久的活」".to_string(),
        "/active_recent [N]  —  本 chat 最近 N 条新建 active（pending / error，created_at desc）— 与 /recent done 反向".to_string(),
        "/find <keyword>  —  搜本聊天派单（命中标题或描述子串，至多 10 条）".to_string(),
        "/find_in_detail <keyword>  —  搜 detail.md 内容（含命中点 snippet，至多 8 条；与 /find 互补 — 「我笔记里写过 X」audit）".to_string(),
        "/show <title>  —  显单条任务完整 raw description（含 markers）+ detail.md 预览".to_string(),
        "/peek <title>  —  一行紧凑视图：status + schedule + 关键 markers（与 /show 完整视图互补 — 快瞄场景用）".to_string(),
        "/dup <title>  —  复制 task 为新 P3 pending 实例，title 加「(副本)」后缀 — 保 schedule / pinned / silent / tags，剥终态 markers".to_string(),
        "/snippets  —  列含 [snippet] / [snippet: <label>] marker 的 task — 可复用模板 / 流程 / 常用回复 audit".to_string(),
        "/recent_events <title> [N]  —  单 task 最近 N 个 butler_history 事件紧凑视图（默认 5，上限 20；与 /timeline 完整视图互补）".to_string(),
        "/touched_today  —  列今日 updated_at 命中 task（任意状态）— audit「我今天动过哪些」；与 /today_done done-only 互补".to_string(),
        "/edit_title <title> :: <new title>  —  仅改 task 标题（不动 description / detail.md / markers）— 前端 inline rename 的 TG 端对偶".to_string(),
        "/touched_yesterday  —  /touched_today 的昨日对偶 — 任意状态、昨日 updated_at 命中 task（复盘视角）".to_string(),
        "/touched_thisweek  —  本周（自周一 00:00 起）任意状态、updated_at 命中 task — 周维度复盘".to_string(),
        "/oldest_done [N]  —  最早完成的 N 条 done task（updated_at asc）— /recent 反向；audit「老 backlog 终于完成」".to_string(),
        "/cascade_rename <title> :: <new title>  —  改标题 + 自动同步所有 detail.md 内 「<old>」 ref token 替换（cross-doc ref 维护）".to_string(),
        "/mute_today  —  静音 proactive 到本地午夜 — 一键「今夜不打扰」预设；与 /mute N / /sleep_until 互补".to_string(),
        "/digest_yesterday [N]  —  昨日 done 任务 + [result:] 一行式（默认 5，上限 20）— /digest 的昨日对偶".to_string(),
        "/digest_thisweek [N]  —  本周 done 任务 + [result:] 一行式（默认 5，上限 20）— 周报场景".to_string(),
        "/search_today <kw>  —  限定今日 updated_at 的 task 内 fuzzy 搜 keyword — 「今天我做的与 X 相关的」精准 audit".to_string(),
        "/search_yesterday <kw>  —  /search_today 的昨日对偶 — 「昨天我做的与 X 相关的」精准 audit（复盘视角）".to_string(),
        "/search_thisweek <kw>  —  /search_today 的本周对偶 — 「本周与 X 相关的」精准 audit（周报场景）".to_string(),
        "/find_in_detail_today <kw>  —  /find_in_detail 的今日切片 — 限今日 task 的 detail.md 内容搜".to_string(),
        "/find_in_detail_yesterday <kw>  —  /find_in_detail_today 的昨日对偶 — 昨日 task 的 detail.md 内容搜".to_string(),
        "/idle_7d  —  pending 且 updated_at ≥ 7 天前的 task — stale backlog audit（PanelTasks 💤 chip TG 对偶）".to_string(),
        "/recent_pins [N]  —  近 N 条 pin 决策（每 title 取最早 [pinned] sighting desc，默认 5，上限 20）".to_string(),
        "/help_table  —  audit family 分组速查表 — /help（flat 全表）的分组兄弟，命令爆炸后 navigation aid".to_string(),
        "/audit_summary  —  聚合 5 大 audit 信号 — sprint kickoff 一键视图".to_string(),
        "/cat_top [N]  —  按 cat items 总量 desc 列前 N — 跨 cat 容量对比（默认 5，上限 20）".to_string(),
        "/alarms_today  —  今日待触发 alarm（/alarms 的 today 切片；无 N 参 — 今日范围天然小）".to_string(),
        "/alarms_thisweek  —  /alarms_today 的本周对偶 — 本周内触发 alarm 集中视图（无 N 参）".to_string(),
        "/peek_pinned  —  所有 pinned task 一行紧凑视图（status + schedule + markers）— /pinned 密集版".to_string(),
        "/random_pinned  —  从 pinned task 中随机抽 1 条 — /random 的 pinned 子集（选择困难时让 pet 决定）".to_string(),
        "/tags_today  —  今日动过 task 含的 #tag 计数（/tags 的 today 切片）— 「今天主题」audit".to_string(),
        "/tags_yesterday  —  /tags_today 的昨日对偶 — 昨日 task 的 #tag 计数（复盘视角）".to_string(),
        "/tags_thisweek  —  /tags_today 的本周对偶 — 本周 task 的 #tag 计数（周报场景）".to_string(),
        "/timeline <title>  —  时间线：列 butler_history 事件 + 当时状态变化 markers（[done]/[error:]/[snooze:]/[result:] 等）".to_string(),
        "/blocked  —  列出被 [blockedBy: …] 锁住的活跃 task + 仍未解决的 blocker".to_string(),
        "/forks <title>  —  反向 audit：哪些活跃 task 在 [blockedBy: <this>]（这条解锁会让谁动起来）".to_string(),
        "/blocked_by <title>  —  单条 audit：title 仍未解决的 blockers（与 /forks 反向 — 我在等谁）".to_string(),
        "/snoozed  —  列出当前在 [snooze: …] 中的 task + 还多久醒".to_string(),
        "/mute [N]  —  临时静音 proactive N 分钟（默认 30；0 = 解除）".to_string(),
        "/sleep_until <HH:MM>  —  静音到指定本地时刻（HH:MM；目标 ≤ now → 明日同时；与 /mute N 互补）".to_string(),
        "/snooze_until <title> <HH:MM>  —  把任务 snooze 到指定本地时刻（与 /snooze relative preset 互补；目标 ≤ now → 明日同时）".to_string(),
        "/note <text>  —  把任意文本作 general memory item 存（随手记一笔）".to_string(),
        "/reflect <text>  —  把任意文本作 ai_insights memory item 存（反思 / 自我洞察，与 /note 对偶但分类不同）".to_string(),
        "/digest [N]  —  最近 N 条 done task 标题 + result 一行式（默认 5，上限 20）".to_string(),
        "/edit <title> :: <new desc>  —  覆写 butler task 描述（全量替换，markers 需自己写进 new desc）".to_string(),
        "/edit_due <title> <preset>  —  友好 preset 改 due（tonight / 明天 / 周一 / next_friday / +30m / +1d / clear ...）".to_string(),
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

/// `/pinned_due` 命令回复文案。pure：filter views — active (Pending /
/// Error) + pinned + has due — 按 due 升序排（最近到期在前）。
///
/// 与 /pinned（仅 pinned，不 filter due）/ /due（仅 due window，不
/// filter pinned）双重收紧 — owner 紧急 audit「我钉了的 + 有截止
/// 时间的」高优清单。空 → 友好兜底教 owner 看更宽视角。
pub fn format_pinned_due_reply(views: &[crate::task_queue::TaskView]) -> String {
    use crate::task_queue::TaskStatus;
    let mut filtered: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| matches!(v.status, TaskStatus::Pending | TaskStatus::Error))
        .filter(|v| v.pinned)
        .filter(|v| v.due.is_some())
        .collect();
    if filtered.is_empty() {
        return "🔥 暂无同时 pinned + 含 due 的 active task。\n看 /pinned（仅 pinned）或 /due（按窗口看 due）拿更宽视角。".to_string();
    }
    // 按 due ISO 字典序升序 = 时间升序（task_queue 写的 "YYYY-MM-DDTHH:MM"
    // 标准化形式字典序与时间序一致）。
    filtered.sort_by(|a, b| {
        a.due.as_deref().unwrap_or("").cmp(b.due.as_deref().unwrap_or(""))
    });
    let mut out = format!(
        "🔥 pinned + due 任务（共 {} 条，按 due 升序）",
        filtered.len()
    );
    for v in &filtered {
        let emoji = match v.status {
            TaskStatus::Pending => "⏳",
            TaskStatus::Error => "⚠️",
            // unreachable per filter
            _ => "·",
        };
        out.push('\n');
        out.push_str(&format_task_line(emoji, v));
    }
    truncate_if_overflow(out, filtered.len())
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

/// `/buckets` 命令回复文案。pure：把 active task（pending / error）按
/// priority 分到 P0..P9 桶 + 一行式 dump。
///
/// 输出格式：
/// ```
/// 🎯 priority 分桶（N 条 active）
/// P7+: 3 · P5-6: 7 · P3-4: 12 · P1-2: 5 · P0: 2
/// ```
///
/// 分组：P7+ / P5-6 / P3-4 / P1-2 / P0 — 与桌面 PanelTasks 既有
/// priorityBands 同分组（5 段，让 chip 视觉成行不挤）。空 → 友好兜
/// 底「本 chat 无 active task」。
pub fn format_buckets_reply(views: &[crate::task_queue::TaskView]) -> String {
    use crate::task_queue::TaskStatus;
    let actives: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| matches!(v.status, TaskStatus::Pending | TaskStatus::Error))
        .collect();
    if actives.is_empty() {
        return "🎯 本 chat 无 active task — 桶分布无数据。\n用 /tasks 看全状态清单 / /yesterday 看昨日产出。".to_string();
    }
    let mut p7_plus = 0u32;
    let mut p5_6 = 0u32;
    let mut p3_4 = 0u32;
    let mut p1_2 = 0u32;
    let mut p0 = 0u32;
    for v in &actives {
        match v.priority {
            7..=u8::MAX => p7_plus += 1,
            5..=6 => p5_6 += 1,
            3..=4 => p3_4 += 1,
            1..=2 => p1_2 += 1,
            0 => p0 += 1,
        }
    }
    format!(
        "🎯 priority 分桶（{} 条 active）\nP7+: {} · P5-6: {} · P3-4: {} · P1-2: {} · P0: {}",
        actives.len(), p7_plus, p5_6, p3_4, p1_2, p0
    )
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

/// `/oldest_done <N>` 命令回复文案。pure：与 `format_recent_reply` 同
/// 结构但 sort asc（最早完成在前），其余完全一致 — 让 owner 切换视角时
/// 心智一致。空 → 教学指向 /done 标完成。
pub fn format_oldest_done_reply(
    views: &[crate::task_queue::TaskView],
    n: u32,
) -> String {
    use crate::task_queue::TaskStatus;
    let mut done: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| matches!(v.status, TaskStatus::Done))
        .collect();
    // updated_at ISO 字典序 = 时间序 — asc 拿"最早完成在前"（与 /recent
    // 的 desc 反向）
    done.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    if done.is_empty() {
        return "✨ 本聊天派单暂无完成记录。\n做完一条后 /done <标题> 标记，再来 /oldest_done 看清单。".to_string();
    }
    let take_n = (n as usize).max(1);
    let shown = &done[..done.len().min(take_n)];
    let mut out = String::new();
    out.push_str(&format!(
        "🪦 最早完成的 {} 条（共 {}）：",
        shown.len(),
        done.len()
    ));
    for v in shown {
        let when = if v.updated_at.len() >= 16 {
            format!("{} {}", &v.updated_at[..10], &v.updated_at[11..16])
        } else {
            v.updated_at.clone()
        };
        out.push_str(&format!("\n· {} · {}", when, v.title));
    }
    if done.len() > shown.len() {
        out.push_str(&format!(
            "\n…还有 {} 条更晚完成（用 /oldest_done {} 看更多，上限 20；或 /recent 看最近完成）",
            done.len() - shown.len(),
            (done.len()).min(20)
        ));
    }
    out
}

/// `/oldest_n <N>` 命令回复文案。pure：filter pending（active 但不含
/// error — owner 关心「堆积最久」语义偏「活的等待」非「失败重试」），
/// sort by created_at asc（最早创建在前），take N。
///
/// 与 /recent 反向 — recent 看「最新 done」（产出感），oldest_n 看
/// 「最老 pending」（积压感）。给 owner 觉察「我哪些活儿挂得最久 →
/// 是否该 /pri 拉到高优 / /cancel 砍掉 / 重组」。
///
/// 时间戳显示格式 `MM-DD HH:MM`（与 /recent 一致）+ 「N 天前」相对
/// age（与桌面 PanelTasks itemMeta 同），让 owner 一眼看「多老」。
/// caller 传 now 让单测稳定（与 /streak / /yesterday 同 inject 模板）。
pub fn format_oldest_n_reply(
    views: &[crate::task_queue::TaskView],
    n: u32,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> String {
    use crate::task_queue::TaskStatus;
    let mut pending: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| matches!(v.status, TaskStatus::Pending))
        .collect();
    // created_at ISO 字典序 = 时间序，升序拿"最早创建在前"
    pending.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    if pending.is_empty() {
        return "✨ 本聊天派单暂无 pending 任务 — 没有「堆积最久」的活了。\n用 /tasks 看全状态清单 / /recent 看最近完成。".to_string();
    }
    let take_n = (n as usize).max(1);
    let shown = &pending[..pending.len().min(take_n)];
    let mut out = format!(
        "⌛ 最老 {} 条 pending（共 {}，按 created_at 升序）：",
        shown.len(),
        pending.len()
    );
    for v in shown {
        // created_at ISO 形如 "2026-05-04T13:00:00+08:00"
        let when = if v.created_at.len() >= 16 {
            format!("{} {}", &v.created_at[5..10], &v.created_at[11..16])
        } else {
            v.created_at.clone()
        };
        // 相对 age：parse + diff days
        let age_label = chrono::DateTime::parse_from_rfc3339(&v.created_at)
            .ok()
            .map(|created| (now - created).num_days())
            .map(|days| {
                if days >= 1 {
                    format!(" · {} 天前", days)
                } else {
                    String::new()
                }
            })
            .unwrap_or_default();
        out.push_str(&format!("\n· {} · {}{}", when, v.title, age_label));
    }
    if pending.len() > shown.len() {
        out.push_str(&format!(
            "\n…还有 {} 条更老 pending（用 /oldest_n {} 看更多，上限 20）",
            pending.len() - shown.len(),
            (pending.len()).min(20)
        ));
    }
    out
}

/// `/active_recent <N>` 命令回复文案。pure：filter active（pending +
/// error）— 与 /tasks active 段同 status 集；sort by created_at desc
/// （最新创建在前），take N。
///
/// 与 /recent 反向 — recent 看「最新 done」（产出感），active_recent
/// 看「最新创建的活」（输入感）。让 owner 在 TG 上扫读「我最近塞了哪
/// 些活到队列」 — 比 /last（单条）多看几条；比 /tasks（全表 +
/// compare_for_queue 智能排序）更聚焦活动段 + 创建时序。
///
/// 与 /oldest_n 对偶：那个 created_at asc 看「堆积最久」，本命令
/// created_at desc 看「最新塞入」。/oldest_n 仅 pending（语义偏「挂着
/// 没动」），本命令含 error（语义偏「创建时序」 — error 仍是「正在
/// 跑的轨道」上的条目）。
///
/// 时间戳显示格式 `MM-DD HH:MM`（与 /recent / /oldest_n 一致）+
/// status emoji（🟢 pending / ⚠️ error）+ 「N 天前」相对 age。caller
/// 传 now 让单测稳定（与 /oldest_n / /streak / /yesterday 同 inject
/// 模板）。
pub fn format_active_recent_reply(
    views: &[crate::task_queue::TaskView],
    n: u32,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> String {
    use crate::task_queue::TaskStatus;
    let mut active: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| matches!(v.status, TaskStatus::Pending | TaskStatus::Error))
        .collect();
    // created_at ISO 字典序 = 时间序，降序拿"最新创建在前"
    active.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    if active.is_empty() {
        return "✨ 本聊天派单暂无 active 任务 — 没有 pending / error 的活儿。\n用 /task 新建 / /recent 看最近完成。".to_string();
    }
    let take_n = (n as usize).max(1);
    let shown = &active[..active.len().min(take_n)];
    let mut out = format!(
        "🆕 最近 {} 条新建 active（共 {}，按 created_at 降序）：",
        shown.len(),
        active.len()
    );
    for v in shown {
        let when = if v.created_at.len() >= 16 {
            format!("{} {}", &v.created_at[5..10], &v.created_at[11..16])
        } else {
            v.created_at.clone()
        };
        let emoji = match v.status {
            TaskStatus::Pending => "🟢",
            TaskStatus::Error => "⚠️",
            _ => "·",
        };
        let age_label = chrono::DateTime::parse_from_rfc3339(&v.created_at)
            .ok()
            .map(|created| (now - created).num_days())
            .map(|days| {
                if days >= 1 {
                    format!(" · {} 天前", days)
                } else {
                    String::new()
                }
            })
            .unwrap_or_default();
        out.push_str(&format!("\n· {} · {} {}{}", when, emoji, v.title, age_label));
    }
    if active.len() > shown.len() {
        out.push_str(&format!(
            "\n…还有 {} 条更早创建 active（用 /active_recent {} 看更多，上限 20）",
            active.len() - shown.len(),
            (active.len()).min(20)
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

/// `/find_in_detail <keyword>` 命令回复文案。pure：handler 负责 IO
/// 读取每条 task 的 detail.md 并 case-insensitive 子串过滤，本函数
/// 仅做字符串拼装。`hits` 已 sort（pending / error 浮顶），每条含
/// title + status + 命中点附近 60 字 snippet。
///
/// 与 format_find_reply 同模板但 hits cap 8（每行含 snippet 更长）。
/// 空 keyword → missing-arg hint；无命中 → 兜底文案附 keyword。
pub struct FindInDetailHit<'a> {
    pub title: &'a str,
    pub status: crate::task_queue::TaskStatus,
    pub snippet: String,
}

pub fn format_find_in_detail_reply(
    hits: &[FindInDetailHit],
    keyword: &str,
) -> String {
    use crate::task_queue::TaskStatus;
    let kw = keyword.trim();
    if kw.is_empty() {
        return "🔬 用法：/find_in_detail <keyword>\n按 keyword 搜本聊天派单的 detail.md 内容（不分大小写，含命中点 snippet）。\n例：/find_in_detail rebase / /find_in_detail TODO / /find_in_detail 决策\n\n与 /find（仅扫标题 + 描述）互补 — 「我笔记里写过 X」audit。".to_string();
    }
    if hits.is_empty() {
        return format!(
            "🔬 没有 task 的 detail.md 含「{}」。\n试试更短的关键词；或 /find 搜标题 / 描述；或 /tasks 看清单。",
            kw
        );
    }
    let cap = 8;
    let shown = &hits[..hits.len().min(cap)];
    let mut out = format!(
        "🔬 命中「{}」{} 条（detail.md 内容搜索）：",
        kw,
        hits.len()
    );
    for h in shown {
        let emoji = match h.status {
            TaskStatus::Pending => "🟢",
            TaskStatus::Error => "⚠️",
            TaskStatus::Done => "✅",
            TaskStatus::Cancelled => "🚫",
        };
        out.push_str(&format!("\n{} {}\n   …{}…", emoji, h.title, h.snippet));
    }
    if hits.len() > cap {
        out.push_str(&format!(
            "\n…还有 {} 条命中（关键词太宽？试更精确的词）",
            hits.len() - cap
        ));
    }
    out
}

/// `/find_in_detail_today` 命令回复文案。pure。与 `format_find_in_detail
/// _reply` 同结构（emoji + snippet 60 字 context + 8 cap），但 header
/// 含日期 scope + 空集兜底教学指 /find_in_detail 全量 / /touched_today
/// 全谱（避免 self-loop）。
pub fn format_find_in_detail_today_reply(
    hits: &[FindInDetailHit],
    keyword: &str,
    today: chrono::NaiveDate,
) -> String {
    use crate::task_queue::TaskStatus;
    let kw = keyword.trim();
    if kw.is_empty() {
        return "🔬 用法：/find_in_detail_today <keyword>\n限定今日 updated_at 的 task 的 detail.md 内容搜 keyword（不分大小写，含命中点 snippet）。\n例：/find_in_detail_today rebase / /find_in_detail_today API\n\n相关：/find_in_detail（不限日期）；/search_today（扫标题 + description）；/touched_today（今日全谱）。".to_string();
    }
    let today_str = today.format("%Y-%m-%d").to_string();
    if hits.is_empty() {
        return format!(
            "🔬 今日（{}）无 task 的 detail.md 含「{}」。\n试 /find_in_detail 看不限日期 / /touched_today 看今日全谱。",
            today_str, kw,
        );
    }
    let cap = 8;
    let shown = &hits[..hits.len().min(cap)];
    let mut out = format!(
        "🔬 今日（{}）命中「{}」{} 条（detail.md 内容搜索）：",
        today_str,
        kw,
        hits.len(),
    );
    for h in shown {
        let emoji = match h.status {
            TaskStatus::Pending => "🟢",
            TaskStatus::Error => "⚠️",
            TaskStatus::Done => "✅",
            TaskStatus::Cancelled => "🚫",
        };
        out.push_str(&format!("\n{} {}\n   …{}…", emoji, h.title, h.snippet));
    }
    if hits.len() > cap {
        out.push_str(&format!(
            "\n…还有 {} 条命中（关键词太宽？试更精确的词）",
            hits.len() - cap,
        ));
    }
    out
}

/// `/find_in_detail_yesterday` 命令回复文案。pure。clone of
/// `format_find_in_detail_today_reply` 结构（hits/cap/emoji/snippet
/// 一致），仅 scope 是 yesterday + 空集兜底 alt 入口指 /find_in_detail
/// 全量 / /touched_yesterday 全谱（avoid loop）。
pub fn format_find_in_detail_yesterday_reply(
    hits: &[FindInDetailHit],
    keyword: &str,
    yesterday: chrono::NaiveDate,
) -> String {
    use crate::task_queue::TaskStatus;
    let kw = keyword.trim();
    if kw.is_empty() {
        return "🔬 用法：/find_in_detail_yesterday <keyword>\n限定昨日 updated_at 的 task 的 detail.md 内容搜 keyword（不分大小写）。\n例：/find_in_detail_yesterday rebase / /find_in_detail_yesterday API\n\n相关：/find_in_detail_today（今日同模板）；/find_in_detail（不限日期）；/touched_yesterday（昨日全谱）。".to_string();
    }
    let yesterday_str = yesterday.format("%Y-%m-%d").to_string();
    if hits.is_empty() {
        return format!(
            "🔬 昨日（{}）无 task 的 detail.md 含「{}」。\n试 /find_in_detail 看不限日期 / /touched_yesterday 看昨日全谱。",
            yesterday_str, kw,
        );
    }
    let cap = 8;
    let shown = &hits[..hits.len().min(cap)];
    let mut out = format!(
        "🔬 昨日（{}）命中「{}」{} 条（detail.md 内容搜索）：",
        yesterday_str,
        kw,
        hits.len(),
    );
    for h in shown {
        let emoji = match h.status {
            TaskStatus::Pending => "🟢",
            TaskStatus::Error => "⚠️",
            TaskStatus::Done => "✅",
            TaskStatus::Cancelled => "🚫",
        };
        out.push_str(&format!("\n{} {}\n   …{}…", emoji, h.title, h.snippet));
    }
    if hits.len() > cap {
        out.push_str(&format!(
            "\n…还有 {} 条命中（关键词太宽？试更精确的词）",
            hits.len() - cap,
        ));
    }
    out
}

/// `/find_in_detail` helper：从 detail.md 全文里抠 keyword 命中点附近
/// 60 字符的 context snippet（case-insensitive 命中索引；按 UTF-8 char
/// 粒度截以防切多字节中文 / emoji）。
///
/// 返 `Some(snippet)` 当 content 命中 kw（case-insensitive）；`None`
/// 时调用方据此知"该 task detail.md 未命中"跳过。snippet 内换行 / 多
/// 空格 flatten 成单空格（让 reply 单行可读）。
pub fn extract_find_in_detail_snippet(
    content: &str,
    kw: &str,
) -> Option<String> {
    if kw.is_empty() {
        return None;
    }
    let content_lower = content.to_lowercase();
    let kw_lower = kw.to_lowercase();
    let byte_idx = content_lower.find(&kw_lower)?;
    // byte_idx 在 content_lower 与 content 上 valid 等价（to_lowercase 对
    // ASCII 子集语义稳定；多字节中文 lowercase = 自己）。把 byte index 转
    // 为 char index 计 context window。
    let chars: Vec<char> = content.chars().collect();
    // 找 byte_idx 对应的 char index — 走 char_indices。
    let mut hit_char_idx = 0usize;
    for (cidx, (bidx, _)) in content.char_indices().enumerate() {
        if bidx >= byte_idx {
            hit_char_idx = cidx;
            break;
        }
    }
    // 60 字 context 窗：命中点左 30 + 命中点右 30（含 keyword 自身）。
    let context = 30usize;
    let start = hit_char_idx.saturating_sub(context);
    let end = (hit_char_idx + context).min(chars.len());
    let raw: String = chars[start..end].iter().collect();
    // flatten whitespace（newline / tab / 多空格 → 单空格）让 reply 行可读
    let flat = raw
        .replace('\n', " ")
        .replace('\t', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Some(flat)
}

/// `/help_table` 命令回复文案。pure：硬编码 audit family 分组 + 命令
/// 清单。命令爆炸后 owner 想找「pin 相关」/「cat 相关」/「rename
/// 相关」家族 entry — flat /help 一行描述方便细查、本表方便定向到家族
/// 后再 /help <cmd> 看细节。
///
/// 各 group emoji 与既有 chip family 配色协调（pin = 📌 / cat = 🌱 /
/// rename = 🔁 / idle = 💤 / streak = 🔥 / speech = 🗣 / 等）。
///
/// 顺序：高频常用在前（pin / cat / rename / idle / streak），audit 系
/// 列中段（find / speech），系统 / 增删改 / 危险在后。
///
/// 兼容 wrapper — 旧调用走全表路径。
#[allow(dead_code)]
pub fn format_help_table_reply() -> String {
    format_help_table_reply_full(None)
}

/// `/help_table [family]` 实现：family=None 显全表；family=Some 显
/// 该 family 的详细命令清单 + 一行描述。family key case-insensitive
/// + 含「pin / cat / rename / idle / stale / streak / find / search /
/// tag / speech / 对话 / alarm / mute / status / overview / 增删改 /
/// task / batch / 危险 / system / 系统」alias。
///
/// 未知 family → 列出 available family 名 + 全表 entry 兜底教学。
pub fn format_help_table_reply_full(family: Option<&str>) -> String {
    let family = family.map(|s| s.trim().to_ascii_lowercase());
    if let Some(key) = family.as_deref() {
        if !key.is_empty() {
            return format_help_table_family(key);
        }
    }
    [
        "📚 命令分组速查表（按 audit family）",
        "",
        "📌 pin 关注度",
        "  /pin /unpin /pinned /pinned_due /peek_pinned /random_pinned",
        "  /pin_all_p7 /recent_pins",
        "",
        "📚 cat（memory category）",
        "  /cat_top",
        "",
        "🔁 rename 重命名 audit",
        "  /edit_title /cascade_rename",
        "",
        "💤 idle / stale backlog",
        "  /idle_7d /touched_today /touched_yesterday /touched_thisweek",
        "  /oldest_n /oldest_done /active_recent",
        "",
        "🔥 streak 连续节奏",
        "  /streak",
        "",
        "🔎 find / search keyword",
        "  /find /find_in_detail",
        "  /find_in_detail_today /find_in_detail_yesterday",
        "  /search_today /search_yesterday /search_thisweek",
        "",
        "🏷 tag",
        "  /tag /tags /tags_for /tags_today /tags_yesterday /tags_thisweek",
        "",
        "🗣 pet speech / 对话",
        "  /last_speech /show_speech /recent_chats",
        "  /reflect /note /transient /feedback /feedback_history",
        "",
        "⏰ alarm / 通知 / mute",
        "  /alarms /alarms_today /alarms_thisweek",
        "  /mute /mute_today /sleep /sleep_until",
        "  /snooze /snooze_until /unsnooze /snoozed",
        "",
        "📋 task 增删改",
        "  /task /done /cancel /retry /quick /dup",
        "  /edit /edit_due /edit_title /pri /promote /demote /swap_priority",
        "  /pin /unpin /silent /unsilent /touch",
        "",
        "📊 status / overview",
        "  /tasks /stats /buckets /show /peek /timeline /recent_events",
        "  /aware /here /now /today /today_done /yesterday",
        "  /digest /digest_yesterday /digest_thisweek",
        "  /recent /due /last /random /streak /mood /whoami",
        "  /blocked /forks /blocked_by /snippets",
        "",
        "⚠️ batch / 危险（需带 confirm token）",
        "  /cancel_all_error /promote_all_p7 /touch_all_p7",
        "  /pin_all_p7 /consolidate_now /silent_all",
        "",
        "⚙️ system",
        "  /version /help /help_table /audit_summary /reset",
        "",
        "相关：/help（flat 全表 + 一行描述）；/help <cmd>（单命令详细用法）；/help search <kw>（全文 keyword 搜）。",
    ]
    .join("\n")
}

/// `/cat_top [N]` 命令回复文案。pure：caller 已 scan memory index +
/// per-cat item count + sort by count desc + cap N。row：(key, count)。
/// `total_cats` 是 memory index 内总 cat 数（让 header 透明 N 是 cap
/// 还是真总数）。
pub fn format_cat_top_reply(
    rows: &[(String, usize)],
    total_cats: usize,
) -> String {
    if rows.is_empty() {
        return "📊 memory index 内无 cat（或所有 cat 为空）。".to_string();
    }
    let mut out = format!(
        "📊 cat top {}（按 items 总量 desc，共 {} cat in index）：",
        rows.len(),
        total_cats,
    );
    for (key, count) in rows {
        out.push_str(&format!("\n· {} · {} 条", key, count));
    }
    out
}

/// `/audit_summary` 命令回复文案。pure：caller 已聚合 5 大 audit 信
/// 号 + today date。formatter 拼输出 + 每行 deep dive 入口。
/// 输入: (today, pin_streak, current_pinned,
///        idle_7d_count, touched_today_count, recent_renames_7d_count)
#[allow(clippy::too_many_arguments)]
pub fn format_audit_summary_reply(
    today: chrono::NaiveDate,
    pin_streak: usize,
    current_pinned: usize,
    idle_7d_count: usize,
    touched_today_count: usize,
    recent_renames_7d_count: usize,
) -> String {
    let date_str = today.format("%Y-%m-%d").to_string();
    let mut out = format!("📋 audit summary（{}）\n", date_str);
    out.push_str(&format!(
        "· 📌 pin streak: {} 天连续（当前 {} 钉）\n",
        pin_streak, current_pinned,
    ));
    out.push_str(&format!(
        "· 💤 idle 7d+: {} 条 stale pending → /idle_7d\n",
        idle_7d_count,
    ));
    out.push_str(&format!(
        "· ✅ 今日 touched: {} 条 → /touched_today\n",
        touched_today_count,
    ));
    out.push_str(&format!(
        "· 🏷 近 7d rename: {} 次",
        recent_renames_7d_count,
    ));
    out
}

/// pure：`/help_table <family>` 实现 — 取该 family 详细命令清单 +
/// 一行描述。`family_key` 已 lowercase。
///
/// 命令清单复制自 format_help_table_reply_full 的对应 group，并附上
/// /help 一行描述（从 ALL_HELP_TOPICS_EN_CHIP 抽取，或硬编码）。这
/// 让 owner /help_table pin 一次性看 family 内所有命令的简短用途，
/// 比逐 /help <cmd> 翻一次性高效。
///
/// alias key 接受：
/// - pin / 关注度 / 钉
/// - cat / 类目 / 活跃度
/// - rename / 重命名 / alias
/// - idle / stale / 闲置
/// - streak / 连续
/// - find / search / 搜
/// - tag / 标签
/// - speech / 对话 / 说话
/// - alarm / mute / 通知 / 静音
/// - status / overview / 概览
/// - task / 增删改 / edit
/// - batch / 危险 / 批量
/// - system / 系统
pub fn format_help_table_family(family_key: &str) -> String {
    let key = family_key.trim().to_ascii_lowercase();
    // family canonical name + emoji + (cmd, desc) tuples
    let family: Option<(&str, &str, Vec<(&str, &str)>)> = match key.as_str() {
        "pin" | "钉" | "关注度" => Some((
            "📌 pin 关注度",
            "钉住关键 task；与 priority 正交标 owner intent",
            vec![
                ("/pin <title>", "钉住任务（写 [pinned] marker）"),
                ("/unpin <title>", "取消钉住"),
                ("/pinned", "列本聊天派单所有钉住 task"),
                ("/pinned_due", "列 pinned + 含 due 的 active task"),
                ("/peek_pinned", "所有 pinned task 一行紧凑视图"),
                ("/random_pinned", "从 pinned 抽 1 条 — 选择困难入口"),
                ("/pin_all_p7", "批量给所有 P7+ active task 加 [pinned]（需 confirm）"),
                ("/recent_pins [N]", "近 N 条 pin 决策（dedupe by title earliest sighting）"),
            ],
        )),
        "cat" | "类目" => Some((
            "📚 cat（memory category）",
            "cat 维度 audit",
            vec![
                ("/cat_top [N]", "按 cat items 总量 desc 列前 N（capacity axis）"),
            ],
        )),
        "rename" | "重命名" => Some((
            "🔁 rename 重命名",
            "title 修改入口",
            vec![
                ("/edit_title <title> :: <new>", "仅改 task 标题（不动 desc / detail.md）"),
                ("/cascade_rename <title> :: <new>", "rename + 扫所有 detail.md 替换「<old>」ref"),
            ],
        )),
        "idle" | "stale" | "闲置" => Some((
            "💤 idle / stale backlog",
            "pending 但 updated_at 旧的 task — 「我搁着没动了」audit",
            vec![
                ("/idle_7d", "pending + updated_at ≥ 7 天前的 task list"),
                ("/touched_today", "今日 updated_at 命中 task（任意状态）"),
                ("/touched_yesterday", "昨日对偶"),
                ("/touched_thisweek", "本周对偶（自周一起）"),
                ("/oldest_n [N]", "最老 N 条 pending（created_at asc）"),
                ("/oldest_done [N]", "最早完成的 N 条 done（updated_at asc）"),
                ("/active_recent [N]", "最近 N 条新建 active task（pending / error）"),
            ],
        )),
        "streak" | "连续" => Some((
            "🔥 streak 连续节奏",
            "audit 完成度连续天数",
            vec![
                ("/streak", "连续 done 天数 + 近 7/30 天 done 总数"),
            ],
        )),
        "find" | "search" | "搜" => Some((
            "🔎 find / search keyword",
            "按 keyword 搜 — title / description / detail.md × date 矩阵",
            vec![
                ("/find <kw>", "按 keyword 搜 title / desc（至多 10 条）"),
                ("/find_in_detail <kw>", "搜 detail.md 内容（含 snippet）"),
                ("/find_in_detail_today <kw>", "今日切片"),
                ("/find_in_detail_yesterday <kw>", "昨日对偶"),
                ("/search_today <kw>", "限今日 updated_at 的 task fuzzy 搜"),
                ("/search_yesterday <kw>", "昨日对偶"),
                ("/search_thisweek <kw>", "本周对偶"),
            ],
        )),
        "tag" | "标签" => Some((
            "🏷 tag",
            "#tag exact 等值 audit（与 fuzzy /find 互补）",
            vec![
                ("/tag <name>", "列含某 #tag 的所有 task"),
                ("/tags", "列本聊天用过的所有 #tag + 计数"),
                ("/tags_for <title>", "单条 task 的 #tag 清单"),
                ("/tags_today", "今日 task 含的 #tag 计数"),
                ("/tags_yesterday", "昨日对偶"),
                ("/tags_thisweek", "本周对偶"),
            ],
        )),
        "speech" | "对话" | "说话" => Some((
            "🗣 pet speech / 对话",
            "pet 主动 utterance + chat 历史 + reflect / note 入口",
            vec![
                ("/last_speech", "pet 最近一条主动开口"),
                ("/show_speech [N]", "最近 N 条 pet 主动开口"),
                ("/recent_chats [N]", "最近 N 条 user ↔ pet 聊天往返"),
                ("/reflect <text>", "存为 ai_insights memory（自我反思）"),
                ("/note <text>", "存为 general memory（脑暴）"),
                ("/transient <text>", "N 分钟临时上下文给 pet"),
                ("/feedback <text>", "给 pet 留反馈（写 feedback_history）"),
                ("/feedback_history [N]", "列最近 N 条 feedback 记录"),
            ],
        )),
        "alarm" | "mute" | "通知" | "静音" => Some((
            "⏰ alarm / 通知 / mute",
            "reminder / snooze / mute proactive 全谱",
            vec![
                ("/alarms [N]", "列最近 N 条 pending reminders"),
                ("/alarms_today", "今日 alarm 切片"),
                ("/alarms_thisweek", "本周对偶"),
                ("/mute [N]", "mute proactive N 分钟（缺省 30）"),
                ("/mute_today", "静音到本地午夜"),
                ("/sleep", "一键 mute 8h + 「晚安」"),
                ("/sleep_until <HH:MM>", "mute 到指定时刻"),
                ("/snooze <title> [preset]", "暂停 task"),
                ("/snooze_until <title> <HH:MM>", "snooze 到绝对时刻"),
                ("/unsnooze <title>", "解除 snooze"),
                ("/snoozed", "列当前 snooze 中的 task"),
            ],
        )),
        "status" | "overview" | "概览" => Some((
            "📊 status / overview",
            "queue 总览 + 状态 snapshot",
            vec![
                ("/tasks", "列本会话派出的任务清单"),
                ("/stats", "待办 / 逾期 / 今日完成 状态计数"),
                ("/buckets", "active task 按 priority 分桶"),
                ("/show <title>", "显单条完整 raw description + detail 预览"),
                ("/peek <title>", "一行紧凑视图"),
                ("/timeline <title>", "时间线视图（butler_history 全 audit）"),
                ("/recent_events <title> [N]", "单 task 最近 N 个 history 事件"),
                ("/aware", "pet 当前感知 snapshot"),
                ("/here", "owner 视角 snapshot（mute / feedback 等）"),
                ("/now", "一句话快速状态"),
                ("/today", "今日叙事视图"),
                ("/today_done", "今日 done + result"),
                ("/yesterday", "昨日 done + result"),
                ("/digest [N]", "近 N 条 done + result 一行式"),
                ("/digest_yesterday [N]", "昨日对偶"),
                ("/digest_thisweek [N]", "本周对偶"),
                ("/recent [N]", "最近 N 条 done"),
                ("/due [preset]", "指定时段 due 的 task"),
                ("/last", "最近新建的 task"),
                ("/random", "随机抽 1 条 active task"),
                ("/streak", "连续 done 天数"),
                ("/mood", "pet 当前心情"),
                ("/whoami", "pet 自我介绍"),
                ("/blocked", "列被 [blockedBy:] 卡的 active task"),
                ("/forks <title>", "反向 — 列等这条解锁的 task"),
                ("/blocked_by <title>", "单条 task 等谁解锁"),
                ("/snippets", "列含 [snippet:] marker 的可复用模板 task"),
            ],
        )),
        "task" | "增删改" | "edit" => Some((
            "📋 task 增删改",
            "task lifecycle CRUD + marker 微改",
            vec![
                ("/task <title>", "派单（!! P5 / !!! P7 修饰）"),
                ("/done <title>", "标 done"),
                ("/cancel <title>", "取消"),
                ("/retry <title>", "把失败 task 重置回 pending"),
                ("/quick <text>", "静默创 P3 task + 极短 reply"),
                ("/dup <title>", "复制 task 为新 pending（保 schedule / 剥终态 markers）"),
                ("/edit <title> :: <new desc>", "覆写 description"),
                ("/edit_due <title> <preset>", "用 friendly preset 改 due"),
                ("/edit_title <title> :: <new>", "仅改标题"),
                ("/pri <title> <0-9>", "改单条 priority"),
                ("/promote <title>", "priority +1（clamp 9）"),
                ("/demote <title>", "priority -1（clamp 0）"),
                ("/swap_priority <a> :: <b>", "两 task 优先级互换"),
                ("/pin / /unpin <title>", "钉 / 取消钉"),
                ("/silent / /unsilent <title>", "标静默 / 解除"),
                ("/touch <title>", "刷 updated_at 不改内容"),
            ],
        )),
        "batch" | "危险" | "批量" => Some((
            "⚠️ batch / 危险（需带 confirm token）",
            "大范围 sweep — 操作前需带 `confirm` 二次确认",
            vec![
                ("/cancel_all_error confirm", "批量 cancel 所有 error task"),
                ("/promote_all_p7 confirm", "所有 active +1 priority（clamp 7）"),
                ("/touch_all_p7 confirm", "批量 touch 所有 P7+ active"),
                ("/pin_all_p7 confirm", "批量给所有 P7+ 加 [pinned]"),
                ("/consolidate_now confirm", "手动触发 consolidate sweep（LLM-heavy）"),
                ("/silent_all [N]", "批量给所有 butler_task 加 [silent] N 分钟（缺省 60）"),
            ],
        )),
        "system" | "系统" => Some((
            "⚙️ system",
            "基础元命令",
            vec![
                ("/version", "pet app 版本 + SQLite schema 版本"),
                ("/help [cmd | all | search <kw>]", "命令帮助（详见 /help all）"),
                ("/help_table [family]", "audit family 分组速查表"),
                ("/audit_summary", "聚合 audit 信号 sprint kickoff 视图"),
                ("/reset", "清掉 LLM chat context（保 persona）"),
            ],
        )),
        _ => None,
    };
    let Some((header, hint, cmds)) = family else {
        return format!(
            "❌ 未知 family「{}」。\n\n可用 family 名：pin / cat / rename / idle / streak / find / tag / speech / alarm / status / task / batch / system\n\n试 /help_table （无参）看全表概览。",
            family_key.trim(),
        );
    };
    let mut out = String::new();
    out.push_str(&format!("📚 {} 家族详细清单\n", header));
    out.push_str(&format!("{}\n\n", hint));
    for (cmd, desc) in cmds {
        out.push_str(&format!("· {}\n   {}\n", cmd, desc));
    }
    out.push_str("\n相关：/help <cmd>（单命令详细用法）；/help_table（无参全表概览）。");
    out
}

/// `/recent_pins [N]` 命令回复文案。pure：caller 已 scan butler_history
/// → 取含 [pinned] snippet 行 → dedupe by title 保留最早 sighting →
/// 按 ts desc 排好 + cap N。row：(ts_label, title)。
/// `total_in_retention` 是 retention 内 deduped pin 决策总数（让
/// header 透明 cap 因 N 还是 retention）。
pub fn format_recent_pins_reply(
    rows: &[(String, String)],
    total_in_retention: usize,
) -> String {
    if rows.is_empty() {
        return "📌 butler_history 内无 [pinned] sighting。\n试 /pin <title> 钉一条 sprint task；/pinned 看当前 pinned 清单。".to_string();
    }
    let mut out = format!(
        "📌 近 {} 条 pin 决策（共 {} 条 retention 内）：",
        rows.len(),
        total_in_retention,
    );
    for (ts_label, title) in rows {
        out.push_str(&format!("\n· {} · 「{}」", ts_label, title));
    }
    out
}

/// pure：从 butler_history 行集 + 今日 fallback 算 pin streak。
/// - dates_with_sighting: 集合，含 YYYY-MM-DD 字符串（每行 ts 前 10 字
///   且 snippet 包含 [pinned] 的 date）
/// - has_current_pinned: 当前是否有 active pinned task（若 true 且今日
///   不在 dates_with_sighting，今日 fallback 算 +1）
/// - today: 今日 YYYY-MM-DD（caller 传入便测）
///
/// 算法：从 today 往前 walk，每天 check date ∈ dates_with_sighting
/// （含 today fallback）；连续命中天数 = streak。第一天 miss 即 break。
///
/// 返回 (streak, earliest_sighting_date_or_none, max_streak_in_window)。
/// max_streak: 在 dates_with_sighting 集合内找最长连续段（含 today
/// fallback）— audit 历史峰值。
pub fn compute_pin_streak(
    dates_with_sighting: &std::collections::HashSet<String>,
    has_current_pinned: bool,
    today: chrono::NaiveDate,
) -> (usize, Option<String>, usize) {
    let date_str = |d: chrono::NaiveDate| d.format("%Y-%m-%d").to_string();
    // current streak: walk 后退
    let mut streak = 0usize;
    let mut earliest: Option<String> = None;
    let mut cursor = today;
    loop {
        let s = date_str(cursor);
        let hit = dates_with_sighting.contains(&s)
            || (cursor == today && has_current_pinned);
        if !hit {
            break;
        }
        streak += 1;
        earliest = Some(s);
        cursor = cursor - chrono::Duration::days(1);
    }
    // max streak in retention window：扫 dates_with_sighting 找最长连续
    // segment。把今日 fallback 也并入临时 set 后算。
    let mut all_dates: std::collections::BTreeSet<String> =
        dates_with_sighting.iter().cloned().collect();
    if has_current_pinned {
        all_dates.insert(date_str(today));
    }
    // dates 已 BTreeSet 排序；iterate + walk consecutive
    let mut max_streak = 0usize;
    let mut cur_streak = 0usize;
    let mut prev_date: Option<chrono::NaiveDate> = None;
    for s in &all_dates {
        let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") else {
            continue;
        };
        let consecutive = match prev_date {
            Some(pd) => (d - pd).num_days() == 1,
            None => false,
        };
        if consecutive {
            cur_streak += 1;
        } else {
            cur_streak = 1;
        }
        if cur_streak > max_streak {
            max_streak = cur_streak;
        }
        prev_date = Some(d);
    }
    (streak, earliest, max_streak)
}

/// `/idle_7d` 命令回复文案。pure：caller (bot.rs handler) 已 filter
/// pending + updated_at ≥ 7d 前的 task + 算 idle 天数 + 按 days desc
/// 排好 + 取 last update YYYY-MM-DD label。formatter 拼文案 + 空兜底
/// + cap 12（backlog audit 列表用）。
/// row：(title, days_idle, last_update_date_str)。
pub fn format_idle_7d_reply(
    rows: &[(String, i64, String)],
) -> String {
    if rows.is_empty() {
        return "💤 无 7d+ idle pending — 健康状态。\n试 /touched_thisweek 看本周活跃 task。".to_string();
    }
    const IDLE_CAP: usize = 12;
    let shown = &rows[..rows.len().min(IDLE_CAP)];
    let mut out = format!(
        "💤 stale backlog {} 条（pending + updated_at ≥ 7 天前）：",
        rows.len(),
    );
    for (title, days, last_date) in shown {
        out.push_str(&format!(
            "\n· 「{}」 · idle {} 天（last update {}）",
            title, days, last_date,
        ));
    }
    if rows.len() > IDLE_CAP {
        out.push_str(&format!(
            "\n…还有 {} 条（idle 较短的截掉）",
            rows.len() - IDLE_CAP,
        ));
    }
    out
}

/// `/tag <name>` 命令回复文案。pure：在 views（已 chat-scoped）里找
/// tags 数组含 `name`（case-insensitive，full-token 等值）的 task。
/// 输出 status emoji + title + due（如有）。与 /find（子串搜）正交 —
/// /tag 是精确 tag 等值匹配。
///
/// 空 name → usage hint。无命中 → 友好兜底 + 推 /tags 看可用 tag。
pub fn format_tag_reply(
    views: &[crate::task_queue::TaskView],
    name: &str,
) -> String {
    use crate::task_queue::TaskStatus;
    let kw = name.trim();
    if kw.is_empty() {
        return "🏷 用法：/tag <name>\n列含某 #tag 的所有 task（含 / 不含 `#` 前缀都接受，case-insensitive）。\n例：/tag 工作 / /tag #urgent / /tag 健身\n\n相关：/tags 看本聊天用过的所有 tag + 各自任务数。"
            .to_string();
    }
    let kw_lower = kw.to_lowercase();
    let mut hits: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| {
            v.tags
                .iter()
                .any(|t| t.to_lowercase() == kw_lower)
        })
        .collect();
    // pending / error 先（owner 当下更可能想 audit 活跃任务），其次
    // done / cancelled。同状态保 views 原序（视图已 compare_for_queue）。
    let status_rank = |s: &TaskStatus| match s {
        TaskStatus::Pending => 0u8,
        TaskStatus::Error => 1,
        TaskStatus::Done => 2,
        TaskStatus::Cancelled => 3,
    };
    hits.sort_by_key(|v| status_rank(&v.status));
    if hits.is_empty() {
        return format!(
            "🏷 没有任务带 #{}。\n试 /tags 看本聊天用过的所有 tag + 各自任务数。",
            kw
        );
    }
    let cap = 20;
    let shown = &hits[..hits.len().min(cap)];
    let mut out = format!("🏷 #{} 命中 {} 条：", kw, hits.len());
    for v in shown {
        let emoji = match v.status {
            TaskStatus::Pending => "🟢",
            TaskStatus::Error => "⚠️",
            TaskStatus::Done => "✅",
            TaskStatus::Cancelled => "🚫",
        };
        let due_part = match &v.due {
            Some(d) if !d.is_empty() => {
                // "YYYY-MM-DDTHH:MM" → "MM-DD HH:MM" 紧凑
                let short = if d.len() >= 16 {
                    format!("{} {}", &d[5..10], &d[11..16])
                } else {
                    d.clone()
                };
                format!(" · ⏰ {}", short)
            }
            _ => String::new(),
        };
        out.push_str(&format!("\n{} {}{}", emoji, v.title, due_part));
    }
    if hits.len() > cap {
        out.push_str(&format!(
            "\n…还有 {} 条带本 tag（共 {} 条，本行仅显前 {}）",
            hits.len() - cap,
            hits.len(),
            cap
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

/// `/forks <title>` 命令回复文案。pure：扫 views 找所有 active（Pending /
/// Error）task 的 blocked_by 含 target_title 的 — 反向 audit「解锁 target
/// 会让谁动起来」。
///
/// 与 /blocked 对偶但 scope 反向：
/// - /blocked：以"被卡"为视角，列被 blockedBy 锁住的 + 列锁住它的 blocker
/// - /forks：以"卡别人"为视角，给定一个 title，列谁在等它解锁
///
/// 空 target_title → usage hint（caller 在 handler 已用 missing_argument
/// 兜底；这里防御性也覆盖一遍避免直接调 fn 时 panic）。
/// 无命中 → 友好兜底文案：解锁不影响任何其它 task（这条 task 是叶子节点）。
/// blocked_by 是 `Vec<String>`：内部元素来自 description 的 `[blockedBy: ...]`
/// marker；title 比较 trim 后字面相等（与 unresolved_blockers 算法一致）。
pub fn format_forks_reply(
    views: &[crate::task_queue::TaskView],
    target_title: &str,
) -> String {
    use crate::task_queue::TaskStatus;
    let target = target_title.trim();
    if target.is_empty() {
        return "🔱 用法：/forks <title>\n\n反向 audit：哪些 active task 在等这条解锁。空 title → 此提示。".to_string();
    }
    let mut rows: Vec<(&str, &TaskStatus)> = Vec::new();
    for v in views {
        if !matches!(v.status, TaskStatus::Pending | TaskStatus::Error) {
            continue;
        }
        if v.blocked_by.iter().any(|b| b.trim() == target) {
            rows.push((v.title.as_str(), &v.status));
        }
    }
    if rows.is_empty() {
        return format!(
            "🔱 解锁「{}」不会影响其它 active task（叶子节点 / 无引用方）。",
            target
        );
    }
    let mut out = format!("🔱 解锁「{}」会松开 {} 条 task：", target, rows.len());
    for (title, status) in &rows {
        let icon = match status {
            TaskStatus::Pending => "🟢",
            TaskStatus::Error => "⚠️",
            _ => "·",
        };
        out.push_str(&format!("\n{} {}", icon, title));
    }
    out
}

/// `/blocked_by <title>` 命令回复文案。pure：找 title 对应的 view +
/// 列其 blocked_by markers 中**仍未解决**的 blocker（即 blocker 在
/// active 集合中 — 已 done / cancelled 的 blocker 视作已解决跳过）。
///
/// 与 /forks 反向 — /forks 列「谁等我」（owner 解锁 title 后谁会
/// 动起来）；/blocked_by 列「我等谁」（title 卡在等什么）。与
/// /blocked（全 chat audit）对比 — 那个跨任务列所有被卡的，本命令
/// 聚焦单条。
///
/// 状态机：
/// - 空 target_title → defensive usage hint（caller 已用 missing-arg
///   兜底；这里防御性覆盖）
/// - target 在 views 找不到 → "task 不存在"错误（caller resolve 失败）
/// - target blocked_by 为空 → "无 blockedBy markers — 这条不在等谁"
/// - target blocked_by 全部已解决 → "所有 blocker 已解决 ✨"
/// - 有未解决 blocker → 列表显
pub fn format_blocked_by_reply(
    views: &[crate::task_queue::TaskView],
    target_title: &str,
) -> String {
    use crate::task_queue::TaskStatus;
    let target = target_title.trim();
    if target.is_empty() {
        return "🔒 用法：/blocked_by <title>\n\n单条 audit：title 卡在等什么 active blocker。".to_string();
    }
    let Some(target_view) = views.iter().find(|v| v.title == target) else {
        return format!("🔒 没找到 task「{}」。", target);
    };
    if target_view.blocked_by.is_empty() {
        return format!(
            "🔒 「{}」无 `[blockedBy: ...]` markers — 这条不在等任何 blocker。",
            target
        );
    }
    // active 集合用于 unresolved 判定。task_view 含 done / cancelled
    // — 仅 Pending / Error 视作活跃 blocker。
    let active: std::collections::HashSet<&str> = views
        .iter()
        .filter(|v| matches!(v.status, TaskStatus::Pending | TaskStatus::Error))
        .map(|v| v.title.as_str())
        .collect();
    let unresolved: Vec<&str> = target_view
        .blocked_by
        .iter()
        .filter(|b| active.contains(b.trim()))
        .map(|s| s.as_str())
        .collect();
    if unresolved.is_empty() {
        let total = target_view.blocked_by.len();
        return format!(
            "✨ 「{}」的 {} 条 blocker 均已解决 — 可以推进了。",
            target, total
        );
    }
    let mut out = format!(
        "🔒 「{}」被 {} 条 blocker 卡住（共 {} 条 marker / {} 仍未解决）：",
        target,
        unresolved.len(),
        target_view.blocked_by.len(),
        unresolved.len()
    );
    for b in &unresolved {
        // active 集合命中 → 对应 view 必存在；status emoji 根据 status 选
        let icon = views
            .iter()
            .find(|v| v.title == b.trim())
            .map(|v| match v.status {
                TaskStatus::Pending => "🟢",
                TaskStatus::Error => "⚠️",
                _ => "·",
            })
            .unwrap_or("·");
        out.push_str(&format!("\n{} {}", icon, b));
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

/// `/sleep_until <HH:MM>` 时刻解析：accept "HH:MM" / "H:MM" / "HH" /
/// "H"（单数字视为 HH:00）。invalid → None。trim + clamp 24h × 60m。
///
/// 与既有 chrono parse 模板不依赖 — 简单 split + parse 让单测稳定且
/// 不引 chrono 时区干扰。
pub fn parse_sleep_until_time(s: &str) -> Option<(u8, u8)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some((h_str, m_str)) = s.split_once(':') {
        let h: u8 = h_str.trim().parse().ok()?;
        let m: u8 = m_str.trim().parse().ok()?;
        if h < 24 && m < 60 {
            Some((h, m))
        } else {
            None
        }
    } else {
        let h: u8 = s.parse().ok()?;
        if h < 24 {
            Some((h, 0))
        } else {
            None
        }
    }
}

/// `/sleep_until` 命令回复文案。pure：caller 已 parse 出 target +
/// 计算 minutes + 调 `set_mute_minutes(minutes)`；本函数仅按 (raw_arg,
/// parsed_time, minutes, until_local) 拼 owner 友好文案。
///
/// - raw 空 / parse 失败 → usage hint
/// - 成功 → 「🌙 已静音到 HH:MM（N 分钟后自动解除）」+ 跨日提示
pub fn format_sleep_until_reply(
    raw_arg: &str,
    parsed_time: Option<(u8, u8)>,
    minutes: i64,
    until_local: Option<chrono::DateTime<chrono::Local>>,
    crosses_midnight: bool,
) -> String {
    if raw_arg.trim().is_empty() {
        return "🌙 用法：/sleep_until <HH:MM>\n静音 proactive 到指定本地时刻（HH:MM 24h；H:MM / HH / H 单数字也接受）。目标 ≤ now → 落明日同时刻。\n例：/sleep_until 8:00 / /sleep_until 22:30 / /sleep_until 14"
            .to_string();
    }
    let Some((h, m)) = parsed_time else {
        return format!(
            "🌙 「{}」不是合法时刻。\n用法：/sleep_until <HH:MM>（24h；H:MM / HH / H 单数字也行）。\n例：/sleep_until 8:00 / /sleep_until 22:30 / /sleep_until 14",
            raw_arg.trim()
        );
    };
    let when = until_local
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_else(|| format!("{:02}:{:02}", h, m));
    let nice = if minutes < 60 {
        format!("{} 分钟", minutes)
    } else if minutes < 60 * 24 {
        let hh = minutes / 60;
        let mm = minutes % 60;
        if mm == 0 {
            format!("{} 小时", hh)
        } else {
            format!("{} 小时 {} 分钟", hh, mm)
        }
    } else {
        let d = minutes / (60 * 24);
        let hh = (minutes % (60 * 24)) / 60;
        if hh == 0 {
            format!("{} 天", d)
        } else {
            format!("{} 天 {} 小时", d, hh)
        }
    };
    let cross_hint = if crosses_midnight {
        "（明日同时刻 — 目标 ≤ now 自动跨日）"
    } else {
        ""
    };
    format!(
        "🌙 已静音 proactive 到 {}{}（{} 后自动解除）。期间宠物不主动开口；用 /mute 0 立刻解除。",
        when, cross_hint, nice
    )
}

/// `/snooze_until` 命令回复文案。pure：caller 已 parse title + HH:MM
/// + 算 target Local time + 调 task_set_snooze。本函数仅按入参拼
/// owner 友好文案。
///
/// 4 态：
/// - title 空 → usage hint
/// - time 解析失败（None）→ 错误 + 用法 hint
/// - title resolve 失败（save_ok=Err）→ 显具体错误
/// - 成功 → 「💤 已 snooze『title』到 HH:MM（跨日 hint 如有）」
pub fn format_snooze_until_reply(
    title: &str,
    time: Option<(u8, u8)>,
    until_local: Option<chrono::DateTime<chrono::Local>>,
    crosses_midnight: bool,
    save_ok: Result<(), String>,
) -> String {
    let t = title.trim();
    if t.is_empty() {
        return "💤 用法：/snooze_until <title> <HH:MM>\n把任务 snooze 到指定本地时刻（HH:MM 24h；H:MM / HH / H 单数字也接受）。\n例：/snooze_until 整理 Downloads 18:00\n例：/snooze_until 写周报 9:00\n\n与 /snooze <title> [preset] 互补 — 那个走相对预设（30m / 2h / tonight 等），本命令是绝对时刻。".to_string();
    }
    let Some((h, m)) = time else {
        return format!(
            "💤 「{}」末尾不是合法时刻。\n用法：/snooze_until <title> <HH:MM>（24h；H:MM / HH / H 单数字也接受）。\n例：/snooze_until 整理 Downloads 18:00",
            t
        );
    };
    if let Err(reason) = save_ok {
        return format!(
            "💤 设 snooze 失败：{}\n（title `{}` / 时刻 {:02}:{:02}）",
            reason, t, h, m
        );
    }
    let when = until_local
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_else(|| format!("{:02}:{:02}", h, m));
    let cross_hint = if crosses_midnight {
        "（明日同时刻 — 目标 ≤ now 自动跨日）"
    } else {
        ""
    };
    format!(
        "💤 已 snooze 「{}」到 {}{}。期间不进 proactive 选单；用 /unsnooze {} 立刻解除。",
        t, when, cross_hint, t
    )
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

/// `/feedback_history [N]` 命令回复文案。pure。
///
/// 入参 `entries` 必须是 newest-first（caller 用 recent_feedback(n).await
/// 然后 reverse）— 渲染顺序与入参一致，让"最近一条"显在 TG 屏顶。
///
/// - 空 entries → 友好兜底文案 + 引导 /feedback 写第一条
/// - 非空 → "📜 最近 N 条 feedback：" header + 逐行 `· HH:MM <emoji>
///   <excerpt>` 列表
///
/// kind emoji map 让 owner 一眼分辨"主动正反馈 / 主动负反馈 / 被动
/// 信号 / 评论"四类。excerpt 来自 feedback_history.log 已 cap 64 字
/// （FEEDBACK_EXCERPT_CHARS），TG msg 总长 N=20 × ~90 char = 1800
/// 内 — 远在 4096 limit 内。
pub fn format_feedback_history_reply(
    entries: &[crate::feedback_history::FeedbackEntry],
    n: u32,
) -> String {
    if entries.is_empty() {
        return "📜 暂无 feedback 记录。\n\n用 /feedback <text> 写第一条；或自然交互（回复 / 主动点掉宠物开口 / 👍 给 ✅）— 这些动作自动写 feedback_history.log。"
            .to_string();
    }
    let cap = (n as usize).max(1);
    let shown_n = entries.len().min(cap);
    let shown = &entries[..shown_n];
    let mut out = format!("📜 最近 {} 条 feedback：", shown.len());
    for e in shown {
        // timestamp "2026-05-17T18:45:32+08:00" → "HH:MM" 切片
        let when = if e.timestamp.len() >= 16 {
            e.timestamp[11..16].to_string()
        } else {
            e.timestamp.clone()
        };
        let emoji = match e.kind {
            crate::feedback_history::FeedbackKind::Replied => "✅",
            crate::feedback_history::FeedbackKind::Liked => "👍",
            crate::feedback_history::FeedbackKind::Comment => "💬",
            crate::feedback_history::FeedbackKind::Ignored => "🙉",
            crate::feedback_history::FeedbackKind::Dismissed => "👋",
            crate::feedback_history::FeedbackKind::Puzzled => "🤔",
        };
        out.push_str(&format!(
            "\n· {} {} {} | {}",
            when,
            emoji,
            e.kind.as_str(),
            e.excerpt
        ));
    }
    if entries.len() > shown_n {
        out.push_str(&format!(
            "\n…还有 {} 条更早记录（/feedback_history {} 看更多，上限 20）",
            entries.len() - shown_n,
            entries.len().min(20)
        ));
    }
    out
}

/// `/silent_all [minutes]` 命令回复文案。pure。
///
/// 入参语义：
/// - `armed_count`: arm 成功时新窗口的 silenced 条数（0 = 没 candidates，
///   armed_count == 0 + minutes > 0 走"无可 silent 任务"友好兜底）
/// - `released_count`: 同次操作释放的 prior 窗口条数（minutes==0 或 arm
///   先 release_active 时；用于"已解除 N"片段；0 = 没有上轮可释放）
/// - `minutes`: 用户传入分钟数（0 = release-only intent）
/// - `until_local`: arm 成功时新窗口到期时刻，None = release-only / 失败
///
/// 输出 4 种态：
/// - minutes == 0 + released > 0 → "🔊 已解除 N 条 silent"
/// - minutes == 0 + released == 0 → "✨ 当前无 silent 窗口可解除"
/// - minutes > 0 + armed > 0 → "⏸ 已 silent N 条·M 分钟后自动解除（到 HH:MM）"
///   含 released 信息：上轮 reset 时附加"（先解除上轮 X 条）"
/// - minutes > 0 + armed == 0 → "✨ 暂无可 silent 任务（无 pending 或都已 silent）"
pub fn format_silent_all_reply(
    armed_count: usize,
    released_count: usize,
    minutes: i64,
    until_local: Option<chrono::DateTime<chrono::Local>>,
) -> String {
    if minutes == 0 {
        if released_count == 0 {
            return "✨ 当前无 silent 窗口可解除。\n用 /silent_all [minutes] 开始批量 silent；minutes 缺省 60。".to_string();
        }
        return format!(
            "🔊 已解除 {} 条 butler_task 的临时 [silent]。",
            released_count
        );
    }
    if armed_count == 0 {
        return "✨ 暂无可 silent 任务（butler_tasks 段无 pending 或已全部 silent）。".to_string();
    }
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
    let when = until_local
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_else(|| "—".to_string());
    let release_note = if released_count > 0 {
        format!("（先解除上轮 {} 条）", released_count)
    } else {
        String::new()
    };
    format!(
        "⏸ 已 silent {} 条 butler_task{} · {} 后自动解除（到 {}）。\n\n期间 LLM proactive cycle 不会主动选这些 task；用 /silent_all 0 立即解除。",
        armed_count, release_note, nice, when
    )
}

/// `/alarms [N]` 命令回复文案。pure。
///
/// 入参 `rows`：`(target, topic, title)` 三元组，按 target 升序排（最近
/// 先 fire 在前），caller 负责排序 / 截 N。`now` 当前本地时刻 — 用来
/// 算每条 entry 的"剩余 N 分 / 已逾期 N 分"。
///
/// 输出：
/// - 空 rows → 友好兜底 + 引导 /task 用 [remind:] 或桌面 ⏰ chip 创建
/// - 非空 → "⏰ 最近 N 条 pending alarms：" header + 逐行
///   `· HH:MM <剩 / 逾期 N 分> | <topic>` 列表
///
/// "剩 / 逾期" 计算用 chrono::Duration delta；分钟级精度（与 PanelTasks
/// dueRelative 同心智）。Absolute 与 TodayHour 都对应"已格式化"的
/// time 字符串，对 owner 仅显 HH:MM 部分（Absolute 含 YYYY-MM-DD 但
/// 在剩余分钟语境下日期可隐含 — 简化输出）。
pub fn format_alarms_reply(
    rows: &[(
        crate::proactive::ReminderTarget,
        String, // topic
        String, // title
    )],
    now: chrono::NaiveDateTime,
    n: u32,
) -> String {
    if rows.is_empty() {
        return "⏰ 暂无 pending alarms。\n\n桌面 PanelMemory 任意 item ⏰ chip / 直接创建 `todo` 条目 `[remind: HH:MM] <topic>` 都能设；到点 ChatMini 软提醒。".to_string();
    }
    let cap = (n as usize).max(1);
    let shown_n = rows.len().min(cap);
    let shown = &rows[..shown_n];
    let mut out = format!("⏰ 最近 {} 条 pending alarms：", shown.len());
    for (target, topic, _title) in shown {
        let target_dt = match target {
            crate::proactive::ReminderTarget::Absolute(dt) => *dt,
            crate::proactive::ReminderTarget::TodayHour(h, m) => {
                // 与 is_reminder_due TodayHour 路径同语义：取今日 HH:MM
                // 若已过且小于半天，按今日；否则按明日 — owner 心智"下次
                // fire"的最近未来时刻。
                let today_t = now
                    .date()
                    .and_hms_opt(*h as u32, *m as u32, 0)
                    .unwrap_or(now);
                if today_t >= now {
                    today_t
                } else {
                    // 已过今日 HH:MM — 按"今日已逾期"显示（不按明日，
                    // 避免误导 owner 以为这条还会自动 fire 明天 —— 实
                    // 际 reminder 单次 fire 后就该消费掉 / 不再触发）
                    today_t
                }
            }
        };
        let delta = target_dt - now;
        let mins = delta.num_minutes();
        let when_label = format_target_short(target);
        let remaining_label = if mins.abs() < 60 {
            if mins >= 0 {
                format!("剩 {} 分", mins.max(1))
            } else {
                format!("已逾期 {} 分", (-mins).max(1))
            }
        } else if mins.abs() < 60 * 24 {
            let h = mins.abs() / 60;
            if mins >= 0 {
                format!("剩 {} 小时", h)
            } else {
                format!("已逾期 {} 小时", h)
            }
        } else {
            let d = mins.abs() / (60 * 24);
            if mins >= 0 {
                format!("剩 {} 天", d)
            } else {
                format!("已逾期 {} 天", d)
            }
        };
        out.push_str(&format!(
            "\n· {} ({}) | {}",
            when_label, remaining_label, topic
        ));
    }
    if rows.len() > shown_n {
        out.push_str(&format!(
            "\n…还有 {} 条更晚 alarms（/alarms {} 看更多，上限 20）",
            rows.len() - shown_n,
            rows.len().min(20)
        ));
    }
    out
}

/// `/alarms_today` 命令回复文案。pure：与 `format_alarms_reply` 同结构
/// 但 filter 到 target 落在本地今日的 alarm。无 N cap — 今日范围天然
/// 小。
///
/// filter 规则：
/// - `TodayHour(h,m)` — 永远算今日（按定义）
/// - `Absolute(dt)` — 仅 `dt.date() == today` 才算
///
/// 输出 header 改「⏰ 今日（YYYY-MM-DD）N 条 alarms」让 scope 明确；
/// 行格式（HH:MM + 剩余 / 逾期 + topic）与 /alarms 同。
pub fn format_alarms_today_reply(
    rows: &[(
        crate::proactive::ReminderTarget,
        String, // topic
        String, // title
    )],
    now: chrono::NaiveDateTime,
) -> String {
    let today = now.date();
    let today_str = today.format("%Y-%m-%d").to_string();
    // filter rows whose target lands on today's local date
    let filtered: Vec<&(crate::proactive::ReminderTarget, String, String)> = rows
        .iter()
        .filter(|(target, _, _)| match target {
            crate::proactive::ReminderTarget::TodayHour(_, _) => true,
            crate::proactive::ReminderTarget::Absolute(dt) => dt.date() == today,
        })
        .collect();
    if filtered.is_empty() {
        return format!(
            "⏰ 今日（{}）暂无 alarm。\n用 /alarms 看不限日期的 pending alarms / 桌面 PanelMemory todo 段创建新 reminder。",
            today_str,
        );
    }
    let mut out = format!(
        "⏰ 今日（{}）{} 条 alarms：",
        today_str,
        filtered.len(),
    );
    for (target, topic, _title) in &filtered {
        let target_dt = match target {
            crate::proactive::ReminderTarget::Absolute(dt) => *dt,
            crate::proactive::ReminderTarget::TodayHour(h, m) => now
                .date()
                .and_hms_opt(*h as u32, *m as u32, 0)
                .unwrap_or(now),
        };
        let delta = target_dt - now;
        let mins = delta.num_minutes();
        // today scope — 显 HH:MM only（日期已在 header）
        let when_label = match target {
            crate::proactive::ReminderTarget::TodayHour(h, m) => {
                format!("{:02}:{:02}", h, m)
            }
            crate::proactive::ReminderTarget::Absolute(dt) => {
                dt.format("%H:%M").to_string()
            }
        };
        // 剩 / 逾期 — 与 /alarms 同分级算法
        let remaining_label = if mins.abs() < 60 {
            if mins >= 0 {
                format!("剩 {} 分", mins.max(1))
            } else {
                format!("已逾期 {} 分", (-mins).max(1))
            }
        } else if mins.abs() < 60 * 24 {
            let h = mins.abs() / 60;
            if mins >= 0 {
                format!("剩 {} 小时", h)
            } else {
                format!("已逾期 {} 小时", h)
            }
        } else {
            // 今日切片不太可能有 ≥ 1 天 delta（target 落今日 → max 24h）；
            // 防御性兜底
            let d = mins.abs() / (60 * 24);
            if mins >= 0 {
                format!("剩 {} 天", d)
            } else {
                format!("已逾期 {} 天", d)
            }
        };
        out.push_str(&format!(
            "\n· {} ({}) | {}",
            when_label, remaining_label, topic,
        ));
    }
    out
}

/// `/alarms_thisweek` 命令回复文案。pure：与 `format_alarms_today_reply`
/// 同结构，filter 改为 target 落在本周（week_start..=week_end_inclusive，
/// week_end 为 week_start + 6 天）。跨日 scope 行带 MM-DD HH:MM（与
/// /alarms 一致）。空集兜底教学指 /alarms 全量 / /alarms_today。
pub fn format_alarms_thisweek_reply(
    rows: &[(
        crate::proactive::ReminderTarget,
        String, // topic
        String, // title
    )],
    now: chrono::NaiveDateTime,
    week_start: chrono::NaiveDate,
) -> String {
    let week_end = week_start + chrono::Duration::days(6);
    let week_start_str = week_start.format("%Y-%m-%d").to_string();
    let filtered: Vec<&(crate::proactive::ReminderTarget, String, String)> = rows
        .iter()
        .filter(|(target, _, _)| match target {
            crate::proactive::ReminderTarget::TodayHour(_, _) => true, // 今日按定义算本周
            crate::proactive::ReminderTarget::Absolute(dt) => {
                let d = dt.date();
                d >= week_start && d <= week_end
            }
        })
        .collect();
    if filtered.is_empty() {
        return format!(
            "⏰ 本周（{} 起）暂无 alarm。\n用 /alarms 看不限日期 / /alarms_today 看仅今日 / /touched_thisweek 看本周 task 全谱。",
            week_start_str,
        );
    }
    let mut out = format!(
        "⏰ 本周（{} 起）{} 条 alarms：",
        week_start_str,
        filtered.len(),
    );
    for (target, topic, _title) in &filtered {
        let target_dt = match target {
            crate::proactive::ReminderTarget::Absolute(dt) => *dt,
            crate::proactive::ReminderTarget::TodayHour(h, m) => now
                .date()
                .and_hms_opt(*h as u32, *m as u32, 0)
                .unwrap_or(now),
        };
        let delta = target_dt - now;
        let mins = delta.num_minutes();
        // 跨日 scope — 行 MM-DD HH:MM（与 /alarms 同；/alarms_today 是仅
        // HH:MM 因 single day scope）
        let when_label = format_target_short(target);
        let remaining_label = if mins.abs() < 60 {
            if mins >= 0 {
                format!("剩 {} 分", mins.max(1))
            } else {
                format!("已逾期 {} 分", (-mins).max(1))
            }
        } else if mins.abs() < 60 * 24 {
            let h = mins.abs() / 60;
            if mins >= 0 {
                format!("剩 {} 小时", h)
            } else {
                format!("已逾期 {} 小时", h)
            }
        } else {
            let d = mins.abs() / (60 * 24);
            if mins >= 0 {
                format!("剩 {} 天", d)
            } else {
                format!("已逾期 {} 天", d)
            }
        };
        out.push_str(&format!(
            "\n· {} ({}) | {}",
            when_label, remaining_label, topic,
        ));
    }
    out
}

/// pure helper：把 ReminderTarget 渲染为短格式（HH:MM 或 MM-DD HH:MM），
/// 让 /alarms 输出在 list 行内紧凑。Absolute 含日期 — 若 target 日期与
/// "今日 (now.date())" 相同则省日期段，否则显 MM-DD HH:MM。
fn format_target_short(target: &crate::proactive::ReminderTarget) -> String {
    match target {
        crate::proactive::ReminderTarget::TodayHour(h, m) => {
            format!("{:02}:{:02}", h, m)
        }
        crate::proactive::ReminderTarget::Absolute(dt) => {
            // MM-DD HH:MM 紧凑。今天的也显日期保格式一致 — 在 list 里
            // 区分"今天 14:00" 还是"5/20 14:00" 比省字符值钱。
            dt.format("%m-%d %H:%M").to_string()
        }
    }
}

/// `/recent_chats [N]` 命令回复文案。pure。
///
/// 入参：
/// - `items`: `(role, excerpt)` 二元组，按聊天顺序（最早在前），caller
///   已 cap N + truncate excerpt 至 EXCERPT_CHARS。
/// - `session_title` / `session_updated_at`: 当前 active session 元数据；
///   formatter 在 header 里呈现一次（per-msg ts 不在后端 schema，所以
///   只能给"会话级"时刻信号）。
/// - `n`: clamp 后的 N 值；用于"还有 M 条更早" overflow hint 算法。
/// - `total`: 当前 session 内 user/assistant 总条数（含 N 之外的旧 items），
///   formatter 用 `total - items.len()` 算 overflow 数。
///
/// 输出态：
/// - active session 不存在 / 空 → 友好兜底
/// - 有 items → header（"💬 最近 N 条 chat · 会话「title」更新 HH:MM"）+
///   逐行 `<role glyph> <excerpt>` + overflow hint（如有）
///
/// role glyph：🧑 user / 🐾 assistant — 与桌面 ChatPanel export markdown
/// 同视觉锚（panelChatBits.tsx export 路径用同 emoji）。
pub const RECENT_CHATS_EXCERPT_CHARS: usize = 80;
pub fn format_recent_chats_reply(
    items: &[(String, String)],
    session_title: &str,
    session_updated_at: &str,
    n: u32,
    total: usize,
) -> String {
    if items.is_empty() {
        return "💬 暂无聊天记录。\n\n用 ChatMini 或 ChatPanel 跟 pet 聊一句，再 /recent_chats 看回顾。".to_string();
    }
    // session_updated_at: "2026-05-17T18:30:00.000" → 切 "MM-DD HH:MM"
    let when = if session_updated_at.len() >= 16 {
        format!(
            "{} {}",
            &session_updated_at[5..10],
            &session_updated_at[11..16]
        )
    } else {
        session_updated_at.to_string()
    };
    let title_short = if session_title.chars().count() > 24 {
        let head: String = session_title.chars().take(24).collect();
        format!("{}…", head)
    } else {
        session_title.to_string()
    };
    let mut out = format!(
        "💬 最近 {} 条 chat · 会话「{}」最近活动 {}：",
        items.len(),
        if title_short.is_empty() {
            "（无标题）".to_string()
        } else {
            title_short
        },
        when
    );
    for (role, excerpt) in items {
        let glyph = match role.as_str() {
            "user" => "🧑",
            "assistant" => "🐾",
            _ => "·",
        };
        out.push_str(&format!("\n{} {}", glyph, excerpt));
    }
    let _ = n;
    let overflow = total.saturating_sub(items.len());
    if overflow > 0 {
        out.push_str(&format!(
            "\n…还有 {} 条更早消息（/recent_chats {} 看更多，上限 20）",
            overflow,
            total.min(20)
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

/// `/tags` 命令回复文案。pure：扫 views（已过滤本 chat 派单），统计 tag
/// 计数（无视 done/cancelled — owner audit 时 "我用过哪些 tag" 是历史维
/// 度，不只看 active）。按数量 desc + 名字 asc 二阶排序保结果稳定。空
/// tag 矩阵 → 友好兜底文案；超 TAGS_CAP 个 → 列前 15 + "其它 N 个" 汇
/// 总。同时输出"无 #tag 任务"数让 owner 看到分母。
/// `/tags_today` 命令回复文案。pure：filter views by updated_at 起始
/// 匹配 today 日期前缀，扫 tags 计数。与 /tags 同算法但 scope 限今日。
/// 无 cap — 今日范围天然小（典型 < 10 个不同 tag）。
pub fn format_tags_today_reply(
    views: &[crate::task_queue::TaskView],
    today: chrono::NaiveDate,
) -> String {
    use std::collections::BTreeMap;
    let today_str = today.format("%Y-%m-%d").to_string();
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut untagged: u32 = 0;
    for v in views {
        // 仅扫今日 updated_at 命中
        if v.updated_at.len() < 10 || &v.updated_at[..10] != today_str.as_str() {
            continue;
        }
        if v.tags.is_empty() {
            untagged += 1;
        } else {
            for t in &v.tags {
                let key = t.trim();
                if key.is_empty() {
                    continue;
                }
                *counts.entry(key.to_string()).or_insert(0) += 1;
            }
        }
    }
    if counts.is_empty() {
        return format!(
            "🏷 今日（{}）动过的 task 都无 #tag。\n创建任务时在 description 写 `#name` 自动收录；/tags 看全量 #tag 矩阵。",
            today_str,
        );
    }
    // 与 /tags 同 sort：desc by count，ties 字典序（BTreeMap 默认 key asc）
    let mut sorted: Vec<(String, u32)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let total_tags = sorted.len();
    let mut out = format!(
        "🏷 今日（{}）{} 个 tag",
        today_str,
        total_tags,
    );
    for (name, count) in &sorted {
        out.push_str(&format!("\n· #{} ×{}", name, count));
    }
    if untagged > 0 {
        out.push_str(&format!("\n\n无 #tag 任务（今日）：{} 条", untagged));
    }
    out
}

/// `/tags_yesterday` 命令回复文案。pure。与 `format_tags_today_reply`
/// 同结构（filter / 聚合 / sort 完全一致），仅 scope 是 yesterday 日
/// 期 + 标题用「昨日」+ 空集兜底教学指 /tags 全量 / /tags_today 今日
/// （避免循环建议 yesterday）。
pub fn format_tags_yesterday_reply(
    views: &[crate::task_queue::TaskView],
    yesterday: chrono::NaiveDate,
) -> String {
    use std::collections::BTreeMap;
    let yesterday_str = yesterday.format("%Y-%m-%d").to_string();
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut untagged: u32 = 0;
    for v in views {
        if v.updated_at.len() < 10 || &v.updated_at[..10] != yesterday_str.as_str() {
            continue;
        }
        if v.tags.is_empty() {
            untagged += 1;
        } else {
            for t in &v.tags {
                let key = t.trim();
                if key.is_empty() {
                    continue;
                }
                *counts.entry(key.to_string()).or_insert(0) += 1;
            }
        }
    }
    if counts.is_empty() {
        return format!(
            "🏷 昨日（{}）动过的 task 都无 #tag。\n用 /tags 看全量 / /tags_today 看今日 / /touched_yesterday 看昨日全谱。",
            yesterday_str,
        );
    }
    let mut sorted: Vec<(String, u32)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let total_tags = sorted.len();
    let mut out = format!(
        "🏷 昨日（{}）{} 个 tag",
        yesterday_str,
        total_tags,
    );
    for (name, count) in &sorted {
        out.push_str(&format!("\n· #{} ×{}", name, count));
    }
    if untagged > 0 {
        out.push_str(&format!("\n\n无 #tag 任务（昨日）：{} 条", untagged));
    }
    out
}

/// `/tags_thisweek` 命令回复文案。pure。与 `format_tags_today_reply` 同
/// 结构，filter 改为 updated_at >= week_start 日期前缀（与 /touched_
/// thisweek / /search_thisweek 同 ISO 字典序比较）。空集兜底教学指
/// /tags 全量 / /tags_today / /touched_thisweek（avoid loop）。
pub fn format_tags_thisweek_reply(
    views: &[crate::task_queue::TaskView],
    week_start: chrono::NaiveDate,
) -> String {
    use std::collections::BTreeMap;
    let week_start_str = week_start.format("%Y-%m-%d").to_string();
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut untagged: u32 = 0;
    for v in views {
        if v.updated_at.len() < 10 || &v.updated_at[..10] < week_start_str.as_str() {
            continue;
        }
        if v.tags.is_empty() {
            untagged += 1;
        } else {
            for t in &v.tags {
                let key = t.trim();
                if key.is_empty() {
                    continue;
                }
                *counts.entry(key.to_string()).or_insert(0) += 1;
            }
        }
    }
    if counts.is_empty() {
        return format!(
            "🏷 本周（{} 起）动过的 task 都无 #tag。\n用 /tags 看全量 / /tags_today 看今日 / /touched_thisweek 看本周全谱。",
            week_start_str,
        );
    }
    let mut sorted: Vec<(String, u32)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let total_tags = sorted.len();
    let mut out = format!(
        "🏷 本周（{} 起）{} 个 tag",
        week_start_str,
        total_tags,
    );
    for (name, count) in &sorted {
        out.push_str(&format!("\n· #{} ×{}", name, count));
    }
    if untagged > 0 {
        out.push_str(&format!("\n\n无 #tag 任务（本周）：{} 条", untagged));
    }
    out
}

pub const TAGS_CAP: usize = 15;
pub fn format_tags_reply(views: &[crate::task_queue::TaskView]) -> String {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut untagged: u32 = 0;
    for v in views {
        if v.tags.is_empty() {
            untagged += 1;
        } else {
            for t in &v.tags {
                let key = t.trim();
                if key.is_empty() {
                    continue;
                }
                *counts.entry(key.to_string()).or_insert(0) += 1;
            }
        }
    }
    if counts.is_empty() {
        return format!(
            "🏷 /tags\n\n本聊天派单暂无 #tag 任务（{} 条任务无 tag）。\n创建任务时在 description 写 `#name` 即被自动收录（如 #健身 / #读书）。",
            untagged
        );
    }
    // counts.into_iter() 默认按 key 字典序（BTreeMap）；再 by-count desc 排序
    // 用 stable sort 保 ties 字典序。
    let mut sorted: Vec<(String, u32)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let total_tags = sorted.len();
    let mut out = String::new();
    out.push_str(&format!("🏷 /tags（共 {} 个 tag）\n", total_tags));
    let shown = sorted.iter().take(TAGS_CAP);
    for (name, count) in shown {
        out.push_str(&format!("\n· #{} ×{}", name, count));
    }
    if total_tags > TAGS_CAP {
        out.push_str(&format!("\n…还有 {} 个 tag", total_tags - TAGS_CAP));
    }
    if untagged > 0 {
        out.push_str(&format!("\n\n无 #tag 任务：{} 条", untagged));
    }
    out
}

/// `/tags_for <title>` 命令回复文案。pure：单条 task 的 tags 清单。
///
/// 状态机：
/// - 空 target_title → usage hint（caller 已用 missing-arg 兜底；
///   防御性覆盖）
/// - target 在 views 找不到 → "没找到 task"
/// - target.tags 空 → 「无 #tag 标记」+ 提示在 description 写 `#name`
///   自动收录
/// - 有 tags → 「🏷 「<title>」N 个 tag：#a #b ...」
pub fn format_tags_for_reply(
    views: &[crate::task_queue::TaskView],
    target_title: &str,
) -> String {
    let target = target_title.trim();
    if target.is_empty() {
        return "🏷 用法：/tags_for <title>\n\n单条 audit — 列 title 标的所有 #tag。".to_string();
    }
    let Some(target_view) = views.iter().find(|v| v.title == target) else {
        return format!("🏷 没找到 task「{}」。", target);
    };
    if target_view.tags.is_empty() {
        return format!(
            "🏷 「{}」无 #tag 标记。\n在 description 写 `#name`（如 #健身 / #读书）即被自动收录。",
            target
        );
    }
    let tags_str = target_view
        .tags
        .iter()
        .map(|t| format!("#{}", t.trim()))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "🏷 「{}」{} 个 tag：\n{}",
        target,
        target_view.tags.len(),
        tags_str
    )
}

/// `/touch <title>` 命令回复文案。pure：caller 已 resolve title +
/// 调 task_touch_inner。
///
/// 状态机：
/// - 空 title → usage hint
/// - save Ok → "✨ 已 touch「<title>」— updated_at 已刷新，老任务
///   重新冒头 proactive 选单。"
/// - save Err → "✨ touch 失败：<msg>"
pub fn format_touch_reply(title: &str, save_ok: Result<(), &str>) -> String {
    let t = title.trim();
    if t.is_empty() {
        return "✨ 用法：/touch <title>\n\n刷 updated_at 不改内容 — 让老 task 重新冒头 proactive 选单。\n\n例：/touch 整理 Downloads\n例：/touch 1   （/tasks 输出第 1 条）\n\n机制：rewrite 同 description → updated_at 自动 stamp 到 now。done / cancelled task 拒（终态 touch 无意义）。与 /skip（同 backend 但语义是「跳本轮 fire」）共享机制；decision_log 标 TaskTouch audit 区分。".to_string();
    }
    match save_ok {
        Ok(()) => format!(
            "✨ 已 touch「{}」— updated_at 已刷新，老任务重新冒头 proactive 选单。",
            t
        ),
        Err(e) => format!("✨ touch 失败：{}", e),
    }
}

/// `/cancel_all_error` 命令回复文案。pure：
/// - confirmed=false → usage hint with `confirm` token + error count
/// - confirmed=true + 0 cancelled → "本聊天暂无 error 任务" 兜底
/// - confirmed=true + N cancelled → "🧹 已 cancel N 条 error 任务" + 失败数
pub fn format_cancel_all_error_reply(
    confirmed: bool,
    error_count_before: u32,
    cancelled_ok: u32,
    cancelled_err: u32,
) -> String {
    if !confirmed {
        if error_count_before == 0 {
            return "🧹 /cancel_all_error confirm\n\n本聊天暂无 error 状态任务，无需批量 cancel。".to_string();
        }
        return format!(
            "🧹 /cancel_all_error confirm\n\n本聊天有 {} 条 error 状态任务等待 cancel。\n**这是破坏性操作 — 必须带 `confirm` token 才执行**：\n\n  /cancel_all_error confirm\n\n仅 cancel 本 chat 派单（origin == Tg<chat_id>）；其它 chat / 桌面任务不动。",
            error_count_before
        );
    }
    if cancelled_ok == 0 && cancelled_err == 0 {
        return "🧹 本聊天暂无 error 状态任务可 cancel ✨".to_string();
    }
    let mut out = format!(
        "🧹 已批量 cancel {} 条 error 任务",
        cancelled_ok
    );
    if cancelled_err > 0 {
        out.push_str(&format!("\n⚠️ {} 条 cancel 失败（可能并发改了状态）", cancelled_err));
    }
    out.push_str("\n用 /tasks 看更新后清单 / /retry <title> 单条重启。");
    out
}

/// `/promote_all_p7` 命令回复文案。pure：与 format_cancel_all_error_reply
/// 同模板（confirm-required 破坏性批量操作族）。
///
/// 状态机：
/// - confirmed=false + targets_before=0 → 「本聊天暂无可升级 task」兜底
/// - confirmed=false + targets_before>0 → usage hint 含 confirm token 提示
/// - confirmed=true + 0 changes → 友好兜底（"暂无 task 可升级"）
/// - confirmed=true + N changes → 「已升 N 条 / +1 到 P7」+ 失败计数（如有）
///
/// `targets_before` = 处理前候选数（active 状态 + pri < 7）；`promoted_ok`
/// + `promoted_err` 是执行后计数。calling code 负责自己 walk 候选 + 调
/// task_set_priority 累计；formatter 不做 IO。
pub fn format_promote_all_p7_reply(
    confirmed: bool,
    targets_before: u32,
    promoted_ok: u32,
    promoted_err: u32,
) -> String {
    if !confirmed {
        if targets_before == 0 {
            return "🎯 /promote_all_p7 confirm\n\n本聊天暂无可升级的 active task（所有 pending / error 任务已是 P7+，或全是 done / cancelled）。".to_string();
        }
        return format!(
            "🎯 /promote_all_p7 confirm\n\n本聊天有 {} 条 active task（pending / error）priority < 7 可升 +1。\n**这是批量修改 — 必须带 `confirm` token 才执行**：\n\n  /promote_all_p7 confirm\n\n语义：把每条 active task 的 priority 升 +1（clamp 7），已 ≥ P7 的不动。仅本 chat 派单（origin == Tg<chat_id>）。\n\n场景：紧急 sprint / deadline 收尾 — 让 LLM 立即优先所有挂着的活儿。",
            targets_before
        );
    }
    if promoted_ok == 0 && promoted_err == 0 {
        return "🎯 本聊天暂无可升级 task ✨（active 任务都已 ≥ P7 或无 active）".to_string();
    }
    let mut out = format!(
        "🎯 已批量升 {} 条 active task priority +1（clamp 7）",
        promoted_ok
    );
    if promoted_err > 0 {
        out.push_str(&format!("\n⚠️ {} 条升级失败（可能并发改了状态）", promoted_err));
    }
    out.push_str("\n用 /tasks 看更新后清单 / /pri <title> <N> 单条精调。");
    out
}

/// `/touch_all_p7` 命令回复文案。pure：与 format_promote_all_p7_reply
/// 同 4 态模板但语义不同 — touch 仅刷 updated_at（让"挂着没动的高
/// 优"重新冒头 proactive 选单），不改 priority。`targets_before` 是
/// 处理前候选数（active + priority ≥ 7）；`touched_ok` + `touched_err`
/// 是执行后计数。calling code 负责 walk 候选 + 调 task_touch_inner。
pub fn format_touch_all_p7_reply(
    confirmed: bool,
    targets_before: u32,
    touched_ok: u32,
    touched_err: u32,
) -> String {
    if !confirmed {
        if targets_before == 0 {
            return "✨ /touch_all_p7 confirm\n\n本聊天暂无 P7+ active task（pending / error 任务都 < P7，或全是 done / cancelled）。".to_string();
        }
        return format!(
            "✨ /touch_all_p7 confirm\n\n本聊天有 {} 条 P7+ active task（priority ≥ 7）可批量 touch。\n**这是批量修改 — 必须带 `confirm` token 才执行**：\n\n  /touch_all_p7 confirm\n\n语义：批量刷 updated_at 不改内容 — 让挂着没动的高优 task 重新冒头 proactive 选单。与 /promote_all_p7 互补（那个升 priority；本命令仅刷时间序）。",
            targets_before
        );
    }
    if touched_ok == 0 && touched_err == 0 {
        return "✨ 本聊天暂无 P7+ active task ✨（无可 touch 候选）".to_string();
    }
    let mut out = format!(
        "✨ 已批量 touch {} 条 P7+ active task — updated_at 已刷新，挂着的高优重新冒头",
        touched_ok
    );
    if touched_err > 0 {
        out.push_str(&format!(
            "\n⚠️ {} 条 touch 失败（可能并发改了状态）",
            touched_err
        ));
    }
    out.push_str("\n用 /tasks 看更新后顺序 / /oldest_n 看堆积最久的活。");
    out
}

/// `/pin_all_p7` 命令回复文案。pure：与 format_touch_all_p7_reply /
/// format_promote_all_p7_reply 同 4 态模板。语义：批量给 P7+ active
/// task 加 `[pinned]` marker — sprint 收尾「把高优清单全钉住」。
/// `targets_before` 是处理前候选数（active + priority ≥ 7 + 未 [pinned]）；
/// `pinned_ok` + `pinned_err` 是执行后计数。calling code 负责 walk
/// 候选 + 调 task_set_pinned。
pub fn format_pin_all_p7_reply(
    confirmed: bool,
    targets_before: u32,
    pinned_ok: u32,
    pinned_err: u32,
) -> String {
    if !confirmed {
        if targets_before == 0 {
            return "📌 /pin_all_p7 confirm\n\n本聊天暂无可 pin 的 P7+ active task（priority < 7 或已全部 [pinned]，或全是 done / cancelled）。".to_string();
        }
        return format!(
            "📌 /pin_all_p7 confirm\n\n本聊天有 {} 条 P7+ active task 可批量 pin（priority ≥ 7 且未 [pinned]）。\n**这是批量修改 — 必须带 `confirm` token 才执行**：\n\n  /pin_all_p7 confirm\n\n语义：批量加 [pinned] marker — sprint 收尾「把高优清单全钉住」。与 /touch_all_p7（刷 updated_at）/ /promote_all_p7（升 priority）组成 P7+ 批量族。",
            targets_before
        );
    }
    if pinned_ok == 0 && pinned_err == 0 {
        return "📌 本聊天暂无可 pin 的 P7+ active task ✨（全已 [pinned] 或全 < P7）".to_string();
    }
    let mut out = format!(
        "📌 已批量 pin {} 条 P7+ active task — [pinned] marker 已写入，高优清单已全部钉住",
        pinned_ok
    );
    if pinned_err > 0 {
        out.push_str(&format!(
            "\n⚠️ {} 条 pin 失败（可能并发改了状态）",
            pinned_err
        ));
    }
    out.push_str("\n用 /pinned 看本 chat 已钉清单 / /tasks 看全状态视图。");
    out
}

/// `/consolidate_now` 命令回复文案。pure：caller 在 confirmed 路径已
/// `trigger_consolidate(app).await` 拿到 Result<String, String> 传入；
/// 本函数仅做字符串拼装。3 态：
/// - !confirmed → usage hint 含「LLM-heavy + confirm 模板」warning
/// - confirmed + Ok(summary) → 「🧹 Consolidation finished · summary」
/// - confirmed + Err(reason) → 「🧹 Consolidate 失败：reason」（含 cancel
///   / config 错误等具体原因）
pub fn format_consolidate_now_reply(
    confirmed: bool,
    result: Option<Result<String, String>>,
) -> String {
    if !confirmed {
        return "🧹 /consolidate_now confirm\n\nTG 端手动触发一次 consolidate sweep（与桌面「立即整理」同后端）。**LLM-heavy + 烧 token + ~30s..2min 执行**，必须带 `confirm` token 防误触。\n\n用法：/consolidate_now confirm\n\n场景：sprint 整理 / 调 prompt 后想立即 audit consolidate 行为而不等 cron。常态走 cron（默认 6h interval）— 桌面 PanelDebug「⏰ 下次 consolidate」chip 显 ETA。".to_string();
    }
    match result {
        Some(Ok(summary)) => format!("🧹 {}", summary),
        Some(Err(reason)) => {
            if reason.contains("用户取消") {
                "🧹 已取消整理（已完成步骤保留）".to_string()
            } else {
                format!("🧹 Consolidate 失败：{}", reason)
            }
        }
        None => "🧹 未执行（confirmed=true 但 caller 没传 result — 这不该发生）".to_string(),
    }
}

/// `/demote <title>` 命令回复文案。pure：与 format_promote_reply 对偶 —
/// 边界态 old == 0（已是 P0）友好 no-op；其它态显 P<old> → P<new>。
pub fn format_demote_reply(
    title: &str,
    old_priority: Option<u8>,
    save_ok: Result<(), &str>,
) -> String {
    let t = title.trim();
    if t.is_empty() {
        return "🎯 用法：/demote <title>\n\n把任务 priority 降 -1（clamp 0）— 与 /promote 对偶，「这条不那么急了」一键降。保留 due / body / 其它 markers 不动。\n\n例：/demote 整理 Downloads\n例：/demote 1   （/tasks 输出第 1 条）\n\n相关：/pri <title> <N>（绝对设值）；/promote（+1 反方向）。".to_string();
    }
    let Some(old) = old_priority else {
        return match save_ok {
            Ok(()) => format!("🎯 已降「{}」", t),
            Err(e) => format!("🎯 降 priority 失败：{}", e),
        };
    };
    if old == 0 {
        return format!("🎯 「{}」已是 P0（最低）— 不再降", t);
    }
    let new_pri = old - 1;
    match save_ok {
        Ok(()) => format!("🎯 已降「{}」P{} → P{}", t, old, new_pri),
        Err(e) => format!("🎯 降 priority 失败：{}", e),
    }
}

/// `/promote <title>` 命令回复文案。pure：caller 已算好 new_priority
/// (clamp 9)，本 helper 仅展示结果 + 边界态。
/// - title 空 → usage hint
/// - old == 9 → "已是 P9（最高）" no-op 友好文案
/// - Ok(()) → "🎯 已升「title」P<old> → P<new>"
/// - Err(msg) → "🎯 升 priority 失败：<msg>"
pub fn format_promote_reply(
    title: &str,
    old_priority: Option<u8>,
    save_ok: Result<(), &str>,
) -> String {
    let t = title.trim();
    if t.is_empty() {
        return "🎯 用法：/promote <title>\n\n把任务 priority 升 +1（clamp 9）— 一步操作不必算具体 P 值（与 /pri 绝对值互补）。保留 due / body / 其它 markers 不动。\n\n例：/promote 整理 Downloads\n例：/promote 1   （/tasks 输出第 1 条）\n\n相关：/pri <title> <N>（绝对设值）；/demote（-1 反方向）。".to_string();
    }
    let Some(old) = old_priority else {
        // 无法读到 priority — caller path 错误（resolve 成功但 view 查不到）
        return match save_ok {
            Ok(()) => format!("🎯 已升「{}」", t),
            Err(e) => format!("🎯 升 priority 失败：{}", e),
        };
    };
    if old >= 9 {
        return format!("🎯 「{}」已是 P9（最高）— 不再升", t);
    }
    let new_pri = old + 1;
    match save_ok {
        Ok(()) => format!("🎯 已升「{}」P{} → P{}", t, old, new_pri),
        Err(e) => format!("🎯 升 priority 失败：{}", e),
    }
}

/// `/feedback <text>` 命令回复文案。pure：
/// - 空 text → usage hint
/// - 写盘成功 → "💬 已记到 feedback_history" + preview
/// - 写盘失败 → caller 应直接显错误（本 helper 不分支 — feedback_history
///   record_event 是 best-effort 不返 Result）
pub const FEEDBACK_PREVIEW_CHARS: usize = 60;
pub fn format_feedback_reply(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "💬 用法：/feedback <text>\n\n给 pet 留反馈到 feedback_history.log — LLM 在下次 proactive cycle 看到 owner 原话调整行为。正向 / 负向 / 中性建议都可走此入口。\n\n例：/feedback 你最近说话太啰嗦，请精炼点\n例：/feedback 这次主动选 task 选得很到位\n例：/feedback 周末别那么主动开口，让我休息\n\n对比 /note（杂项 → general）/ /reflect（反思 → ai_insights）：本命令直接影响 pet 行为，是与 pet 沟通调整的快速通道。".to_string();
    }
    let preview: String = if trimmed.chars().count() > FEEDBACK_PREVIEW_CHARS {
        let head: String = trimmed.chars().take(FEEDBACK_PREVIEW_CHARS).collect();
        format!("{}…", head)
    } else {
        trimmed.to_string()
    };
    format!(
        "💬 已记到 feedback_history\n\n{}\n\n（pet 在下次主动开口前会读到这条 feedback 调整行为。）",
        preview
    )
}

/// `/transient <text> [minutes]` 命令回复文案。pure：
/// - text 空 → usage hint（含示例 + 与 /note / /mute 区别说明）
/// - 正常 set → "📝 已设 transient_note：「<preview>」N 分钟有效（到 HH:MM 自动清除）"
/// - until 缺失（极少见，set_transient_note 内部异常）→ 简化 reply
pub const TRANSIENT_PREVIEW_CHARS: usize = 60;
pub fn format_transient_reply(
    text: &str,
    minutes: i64,
    until_local: Option<chrono::DateTime<chrono::Local>>,
) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "📝 用法：/transient <text> [minutes]\n\n写一条 N 分钟有效的临时指示给宠物 — 不存盘，只挂当前 in-memory，到时自动清除。pet 开口时会读到这条 [临时指示] 调整语气 / 选择。minutes 缺省 60；上限 10080（7 天）。\n\n例：/transient 在开会，半小时别打扰我 30\n例：/transient 集中写文档不要主动开口 90\n例：/transient 今晚 9 点后再 ping 我 240\n例：/transient 心情不好别活泼  （默认 60 分钟）\n\n对比 /note（杂项 → general memory 存盘）/ /reflect（反思 → ai_insights 存盘）/ /feedback（写 feedback_history 改 pet 行为）/ /mute（直接静音不开口）：本命令是「给 pet 临时上下文，但不阻塞它说话」的快速通道。".to_string();
    }
    let preview: String = if trimmed.chars().count() > TRANSIENT_PREVIEW_CHARS {
        let head: String = trimmed.chars().take(TRANSIENT_PREVIEW_CHARS).collect();
        format!("{}…", head)
    } else {
        trimmed.to_string()
    };
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
    match until_local {
        Some(t) => format!(
            "📝 已设 transient_note（{} 有效）\n\n{}\n\n到 {} 自动清除。pet 开口时会读到这条 [临时指示]。",
            nice,
            preview,
            t.format("%H:%M")
        ),
        None => format!(
            "📝 已设 transient_note（{} 有效）\n\n{}\n\npet 开口时会读到这条 [临时指示]。",
            nice, preview
        ),
    }
}

/// `/pri <title> <N>` 命令回复文案。pure：
/// - title 空 → usage hint
/// - priority None（解析失败 / 缺失）→ usage hint with examples
/// - resolved_title = Err → format_command_error (caller 路径)
/// - save_ok = Ok(()) → "🎯 已设「<title>」P<N>"
/// - save_ok = Err(msg) → "🎯 改 priority 失败：<msg>"
pub fn format_pri_reply(
    title: &str,
    priority: Option<u8>,
    save_ok: Result<(), &str>,
) -> String {
    let t = title.trim();
    if t.is_empty() || priority.is_none() {
        return "🎯 用法：/pri <title> <N>\n\n单改任务 priority（0..=9）— 不走 /edit 全量覆写，保留所有其它 markers。N 必须是 0-9 整数。title 含空格 / 中文标点也保（取末 token 当 N）。\n\n例：/pri 整理 Downloads 5\n例：/pri 写周报 7\n例：/pri 跑步 0".to_string();
    }
    let n = priority.unwrap();
    match save_ok {
        Ok(()) => format!("🎯 已设「{}」P{}", t, n),
        Err(e) => format!("🎯 改 priority 失败：{}", e),
    }
}

/// `/swap_priority <a> :: <b>` 命令回复文案。pure。
///
/// 入参（caller resolved titles after fuzzy match）+ pre-swap priorities
/// + save 结果。状态机：
/// - 任一 title trim 后空 → usage hint（含 `::` separator 示例）
/// - title_a == title_b → 「同一条 task 无需互换」兜底
/// - pre_a / pre_b 任一 None → 「task 不存在」错误（caller resolve 失败）
/// - swap_a / swap_b 任一 Err → 「互换部分失败」warning + 哪条失败
/// - 全 ok → 「🔄 已互换 P<a> ↔ P<b>」
///
/// 注：caller 先读两 task 的 pre-swap priority，再两次调 task_set_priority
/// 把 a → pre_b、b → pre_a。formatter 只组装文本。
pub fn format_swap_priority_reply(
    title_a: &str,
    title_b: &str,
    pre_a: Option<u8>,
    pre_b: Option<u8>,
    save_a: Result<(), &str>,
    save_b: Result<(), &str>,
) -> String {
    let ta = title_a.trim();
    let tb = title_b.trim();
    if ta.is_empty() || tb.is_empty() {
        return "🔄 用法：/swap_priority <title_a> :: <title_b>\n\n互换两 task 的 priority（sprint 重组场景 — 不必算具体 P 值）。`::` 是必填 separator，让 title 含空格 / 中文标点也能精确切。\n\n例：/swap_priority 整理 Downloads :: 写周报\n例：/swap_priority 1 :: 3   （/tasks 输出第 1 与第 3 条互换）\n\nTitle resolve 与 /done / /cancel 同三层（数字 index → fuzzy → 错误候选）。复用 task_set_priority 后端，对称写两次（保留 due / body / markers 不动）。".to_string();
    }
    if ta == tb {
        return format!("🔄 同一条 task 「{}」无需互换 priority。", ta);
    }
    let (Some(a_val), Some(b_val)) = (pre_a, pre_b) else {
        let missing = match (pre_a, pre_b) {
            (None, None) => format!("「{}」与「{}」", ta, tb),
            (None, _) => format!("「{}」", ta),
            (_, None) => format!("「{}」", tb),
            _ => unreachable!(),
        };
        return format!("🔄 互换失败：{} 没找到（resolve 后任务不存在 — 可能已被删 / rename）", missing);
    };
    let a_ok = save_a.is_ok();
    let b_ok = save_b.is_ok();
    if a_ok && b_ok {
        return format!(
            "🔄 已互换 priority：「{}」P{} → P{} · 「{}」P{} → P{}",
            ta, a_val, b_val, tb, b_val, a_val
        );
    }
    // 部分失败：清晰列哪条出问题
    let mut out = String::new();
    out.push_str("🔄 互换部分失败");
    if let Err(e) = save_a {
        out.push_str(&format!("\n⚠️ 「{}」改 P 失败：{}", ta, e));
    } else {
        out.push_str(&format!("\n✓ 「{}」P{} → P{}", ta, a_val, b_val));
    }
    if let Err(e) = save_b {
        out.push_str(&format!("\n⚠️ 「{}」改 P 失败：{}", tb, e));
    } else {
        out.push_str(&format!("\n✓ 「{}」P{} → P{}", tb, b_val, a_val));
    }
    out
}

/// `/edit_due <title> <preset>` 命令回复文案。pure。
///
/// 入参：
/// - title trim 后的字符串（caller resolve 后传 actual title）
/// - preset 解析结果（None = 不识别）
/// - computed: caller 调 compute_edit_due_preset 拿到的最终 NaiveDateTime
///   （Some = 设 due，None = clear 语义）。仅 preset 有效时才传 valid 值。
/// - save_ok: task_set_due 调用结果
///
/// 输出 4 种态：
/// - 空 title / preset=None → usage hint（含 preset 名单 + 示例）
/// - save Err → "📅 设 due 失败：<msg>"
/// - preset=Clear / computed=None → "📅 已清「title」的 due"
/// - preset=有效时刻 → "📅 已设「title」due → MM-DD HH:MM"
pub fn format_edit_due_reply(
    title: &str,
    preset: Option<&EditDuePreset>,
    computed: Option<chrono::NaiveDateTime>,
    save_ok: Result<(), &str>,
) -> String {
    let t = title.trim();
    if t.is_empty() || preset.is_none() {
        return "📅 用法：/edit_due <title> <preset>\n\n免手敲 ISO 日期改任务 due。preset 接友好词：\n\n时刻类：\n  · tonight / 今晚 — 今晚 18:00（已过则明晚）\n  · tomorrow / 明天 / morning / 早上 / tmr — 明早 09:00\n  · monday..sunday / 周一..周日 — 本周（或下周如已过）该 weekday 09:00\n  · next_monday..next_sunday / 下周一..下周日 — 下周 weekday 09:00\n\n相对类：\n  · +30m / +2h / +1d — now + 时长（+Nd 落次日 09:00）\n\n清除：\n  · clear / none / 0 / 清除 — 清掉 due\n\n例：\n  /edit_due 整理 Downloads tonight\n  /edit_due 写周报 next_friday\n  /edit_due 跑步 +30m\n  /edit_due 旧任务 clear\n\nTitle resolve 与 /done / /cancel 同三层（数字 index → fuzzy → 错误候选）。".to_string();
    }
    if let Err(e) = save_ok {
        return format!("📅 设 due 失败：{}", e);
    }
    match computed {
        Some(dt) => format!(
            "📅 已设「{}」due → {}",
            t,
            dt.format("%m-%d %H:%M")
        ),
        None => format!("📅 已清「{}」的 due", t),
    }
}

/// pure：从 views 抽出 done 任务的 updated_at 当日 NaiveDate 集合。
/// `updated_at` 走 RFC3339 + 截前 10 字符（YYYY-MM-DD）NaiveDate parse；
/// 解析失败的条目静默跳过（防御 legacy 数据格式不一致）。
pub fn done_dates_from_views(
    views: &[crate::task_queue::TaskView],
) -> std::collections::HashSet<chrono::NaiveDate> {
    use crate::task_queue::TaskStatus;
    let mut set = std::collections::HashSet::new();
    for v in views {
        if !matches!(v.status, TaskStatus::Done) {
            continue;
        }
        if v.updated_at.len() < 10 {
            continue;
        }
        if let Ok(d) = chrono::NaiveDate::parse_from_str(&v.updated_at[..10], "%Y-%m-%d") {
            set.insert(d);
        }
    }
    set
}

/// pure：算 streak (连续 done 天数 ending at today or yesterday)。空集合
/// → 0；今日有 done → 从今日往前数；否则若昨日有 → 从昨日往前数；都
/// 无 → 0。
pub fn compute_done_streak(
    done_dates: &std::collections::HashSet<chrono::NaiveDate>,
    today: chrono::NaiveDate,
) -> u32 {
    if done_dates.is_empty() {
        return 0;
    }
    let mut anchor = if done_dates.contains(&today) {
        today
    } else if done_dates.contains(&(today - chrono::Duration::days(1))) {
        today - chrono::Duration::days(1)
    } else {
        return 0;
    };
    let mut count: u32 = 1;
    loop {
        let prev = anchor - chrono::Duration::days(1);
        if done_dates.contains(&prev) {
            count += 1;
            anchor = prev;
        } else {
            break;
        }
    }
    count
}

/// pure：算 [today - (days-1), today] 范围内 done 条数（counts task
/// instances, not unique days）。`days` 通常 7 或 30。
pub fn count_done_in_window(
    views: &[crate::task_queue::TaskView],
    today: chrono::NaiveDate,
    days: u32,
) -> u32 {
    use crate::task_queue::TaskStatus;
    if days == 0 {
        return 0;
    }
    let start = today - chrono::Duration::days((days - 1) as i64);
    let mut n: u32 = 0;
    for v in views {
        if !matches!(v.status, TaskStatus::Done) {
            continue;
        }
        if v.updated_at.len() < 10 {
            continue;
        }
        let Ok(d) = chrono::NaiveDate::parse_from_str(&v.updated_at[..10], "%Y-%m-%d") else {
            continue;
        };
        if d >= start && d <= today {
            n += 1;
        }
    }
    n
}

/// `/streak` 命令回复文案。pure：connect 三个 pure helpers + 友好 emoji
/// 包装。caller 注入 `today` 让单测稳定。
pub fn format_streak_reply(
    views: &[crate::task_queue::TaskView],
    today: chrono::NaiveDate,
) -> String {
    let done_dates = done_dates_from_views(views);
    let streak = compute_done_streak(&done_dates, today);
    let week = count_done_in_window(views, today, 7);
    let month = count_done_in_window(views, today, 30);
    let streak_line = if streak == 0 {
        "🌱 streak 中断 — 今日 / 昨日均无完成".to_string()
    } else {
        format!("🔥 连续 {} 天有完成", streak)
    };
    format!(
        "{}\n📊 近 7 天 done：{} 条 · 近 30 天 done：{} 条",
        streak_line, week, month
    )
}

/// `/yesterday` 命令回复文案。pure：filter Done + updated_at 在 `today
/// - 1 day` 当日的任务。按 updated_at 倒序排（最新完成在前），列标题
/// + `[result:]` 摘要（若有）。空 → 友好兜底。
/// caller 传 `today` (NaiveDate)：formatter 内部 `today - 1 day` 算昨
/// 日 boundary，便于单测稳定。
pub fn format_yesterday_reply(
    views: &[crate::task_queue::TaskView],
    today: chrono::NaiveDate,
) -> String {
    use crate::task_queue::TaskStatus;
    let yesterday = today - chrono::Duration::days(1);
    let y_str = yesterday.format("%Y-%m-%d").to_string();
    let mut done: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| matches!(v.status, TaskStatus::Done))
        .filter(|v| v.updated_at.starts_with(&y_str))
        .collect();
    if done.is_empty() {
        return format!(
            "📅 昨日（{}）无完成记录。\n用 /recent 看更早完成 / /today 看今日。",
            y_str
        );
    }
    // updated_at ISO 字典序 = 时间序，最新在前
    done.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let mut out = format!("📅 昨日（{}）完成 {} 条：", y_str, done.len());
    for v in &done {
        out.push_str(&format!("\n· ✅ {}", v.title));
        if let Some(r) = &v.result {
            let r_trim = r.trim();
            if !r_trim.is_empty() {
                // result 摘要截 40 char 保 reply 紧凑
                let preview: String = if r_trim.chars().count() > 40 {
                    let head: String = r_trim.chars().take(40).collect();
                    format!("{}…", head)
                } else {
                    r_trim.to_string()
                };
                out.push_str(&format!(" — {}", preview));
            }
        }
    }
    out
}

/// `/today_done` 命令回复文案。pure：filter Done + updated_at 在 `today`
/// 当日的任务。按 updated_at 倒序排（最新完成在前），列标题 + `[result:]`
/// 摘要（若有）。空 → 友好兜底，建议 `/today` 看 due 段。
///
/// 与 `format_yesterday_reply` 同模板但 scope 是今日 — 实现独立保持
/// 两 fn 各自单测点稳定（不抽 generic boundary day 函数避免 owner
/// 看到混合错文案）。
pub fn format_today_done_reply(
    views: &[crate::task_queue::TaskView],
    today: chrono::NaiveDate,
) -> String {
    use crate::task_queue::TaskStatus;
    let t_str = today.format("%Y-%m-%d").to_string();
    let mut done: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| matches!(v.status, TaskStatus::Done))
        .filter(|v| v.updated_at.starts_with(&t_str))
        .collect();
    if done.is_empty() {
        return format!(
            "📅 今日（{}）暂无完成记录。\n用 /today 看今日 due / /yesterday 看昨日产出。",
            t_str
        );
    }
    // updated_at ISO 字典序 = 时间序，最新在前
    done.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let mut out = format!("📅 今日（{}）完成 {} 条：", t_str, done.len());
    for v in &done {
        out.push_str(&format!("\n· ✅ {}", v.title));
        if let Some(r) = &v.result {
            let r_trim = r.trim();
            if !r_trim.is_empty() {
                // result 摘要截 40 char 保 reply 紧凑（与 /yesterday 同 cap）
                let preview: String = if r_trim.chars().count() > 40 {
                    let head: String = r_trim.chars().take(40).collect();
                    format!("{}…", head)
                } else {
                    r_trim.to_string()
                };
                out.push_str(&format!(" — {}", preview));
            }
        }
    }
    out
}

/// `/edit_title` 命令成功回复文案。pure：
/// - `✏️ 已改标题：「<old>」→「<new>」`
/// - new_title 可能含 unique-filename suffix（memory_rename 内置兜底
///   `_N`）— 透显 caller 传入的 new_title（已含 suffix）
///
/// 同 src/new title 情况由 caller / memory_rename 拦截（"No change."），
/// 不在 formatter 里特判。
pub fn format_edit_title_reply(old_title: &str, new_title: &str) -> String {
    format!(
        "✏️ 已改标题：「{}」→「{}」",
        old_title.trim(),
        new_title.trim(),
    )
}

/// `/mute_today` 命令回复文案。pure：单行 `🌙 已静音到本地午夜
/// （00:00）— 还 N 分钟（M 小时）`。caller 已算好 minutes（now → 次日
/// 午夜的分钟数）。clamp 1..=1440 由 caller 保证；本 formatter 透显。
pub fn format_mute_today_reply(minutes: i64) -> String {
    let hours = minutes as f64 / 60.0;
    if minutes >= 60 {
        format!(
            "🌙 已静音 proactive 到本地午夜（00:00）— 还 {} 分钟（{:.1} 小时）",
            minutes, hours,
        )
    } else {
        format!(
            "🌙 已静音 proactive 到本地午夜（00:00）— 还 {} 分钟",
            minutes,
        )
    }
}

/// `/cascade_rename` 命令成功回复文案。pure：
/// - 头行 `🔁 已改标题：「<old>」→「<new>」`
/// - 注脚一行：cascade 命中数（0 时友好提示「无 detail.md 需要更新」）
pub fn format_cascade_rename_reply(
    old_title: &str,
    new_title: &str,
    updated_md_count: usize,
) -> String {
    let mut out = format!(
        "🔁 已改标题：「{}」→「{}」",
        old_title.trim(),
        new_title.trim(),
    );
    if updated_md_count == 0 {
        out.push_str("\n· 无 detail.md 需要更新（未找到 ref token 引用）");
    } else {
        out.push_str(&format!(
            "\n· 同步 {} 份 detail.md 内的 ref token",
            updated_md_count,
        ));
    }
    out
}

/// `/touched_today` 命令回复文案。pure：filter views by updated_at 起始
/// 匹配 `today` 日期前缀，按 updated_at 倒序排（最新动作在前），列状态
/// emoji + HH:MM + title + 可选 result preview（done task 时）。
///
/// 与 `format_today_done_reply` 的区别：本命令不限 status — 任意状态
/// 只要 updated_at 命中今日就显，让 owner 看到「动过但没完成」的 task
/// （pending after touch / snooze / pin 等 owner action 引发的 update）
/// + 完成 + 失败全谱。snooze 状态用 💤 单独 emoji 区别于 ⏳ 普通 pending。
pub fn format_touched_today_reply(
    views: &[crate::task_queue::TaskView],
    today: chrono::NaiveDate,
) -> String {
    use crate::task_queue::TaskStatus;
    let t_str = today.format("%Y-%m-%d").to_string();
    let mut touched: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| v.updated_at.starts_with(&t_str))
        .collect();
    if touched.is_empty() {
        return format!(
            "📅 今日（{}）暂无动过的 task。\n用 /today 看今日 due / /today_done 看今日完成 / /tasks 看全清单。",
            t_str
        );
    }
    // updated_at ISO 字典序 = 时间序，最新在前
    touched.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let mut out = format!(
        "📅 今日（{}）动过 {} 条（按时间倒序）：",
        t_str,
        touched.len(),
    );
    for v in &touched {
        // updated_at 切 HH:MM。ISO 形如 `YYYY-MM-DDTHH:MM:SS.fff+08:00`
        // 或 `YYYY-MM-DDTHH:MM:SS` — 取 11..16 索引切到分钟。极短串兜底。
        let hm = if v.updated_at.len() >= 16 {
            &v.updated_at[11..16]
        } else {
            ""
        };
        let emoji = match v.status {
            TaskStatus::Done => "✅",
            TaskStatus::Error => "⚠️",
            TaskStatus::Cancelled => "🚫",
            // pending 含 [snooze:] marker 用 💤 与 ⏳ 区分 — owner 一眼看
            // "今天被推后" vs "今天还活着"
            TaskStatus::Pending => {
                if v.raw_description.contains("[snooze:") {
                    "💤"
                } else {
                    "⏳"
                }
            }
        };
        if hm.is_empty() {
            out.push_str(&format!("\n· {} {}", emoji, v.title));
        } else {
            out.push_str(&format!("\n· {} {} {}", emoji, hm, v.title));
        }
        // done 状态时附 result preview（与 /today_done / /yesterday 同 40 字 cap）
        if matches!(v.status, TaskStatus::Done) {
            if let Some(r) = &v.result {
                let r_trim = r.trim();
                if !r_trim.is_empty() {
                    let preview: String = if r_trim.chars().count() > 40 {
                        let head: String = r_trim.chars().take(40).collect();
                        format!("{}…", head)
                    } else {
                        r_trim.to_string()
                    };
                    out.push_str(&format!(" — {}", preview));
                }
            }
        }
    }
    out
}

/// `/search_today <keyword>` 命令回复文案。pure：与 `format_find_reply`
/// 同模板（title / raw_description case-insensitive substring 命中，
/// pending/error 浮顶，10 条 cap），但额外限定 updated_at 起始匹配
/// `today` 日期前缀。
///
/// 三件套定位：
/// - `/find` — 不限日期，全量 fuzzy
/// - `/touched_today` — 今日全谱，无 kw
/// - 本命令 — 今日 + kw 交集（更窄）
///
/// caller 已传 views（chat-scoped）+ today 日期。空 keyword → missing-
/// argument 反馈。
pub const SEARCH_TODAY_MAX_HITS: usize = 10;
pub fn format_search_today_reply(
    views: &[crate::task_queue::TaskView],
    today: chrono::NaiveDate,
    keyword: &str,
) -> String {
    use crate::task_queue::TaskStatus;
    let kw = keyword.trim();
    if kw.is_empty() {
        return "🔎 用法：/search_today <keyword>\n限定今日 updated_at 的 task 内 fuzzy 搜 title / description（不分大小写，至多 10 条）。\n例：/search_today API / /search_today 周报\n\n相关：/find（全量）；/touched_today（今日全谱）。".to_string();
    }
    let t_str = today.format("%Y-%m-%d").to_string();
    let kw_lower = kw.to_lowercase();
    let mut hits: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| v.updated_at.starts_with(&t_str))
        .filter(|v| {
            v.title.to_lowercase().contains(&kw_lower)
                || v.raw_description.to_lowercase().contains(&kw_lower)
        })
        .collect();
    // pending / error 浮顶（与 /find 同 status_rank）
    let status_rank = |s: &TaskStatus| match s {
        TaskStatus::Pending => 0u8,
        TaskStatus::Error => 1,
        TaskStatus::Done => 2,
        TaskStatus::Cancelled => 3,
    };
    hits.sort_by_key(|v| status_rank(&v.status));
    if hits.is_empty() {
        return format!(
            "🔎 今日（{}）无任务命中「{}」（搜了标题 + description 子串）。\n试 /find 看全量历史 / /touched_today 看今日全谱。",
            t_str, kw,
        );
    }
    let cap = SEARCH_TODAY_MAX_HITS;
    let shown = &hits[..hits.len().min(cap)];
    let mut out = format!(
        "🔎 今日（{}）命中「{}」{} 条：",
        t_str,
        kw,
        hits.len(),
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
            hits.len() - cap,
        ));
    }
    out
}

/// `/touched_thisweek` 命令回复文案。pure：filter views by updated_at
/// 起始 >= `week_start` 日期前缀（caller 传周一 00:00 起的日期），按
/// updated_at 倒序排，列状态 emoji + MM-DD HH:MM + title + 可选
/// result preview。
///
/// 与 today/yesterday 切片 fn 区别：跨日 scope 需 MM-DD HH:MM（仅 HH:MM
/// 看不出哪天）；header 显「本周（YYYY-MM-DD 起）」让 owner 一眼看
/// 周一日期。
pub fn format_touched_thisweek_reply(
    views: &[crate::task_queue::TaskView],
    week_start: chrono::NaiveDate,
) -> String {
    use crate::task_queue::TaskStatus;
    let week_start_str = week_start.format("%Y-%m-%d").to_string();
    let mut touched: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| {
            // ISO 日期字典序 = 时间序 — `updated_at >= week_start_str` 即
            // 命中本周。比对前 10 字符（YYYY-MM-DD）足够，避免 tz tail 干扰。
            v.updated_at.len() >= 10 && &v.updated_at[..10] >= week_start_str.as_str()
        })
        .collect();
    if touched.is_empty() {
        return format!(
            "📅 本周（{} 起）暂无动过的 task。\n用 /touched_today 看今日 / /tasks 看全清单 / /yesterday 看昨日完成。",
            week_start_str
        );
    }
    touched.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let mut out = format!(
        "📅 本周（{} 起）动过 {} 条（按时间倒序）：",
        week_start_str,
        touched.len(),
    );
    for v in &touched {
        // 跨日 scope — 取 MM-DD HH:MM（前 16 字符 - 5 取「MM-DD」+ 11..16 取「HH:MM」）
        let date_time = if v.updated_at.len() >= 16 {
            format!("{} {}", &v.updated_at[5..10], &v.updated_at[11..16])
        } else {
            String::new()
        };
        let emoji = match v.status {
            TaskStatus::Done => "✅",
            TaskStatus::Error => "⚠️",
            TaskStatus::Cancelled => "🚫",
            TaskStatus::Pending => {
                if v.raw_description.contains("[snooze:") {
                    "💤"
                } else {
                    "⏳"
                }
            }
        };
        if date_time.is_empty() {
            out.push_str(&format!("\n· {} {}", emoji, v.title));
        } else {
            out.push_str(&format!("\n· {} {} {}", emoji, date_time, v.title));
        }
        if matches!(v.status, TaskStatus::Done) {
            if let Some(r) = &v.result {
                let r_trim = r.trim();
                if !r_trim.is_empty() {
                    let preview: String = if r_trim.chars().count() > 40 {
                        let head: String = r_trim.chars().take(40).collect();
                        format!("{}…", head)
                    } else {
                        r_trim.to_string()
                    };
                    out.push_str(&format!(" — {}", preview));
                }
            }
        }
    }
    out
}

/// `/search_yesterday <keyword>` 命令回复文案。pure。clone of
/// `format_search_today_reply` 结构（filter / status rank / cap / emoji
/// 完全一致），仅 scope 是 yesterday 日期 + 标题用「昨日」+ 空集兜底
/// alt 入口指向 /find / /touched_yesterday（避免循环建议 today）。
pub fn format_search_yesterday_reply(
    views: &[crate::task_queue::TaskView],
    yesterday: chrono::NaiveDate,
    keyword: &str,
) -> String {
    use crate::task_queue::TaskStatus;
    let kw = keyword.trim();
    if kw.is_empty() {
        return "🔎 用法：/search_yesterday <keyword>\n限定昨日 updated_at 的 task 内 fuzzy 搜 title / description（不分大小写，至多 10 条）。\n例：/search_yesterday API / /search_yesterday 周报\n\n相关：/search_today（今日同模板）；/find（全量不限日期）；/touched_yesterday（昨日全谱）。".to_string();
    }
    let t_str = yesterday.format("%Y-%m-%d").to_string();
    let kw_lower = kw.to_lowercase();
    let mut hits: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| v.updated_at.starts_with(&t_str))
        .filter(|v| {
            v.title.to_lowercase().contains(&kw_lower)
                || v.raw_description.to_lowercase().contains(&kw_lower)
        })
        .collect();
    let status_rank = |s: &TaskStatus| match s {
        TaskStatus::Pending => 0u8,
        TaskStatus::Error => 1,
        TaskStatus::Done => 2,
        TaskStatus::Cancelled => 3,
    };
    hits.sort_by_key(|v| status_rank(&v.status));
    if hits.is_empty() {
        return format!(
            "🔎 昨日（{}）无任务命中「{}」（搜了标题 + description 子串）。\n试 /find 看全量历史 / /touched_yesterday 看昨日全谱。",
            t_str, kw,
        );
    }
    let cap = SEARCH_TODAY_MAX_HITS;
    let shown = &hits[..hits.len().min(cap)];
    let mut out = format!(
        "🔎 昨日（{}）命中「{}」{} 条：",
        t_str,
        kw,
        hits.len(),
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
            hits.len() - cap,
        ));
    }
    out
}

/// `/search_thisweek <keyword>` 命令回复文案。pure。与 `format_search_
/// today_reply` 同结构（filter / status rank / cap / emoji 一致）但
/// 限定 updated_at >= week_start 日期前缀（ISO 字典序 = 时间序）。
///
/// 与 format_touched_thisweek_reply 同 week filter 算法。空集兜底教学
/// 指 /find（全量）/ /touched_thisweek（本周全谱）— 避免 self-loop。
pub fn format_search_thisweek_reply(
    views: &[crate::task_queue::TaskView],
    week_start: chrono::NaiveDate,
    keyword: &str,
) -> String {
    use crate::task_queue::TaskStatus;
    let kw = keyword.trim();
    if kw.is_empty() {
        return "🔎 用法：/search_thisweek <keyword>\n限定本周 updated_at（自周一 00:00 起）的 task 内 fuzzy 搜 title / description（不分大小写，至多 10 条）。\n例：/search_thisweek API / /search_thisweek 周报\n\n相关：/search_today（今日同模板）；/find（全量）；/touched_thisweek（本周全谱）。".to_string();
    }
    let week_start_str = week_start.format("%Y-%m-%d").to_string();
    let kw_lower = kw.to_lowercase();
    let mut hits: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| {
            v.updated_at.len() >= 10 && &v.updated_at[..10] >= week_start_str.as_str()
        })
        .filter(|v| {
            v.title.to_lowercase().contains(&kw_lower)
                || v.raw_description.to_lowercase().contains(&kw_lower)
        })
        .collect();
    let status_rank = |s: &TaskStatus| match s {
        TaskStatus::Pending => 0u8,
        TaskStatus::Error => 1,
        TaskStatus::Done => 2,
        TaskStatus::Cancelled => 3,
    };
    hits.sort_by_key(|v| status_rank(&v.status));
    if hits.is_empty() {
        return format!(
            "🔎 本周（{} 起）无任务命中「{}」（搜了标题 + description 子串）。\n试 /find 看全量历史 / /touched_thisweek 看本周全谱。",
            week_start_str, kw,
        );
    }
    let cap = SEARCH_TODAY_MAX_HITS;
    let shown = &hits[..hits.len().min(cap)];
    let mut out = format!(
        "🔎 本周（{} 起）命中「{}」{} 条：",
        week_start_str,
        kw,
        hits.len(),
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
            hits.len() - cap,
        ));
    }
    out
}

/// `/digest_yesterday <N>` 命令回复文案。pure：与 `format_digest_reply`
/// 同结构（done filter + result preview），但额外限定 updated_at 起始
/// 匹配 `yesterday` 日期前缀。caller 已 clamp n 1..=20。
///
/// 与 `format_yesterday_reply`（仅标题无 result）/ `format_digest_reply`
/// （不限日期）双重对偶 — 三者形成 yesterday × done × result-or-not 矩阵
/// 的完整覆盖。
pub fn format_digest_yesterday_reply(
    views: &[crate::task_queue::TaskView],
    yesterday: chrono::NaiveDate,
    n: u32,
) -> String {
    use crate::task_queue::TaskStatus;
    let t_str = yesterday.format("%Y-%m-%d").to_string();
    let mut done: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| matches!(v.status, TaskStatus::Done))
        .filter(|v| v.updated_at.starts_with(&t_str))
        .collect();
    if done.is_empty() {
        return format!(
            "📋 昨日（{}）暂无完成记录。\n用 /digest 看最近 done（不限日期）/ /yesterday 看仅标题视图 / /touched_yesterday 看昨日全谱。",
            t_str
        );
    }
    // updated_at desc — 最新完成在前（与 /digest 同方向）
    done.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let cap = (n as usize).max(1);
    let shown = &done[..done.len().min(cap)];
    let mut out = format!(
        "📋 昨日（{}）完成 {} 条（共 {}）：",
        t_str,
        shown.len(),
        done.len(),
    );
    for v in shown {
        // updated_at 截 HH:MM（昨日 date 已在 header，省 MM-DD 冗余）
        let hm = if v.updated_at.len() >= 16 {
            &v.updated_at[11..16]
        } else {
            ""
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
        if hm.is_empty() {
            out.push_str(&format!("\n· {}{}", v.title, result_part));
        } else {
            out.push_str(&format!("\n· {} · {}{}", hm, v.title, result_part));
        }
    }
    if done.len() > shown.len() {
        out.push_str(&format!(
            "\n…还有 {} 条更早完成（/digest_yesterday {} 看更多，上限 20）",
            done.len() - shown.len(),
            done.len().min(20),
        ));
    }
    out
}

/// `/digest_thisweek <N>` 命令回复文案。pure：与 `format_digest_yesterday_
/// reply` 同结构（done filter + result preview），但 scope 限本周 —
/// `updated_at >= week_start` ISO prefix 比较。
///
/// 跨日 scope → 行 MM-DD HH:MM（与 /digest 同；/digest_yesterday 单日
/// HH:MM）。空集兜底教学指 /digest / /touched_thisweek / /yesterday 三
/// alt 入口（avoid loop）。
pub fn format_digest_thisweek_reply(
    views: &[crate::task_queue::TaskView],
    week_start: chrono::NaiveDate,
    n: u32,
) -> String {
    use crate::task_queue::TaskStatus;
    let week_start_str = week_start.format("%Y-%m-%d").to_string();
    let mut done: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| matches!(v.status, TaskStatus::Done))
        .filter(|v| {
            v.updated_at.len() >= 10 && &v.updated_at[..10] >= week_start_str.as_str()
        })
        .collect();
    if done.is_empty() {
        return format!(
            "📋 本周（{} 起）暂无完成记录。\n用 /digest 看最近 done（不限日期）/ /touched_thisweek 看本周全谱 / /yesterday 看昨日完成。",
            week_start_str,
        );
    }
    // updated_at desc — 最新完成在前
    done.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let cap = (n as usize).max(1);
    let shown = &done[..done.len().min(cap)];
    let mut out = format!(
        "📋 本周（{} 起）完成 {} 条（共 {}）：",
        week_start_str,
        shown.len(),
        done.len(),
    );
    for v in shown {
        // 跨日 scope — MM-DD HH:MM（与 /digest 同；/digest_yesterday 是
        // HH:MM only 因 single-day）
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
            "\n…还有 {} 条更早完成（/digest_thisweek {} 看更多，上限 20）",
            done.len() - shown.len(),
            done.len().min(20),
        ));
    }
    out
}

/// `/touched_yesterday` 命令回复文案。pure。与 `format_touched_today_reply`
/// 同结构（filter / sort / emoji / preview 完全一致），仅 scope 是 `yesterday`
/// 日期 + 标题用「昨日」+ 空集兜底教学指向 /touched_today / /yesterday /
/// /tasks。
///
/// 与 `format_yesterday_reply` 的区别：那个仅 done，本命令不限 status —
/// 复盘视角看 owner 昨日全谱动作（含 pending 调整 / snooze / pin 等 owner
/// action）。
pub fn format_touched_yesterday_reply(
    views: &[crate::task_queue::TaskView],
    yesterday: chrono::NaiveDate,
) -> String {
    use crate::task_queue::TaskStatus;
    let t_str = yesterday.format("%Y-%m-%d").to_string();
    let mut touched: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| v.updated_at.starts_with(&t_str))
        .collect();
    if touched.is_empty() {
        return format!(
            "📅 昨日（{}）暂无动过的 task。\n用 /touched_today 看今日动作 / /yesterday 看昨日完成 / /tasks 看全清单。",
            t_str
        );
    }
    touched.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let mut out = format!(
        "📅 昨日（{}）动过 {} 条（按时间倒序）：",
        t_str,
        touched.len(),
    );
    for v in &touched {
        let hm = if v.updated_at.len() >= 16 {
            &v.updated_at[11..16]
        } else {
            ""
        };
        let emoji = match v.status {
            TaskStatus::Done => "✅",
            TaskStatus::Error => "⚠️",
            TaskStatus::Cancelled => "🚫",
            TaskStatus::Pending => {
                if v.raw_description.contains("[snooze:") {
                    "💤"
                } else {
                    "⏳"
                }
            }
        };
        if hm.is_empty() {
            out.push_str(&format!("\n· {} {}", emoji, v.title));
        } else {
            out.push_str(&format!("\n· {} {} {}", emoji, hm, v.title));
        }
        if matches!(v.status, TaskStatus::Done) {
            if let Some(r) = &v.result {
                let r_trim = r.trim();
                if !r_trim.is_empty() {
                    let preview: String = if r_trim.chars().count() > 40 {
                        let head: String = r_trim.chars().take(40).collect();
                        format!("{}…", head)
                    } else {
                        r_trim.to_string()
                    };
                    out.push_str(&format!(" — {}", preview));
                }
            }
        }
    }
    out
}

/// `/quick <text>` 命令回复文案。pure：极短 ack — 与 `format_task_created_
/// success`（包含完整 /tasks / /cancel 指引）反向 — 让 owner 快速 dump
/// 不被长 reply 打扰。
/// - 空 / 全空白 text → usage hint
/// - save_ok = Ok(()) → "✓ <title>"（单行）
/// - save_ok = Err(msg) → 失败反馈含原 err（owner 需要知道为啥没创成）
pub fn format_quick_reply(text: &str, save_ok: Result<(), &str>) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "⚡ 用法：/quick <text>\n\n静默创一条 P3 task — reply 仅 ✓ + title，适合快速 dump 想法 / 灵感不被长 reply 打扰。\n\n例：/quick 整理 ~/Downloads\n例：/quick 写周报\n\n想精细化（!! P5 / !!! P7）走 /task。".to_string();
    }
    match save_ok {
        Ok(()) => format!("✓ {}", trimmed),
        Err(e) => format!("⚡ 创建失败：{}", e),
    }
}

/// `/sleep` 命令回复文案。pure：caller 已调 `set_mute_minutes(480)`；本
/// 函数生成"晚安"语气 reply。until_local 注入让单测稳定（与 format_mute_
/// reply 同 pattern）。比 /mute 480 的中性文案更温和 — 让"睡前 mute"场
/// 景的情感色调对得上。
pub const SLEEP_MUTE_MINUTES: i64 = 480;
pub fn format_sleep_reply(until_local: Option<chrono::DateTime<chrono::Local>>) -> String {
    let when = until_local
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_else(|| "—".to_string());
    format!(
        "🌙 宠物去睡了 —— 8 小时静音，{}（次日 / 8h 后）自动醒。\n晚安！想立刻叫醒发 /mute 0。",
        when
    )
}

/// `/random` 命令回复文案。pure：从 views 里过滤 pending / error active
/// 任务，按 `index_seed % candidates.len()` 选一条；空 candidate → 兜底。
/// caller (bot.rs) 传 system time nanos 当 seed 拿非确定性体验，单测
/// 传固定 seed 拿确定行为。
pub const RANDOM_RAW_DESC_PREVIEW_CHARS: usize = 200;
pub fn format_random_reply(
    views: &[crate::task_queue::TaskView],
    index_seed: usize,
) -> String {
    use crate::task_queue::TaskStatus;
    let candidates: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| matches!(v.status, TaskStatus::Pending | TaskStatus::Error))
        .collect();
    if candidates.is_empty() {
        return "🎲 /random\n\n本聊天暂无 active 任务（pending / error）可抽。\n用 /task <title> 创一条 / /tasks 看 done & cancelled。".to_string();
    }
    let chosen = candidates[index_seed % candidates.len()];
    let status_emoji = match chosen.status {
        TaskStatus::Pending => "⏳",
        TaskStatus::Error => "⚠️",
        _ => "?",
    };
    let mut out = format!(
        "🎲 抽中 {} 「{}」（共 {} 条 active）",
        status_emoji,
        chosen.title,
        candidates.len()
    );
    let raw = chosen.raw_description.trim();
    if !raw.is_empty() {
        let preview: String = if raw.chars().count() > RANDOM_RAW_DESC_PREVIEW_CHARS {
            let head: String = raw.chars().take(RANDOM_RAW_DESC_PREVIEW_CHARS).collect();
            format!("{}…", head)
        } else {
            raw.to_string()
        };
        out.push_str("\n\n");
        out.push_str(&preview);
    }
    out.push_str("\n\n—— 选择困难？就先做这条吧。");
    out
}

/// `/random_pinned` 命令回复文案。pure：与 `format_random_reply` 同
/// 结构但 candidates filter 改为「pinned + active（pending/error）」交
/// 集 — owner 钉的且没完成的随机选。空集兜底教学指 /pin 设置 / /random
/// fallback。
pub fn format_random_pinned_reply(
    views: &[crate::task_queue::TaskView],
    index_seed: usize,
) -> String {
    use crate::task_queue::TaskStatus;
    let candidates: Vec<&crate::task_queue::TaskView> = views
        .iter()
        .filter(|v| v.pinned)
        .filter(|v| matches!(v.status, TaskStatus::Pending | TaskStatus::Error))
        .collect();
    if candidates.is_empty() {
        return "🎲 /random_pinned\n\n本聊天无 pinned active task。\n用 /pin <title> 钉一条 / /random 看全 active 集 / /pinned 看 pinned 清单。".to_string();
    }
    let chosen = candidates[index_seed % candidates.len()];
    let status_emoji = match chosen.status {
        TaskStatus::Pending => "⏳",
        TaskStatus::Error => "⚠️",
        _ => "?",
    };
    let mut out = format!(
        "🎲 抽中 {} 「{}」（共 {} 条 pinned active）",
        status_emoji,
        chosen.title,
        candidates.len()
    );
    let raw = chosen.raw_description.trim();
    if !raw.is_empty() {
        let preview: String = if raw.chars().count() > RANDOM_RAW_DESC_PREVIEW_CHARS {
            let head: String = raw.chars().take(RANDOM_RAW_DESC_PREVIEW_CHARS).collect();
            format!("{}…", head)
        } else {
            raw.to_string()
        };
        out.push_str("\n\n");
        out.push_str(&preview);
    }
    out.push_str("\n\n—— 选择困难？就先做这条吧。");
    out
}

/// `/last` 命令回复文案。pure：扫 views 按 created_at desc 拿首条，输出
/// `🆕 title` header + status emoji + 相对时间 + raw_description 前 200
/// 字符预览。views 空 → 友好兜底。caller 传 `now` 让相对时间单测稳定。
pub const LAST_RAW_DESC_PREVIEW_CHARS: usize = 200;
pub fn format_last_reply(
    views: &[crate::task_queue::TaskView],
    now: chrono::NaiveDateTime,
) -> String {
    use crate::task_queue::TaskStatus;
    if views.is_empty() {
        return "🆕 /last\n\n本聊天还没派过单。\n用 /task <title> 创建第一条。".to_string();
    }
    // ISO created_at 字典序 = 时间序，取 max 即最新创建。空 created_at
    // 兜底 — 极旧 task 可能无字段，按空串排尾。
    let latest = views
        .iter()
        .max_by(|a, b| a.created_at.cmp(&b.created_at))
        .expect("non-empty views guaranteed above");
    let status_emoji = match latest.status {
        TaskStatus::Pending => "⏳",
        TaskStatus::Done => "✅",
        TaskStatus::Error => "⚠️",
        TaskStatus::Cancelled => "🚫",
    };
    // 相对时间：与 PanelTasks `📅 N 前` / `🕰 拖了` 同 buckets — 通过
    // pure 计算（不调 frontend formatRelativeAge — 后端独立实现）。
    let rel = format_created_relative(&latest.created_at, now);
    let mut out = format!(
        "🆕 最近创建 {} 「{}」\n📅 {}",
        status_emoji,
        latest.title,
        rel
    );
    let raw = latest.raw_description.trim();
    if !raw.is_empty() {
        let preview: String = if raw.chars().count() > LAST_RAW_DESC_PREVIEW_CHARS {
            let head: String = raw.chars().take(LAST_RAW_DESC_PREVIEW_CHARS).collect();
            format!("{}…", head)
        } else {
            raw.to_string()
        };
        out.push_str("\n\n");
        out.push_str(&preview);
    }
    out
}

/// pure：created_at ISO 时间 → "N 分钟前 / 小时前 / 天前 / 刚创建" 桶式
/// 文案。解析失败 / 未来 ts 兜底空串。
pub fn format_created_relative(
    created_at: &str,
    now: chrono::NaiveDateTime,
) -> String {
    let parsed = chrono::DateTime::parse_from_rfc3339(created_at)
        .ok()
        .map(|dt| dt.naive_local());
    let Some(c) = parsed else {
        return format!("created_at parse 失败：{}", created_at);
    };
    let diff = now.signed_duration_since(c);
    if diff.num_seconds() < 60 {
        return "刚创建".to_string();
    }
    if diff.num_minutes() < 60 {
        return format!("{} 分钟前", diff.num_minutes());
    }
    if diff.num_hours() < 24 {
        return format!("{} 小时前", diff.num_hours());
    }
    format!("{} 天前", diff.num_days())
}

/// `/now` 命令回复文案。pure：一行 / 两行的快速状态 check。
/// - 第一行：mood emoji + `YYYY-MM-DD HH:MM` + tz 偏移（如 `+08:00`）
/// - 第二行：陪伴天数 + 心情文本（mood 空时省略心情段）
///
/// caller 注入 now / companionship_days / mood，便于单测断言确定行为。
/// mood = None / 空 text → 第一行用 🐾 兜底 + 第二行不显心情段。
pub fn format_now_reply(
    now: chrono::DateTime<chrono::FixedOffset>,
    companionship_days: Option<u64>,
    mood_text: Option<&str>,
) -> String {
    let mood_t = mood_text.map(|s| s.trim()).filter(|s| !s.is_empty());
    let emoji = mood_t.map(mood_emoji_for).unwrap_or("🐾");
    let time = now.format("%Y-%m-%d %H:%M").to_string();
    // tz offset：`+08:00` / `-05:00` 形式给 owner 一眼看时区上下文
    let tz = now.format("%:z").to_string();
    let mut out = format!("{} {} ({})", emoji, time, tz);
    // 陪伴天数 + 心情段。两段都缺时第二行整段省略，仅保第一行时间。
    let mut bits: Vec<String> = Vec::new();
    if let Some(days) = companionship_days {
        if days == 0 {
            bits.push("今天与你初识".to_string());
        } else {
            bits.push(format!("陪伴 {} 天", days));
        }
    }
    if let Some(t) = mood_t {
        bits.push(format!("心情：{}", t));
    }
    if !bits.is_empty() {
        out.push('\n');
        out.push_str(&bits.join(" · "));
    }
    out
}

/// `/last_speech` 命令回复文案。pure：caller 已 await
/// `recent_speeches_with_meta(1)` 拿到最近一条 entry（如有）+ now 时
/// 间锚点；本函数仅做字符串拼装。
///
/// 入参：
/// - `entry`: Option<(ts_str, text)>；None = speech_history 空 / 读失败
/// - `now`: 计算相对时间（"N 分前 / N 小时前 / N 天前"）的锚点
///
/// 4 态：
/// - None → 「🗣 pet 还没主动开口过」友好兜底
/// - parse ts 失败 → 「🗣 pet 最近主动开口：「<text>」（ts 解析失败 — <raw_ts>）」
/// - 成功 → 「🗣 pet 最近主动开口 · MM-DD HH:MM（N 分前）：\n「<text 前 N 字>」」
///
/// text 截 200 字 cap（与 /last 同 preview 上限）+ 末尾 "…" hint。
pub fn format_last_speech_reply(
    entry: Option<(&str, &str)>,
    now: chrono::DateTime<chrono::Local>,
) -> String {
    let Some((ts_str, text)) = entry else {
        return "🗣 pet 还没主动开口过 — speech_history.log 空 / pet 刚启动 / 一直被 mute。\n用 /aware 看 pet 当前感知；/here 看 owner 信号 snapshot。".to_string();
    };
    // text 截 200 字 cap + 末尾 …
    let chars: Vec<char> = text.chars().collect();
    let preview: String = if chars.len() > 200 {
        let head: String = chars.iter().take(200).collect();
        format!("{}…", head)
    } else {
        text.to_string()
    };
    // parse ts
    let Ok(when) = chrono::DateTime::parse_from_rfc3339(ts_str) else {
        return format!(
            "🗣 pet 最近主动开口：\n「{}」\n（ts 解析失败：{}）",
            preview, ts_str
        );
    };
    let when_local = when.with_timezone(&chrono::Local);
    let when_label = when_local.format("%m-%d %H:%M").to_string();
    let diff = now.signed_duration_since(when_local);
    let rel = if diff.num_seconds() < 0 {
        "刚刚".to_string()
    } else if diff.num_minutes() < 1 {
        "刚刚".to_string()
    } else if diff.num_hours() < 1 {
        format!("{} 分前", diff.num_minutes())
    } else if diff.num_days() < 1 {
        format!("{} 小时前", diff.num_hours())
    } else {
        format!("{} 天前", diff.num_days())
    };
    format!(
        "🗣 pet 最近主动开口 · {}（{}）：\n「{}」",
        when_label, rel, preview
    )
}

/// `/show_speech <N>` 命令回复文案。pure：caller 已 await
/// `recent_speeches_with_meta(n)` 拿到 oldest-first vec；本函数倒序
/// （newest-first）+ 拼字符串。
///
/// 每行 `· MM-DD HH:MM · <text 80 字 cap>`；text 80 字截断（per-row
/// 紧凑 vs /last_speech 单条 200 字完整）。空 → 友好兜底。
pub fn format_show_speech_reply(
    entries: &[(String, String)],
) -> String {
    if entries.is_empty() {
        return "🗣 speech_history 空 — pet 还没主动开口过 / 刚 reset / 一直被 mute。\n用 /aware 看 pet 当前感知；/last_speech 单条 audit。".to_string();
    }
    // entries 来自 caller 已是 oldest-first；reverse 让 newest 在前
    let mut sorted: Vec<&(String, String)> = entries.iter().collect();
    sorted.reverse();
    let mut out = format!(
        "🗣 pet 最近 {} 条主动开口（newest first）：",
        sorted.len()
    );
    for (ts_str, text) in &sorted {
        let when_label =
            match chrono::DateTime::parse_from_rfc3339(ts_str) {
                Ok(dt) => dt
                    .with_timezone(&chrono::Local)
                    .format("%m-%d %H:%M")
                    .to_string(),
                Err(_) => ts_str.to_string(),
            };
        let chars: Vec<char> = text.chars().collect();
        let preview: String = if chars.len() > 80 {
            let head: String = chars.iter().take(80).collect();
            format!("{}…", head)
        } else {
            (*text).clone()
        };
        // flatten newline 让单行 reply 可读
        let flat = preview.replace('\n', " ").replace('\t', " ");
        out.push_str(&format!("\n· {} · {}", when_label, flat));
    }
    out
}

/// `/aware` 命令回复文案。pure。
///
/// 入参（caller 在 bot handler 抓齐传入，让 formatter 可单元测试）：
/// - `transient`: Option<(text, remaining_minutes)>；None = 当前无；
///   remaining_minutes 可能为 0（恰过期边界，caller 取最大 1）
/// - `active_count`: butler_tasks 段内非 [done] 条目数
/// - `mood_text`: Option<&str>；None / 空 / 仅空白 → 走 mood_emoji_for
///   兜底 🐾 + 不显文本
/// - `now`: DateTime<FixedOffset>；含 tz 偏移
/// - `companionship_days`: Option<u64>；0 = 今日初识，> 0 = "陪伴 N 天"
///
/// 输出（5 行，缺段省略让短输出更紧凑）：
///   🐾 当前感知：
///   📝 transient_note: 「<text>」（剩 N 分钟）  /  📝 transient_note: 无
///   📋 active tasks: N 条
///   ☁ mood: <emoji> <text>
///   🕐 当前: YYYY-MM-DD HH:MM (+08:00) · 陪伴 N 天
pub fn format_aware_reply(
    transient: Option<(&str, i64)>,
    active_count: usize,
    mood_text: Option<&str>,
    now: chrono::DateTime<chrono::FixedOffset>,
    companionship_days: Option<u64>,
) -> String {
    let mood_t = mood_text.map(|s| s.trim()).filter(|s| !s.is_empty());
    let emoji = mood_t.map(mood_emoji_for).unwrap_or("🐾");
    let mut out = String::from("🐾 当前感知：");
    // transient_note 行
    match transient {
        Some((text, mins)) if !text.trim().is_empty() => {
            let preview = if text.chars().count() > 60 {
                let head: String = text.chars().take(60).collect();
                format!("{}…", head)
            } else {
                text.to_string()
            };
            let mins_pos = mins.max(1);
            out.push_str(&format!(
                "\n📝 transient_note: 「{}」（剩 {} 分钟）",
                preview, mins_pos
            ));
        }
        _ => out.push_str("\n📝 transient_note: 无"),
    }
    out.push_str(&format!("\n📋 active tasks: {} 条", active_count));
    if let Some(t) = mood_t {
        out.push_str(&format!("\n☁ mood: {} {}", emoji, t));
    } else {
        // mood 空也显 emoji + "（暂无心情）"，让 owner 知道字段存在
        out.push_str(&format!("\n☁ mood: {} （暂无心情）", emoji));
    }
    let time = now.format("%Y-%m-%d %H:%M").to_string();
    let tz = now.format("%:z").to_string();
    let mut tail = format!("🕐 当前: {} ({})", time, tz);
    if let Some(days) = companionship_days {
        if days == 0 {
            tail.push_str(" · 今日初识");
        } else {
            tail.push_str(&format!(" · 陪伴 {} 天", days));
        }
    }
    out.push('\n');
    out.push_str(&tail);
    out
}

/// `/here` 命令回复文案。pure。
///
/// 入参（caller 抓齐传入便于 unit test 不依赖运行时全局 mutex）：
/// - `transient`: Option<(text, remaining_minutes)>；与 /aware 同 shape
/// - `mute_remaining_minutes`: Option<i64>；None = 未静音；Some(0) →
///   clamp 显 "剩 1 分钟"（边界过期态）
/// - `band`: feedback_history::classify_feedback_band 返的 &'static str
///   （"high_negative" / "low_negative" / "mid" / "insufficient_samples"）
///
/// 输出 4 行（标题 + 3 段信号）：
///   🧑 当前 owner 信号：
///   📝 transient_note: 「<text>」（剩 N 分钟）/ 未设
///   🔕 mute: 剩 N 分钟 / 未静音
///   💬 最近 feedback band: <band-label> · <factor 说明>
pub fn format_here_reply(
    transient: Option<(&str, i64)>,
    mute_remaining_minutes: Option<i64>,
    band: &str,
) -> String {
    let mut out = String::from("🧑 当前 owner 信号：");
    // transient_note 行 — 复用 /aware 同语义
    match transient {
        Some((text, mins)) if !text.trim().is_empty() => {
            let preview = if text.chars().count() > 60 {
                let head: String = text.chars().take(60).collect();
                format!("{}…", head)
            } else {
                text.to_string()
            };
            let mins_pos = mins.max(1);
            out.push_str(&format!(
                "\n📝 transient_note: 「{}」（剩 {} 分钟）",
                preview, mins_pos
            ));
        }
        _ => out.push_str("\n📝 transient_note: 未设"),
    }
    // mute 行
    match mute_remaining_minutes {
        Some(mins) => {
            let mins_pos = mins.max(1);
            out.push_str(&format!("\n🔕 mute: 剩 {} 分钟", mins_pos));
        }
        None => out.push_str("\n🔕 mute: 未静音"),
    }
    // feedback band 行 — 带 cooldown factor 说明让 owner 一眼看到"我
    // 给 pet 的信号让它现在更频繁 / 更克制"
    let (label, factor_note) = match band {
        "high_negative" => ("high_negative", "cooldown ×2.0（pet 更克制）"),
        "low_negative" => ("low_negative", "cooldown ×0.7（pet 更主动）"),
        "mid" => ("mid", "cooldown ×1.0（中性）"),
        _ => (
            "insufficient_samples",
            "样本不足 — cooldown 走基础值",
        ),
    };
    out.push_str(&format!(
        "\n💬 最近 feedback band: {} · {}",
        label, factor_note
    ));
    out
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

/// `/peek <title>` 命令回复文案。pure：
/// - 一行紧凑视图，区段间用 ` · ` 分隔
/// - `<status_emoji> 「<title>」 · <schedule?> · <markers?> · P{n}?`
/// - 各可选段只在有内容时拼入；都无 → 仅 emoji + title
///
/// schedule 解析：扫 raw_description 起始的 `[every: ...]` / `[once: ...]` /
/// `[deadline: ...]` 前缀（首个 `]` 收口）— 命中则原文显（仅去 `[` `]`），
/// 加 🕐 前缀。无前缀 → 段省略。
///
/// markers 段：扫 `[pinned]` → 📌；`[silent]` → 🔇；`[snooze: ...]` → 💤；
/// `[blockedBy: ...]` → 🔒。一句空格分隔；都无 → 段省略。
///
/// 优先级：扫 `[task pri=N]` → P{N}（N 必须 0..=9 单字符）；无 → 段省略。
///
/// 与 /show 互补：那个看完整 raw + detail.md preview；本命令仅 raw_description
/// + status，不读 detail.md（紧凑视图不需要）。
pub fn format_peek_reply(
    title: &str,
    raw_description: &str,
    status: crate::task_queue::TaskStatus,
) -> String {
    use crate::task_queue::TaskStatus;
    let status_emoji = match status {
        TaskStatus::Pending => "⏳",
        TaskStatus::Done => "✅",
        TaskStatus::Error => "⚠️",
        TaskStatus::Cancelled => "🚫",
    };
    let raw = raw_description.trim();
    // ---- schedule prefix ----
    // 仅认 raw_description 起始的 `[every: ...]` / `[once: ...]` / `[deadline: ...]`
    // — 与 parse_butler_schedule_prefix 同语义但本 formatter 只展示文本，无需
    // 解析时刻。首个 `]` 收口；非起始位置出现的 [every:...] 不算 schedule。
    let schedule_label: Option<String> = {
        const KEYS: &[&str] = &["every", "once", "deadline"];
        if raw.starts_with('[') {
            if let Some(close) = raw.find(']') {
                let inner = &raw[1..close];
                let matched = KEYS.iter().find(|k| {
                    inner.starts_with(*k)
                        && (inner.len() == k.len()
                            || inner[k.len()..].starts_with(':')
                            || inner[k.len()..].starts_with('：'))
                });
                matched.map(|_| inner.trim().to_string())
            } else {
                None
            }
        } else {
            None
        }
    };
    // ---- markers ----
    // 不复用 extract_marker_tokens 因为本命令要的 marker 集合不同：
    // - 收：pinned / silent / snooze / blockedBy（owner 看的活跃状态）
    // - 不收：done / error / result / cancelled / archived（状态本身已在
    //   status emoji 表达，重复显冗余）
    let mut marker_emojis: Vec<&str> = Vec::new();
    let bytes = raw.as_bytes();
    let mut i = 0;
    let mut saw_pin = false;
    let mut saw_silent = false;
    let mut saw_snooze = false;
    let mut saw_blocked = false;
    while i < bytes.len() {
        if bytes[i] != b'[' {
            i += 1;
            continue;
        }
        let close_rel = match raw[i..].find(']') {
            Some(p) => p,
            None => break,
        };
        let inner_end = i + close_rel;
        let inner = &raw[i + 1..inner_end];
        let starts_with_key = |k: &str| {
            inner.starts_with(k)
                && (inner.len() == k.len()
                    || inner[k.len()..].starts_with(':')
                    || inner[k.len()..].starts_with('：')
                    || inner[k.len()..].starts_with(' '))
        };
        if !saw_pin && starts_with_key("pinned") {
            saw_pin = true;
        }
        if !saw_silent && starts_with_key("silent") {
            saw_silent = true;
        }
        if !saw_snooze && starts_with_key("snooze") {
            saw_snooze = true;
        }
        if !saw_blocked && starts_with_key("blockedBy") {
            saw_blocked = true;
        }
        i = inner_end + 1;
    }
    if saw_pin {
        marker_emojis.push("📌");
    }
    if saw_silent {
        marker_emojis.push("🔇");
    }
    if saw_snooze {
        marker_emojis.push("💤");
    }
    if saw_blocked {
        marker_emojis.push("🔒");
    }
    // ---- priority ----
    // `[task pri=N]` 单字符 N（0..=9）。与 parse_task_prefix 同源约定 — 仅
    // 取首个出现的 `[task pri=` 段。
    let priority_label: Option<String> = {
        let needle = "[task pri=";
        raw.find(needle).and_then(|pos| {
            let after = &raw[pos + needle.len()..];
            let first = after.chars().next()?;
            if first.is_ascii_digit() {
                Some(format!("P{}", first))
            } else {
                None
            }
        })
    };
    // ---- assemble ----
    let mut out = format!("{} 「{}」", status_emoji, title.trim());
    if let Some(s) = schedule_label {
        out.push_str(&format!(" · 🕐 {}", s));
    }
    if !marker_emojis.is_empty() {
        out.push_str(&format!(" · {}", marker_emojis.join(" ")));
    }
    if let Some(p) = priority_label {
        out.push_str(&format!(" · {}", p));
    }
    out
}

/// `/dup <title>` 命令成功回复文案。pure：
/// - 一行标题映射：`📑 已复制「<src>」→「<new>」`
/// - 注脚两行：继承 / 剥落 markers 说明，让 owner 一眼看清楚副本继承
///   了什么、丢了什么 — 比 silent success 更有 audit 价值
pub fn format_dup_reply(src_title: &str, new_title: &str) -> String {
    format!(
        "📑 已复制「{}」→「{}」\n· 继承 schedule / markers / tags / priority / due\n· 剥终态 markers（done / result / snooze / origin 等）",
        src_title.trim(),
        new_title.trim(),
    )
}

/// pure：从 task raw_description 抽 `[snippet]` / `[snippet: <label>]` 标记
/// 的可选 label。
/// - 无 marker → None
/// - `[snippet]` 或 `[snippet:]` → Some("")
/// - `[snippet: <label>]` → Some("<label>".trim())（label 不含两端空白）
/// - 全角冒号 `[snippet：label]` → 同等支持
///
/// 多次出现仅取首个；非 token-boundary 起始（如 `prefix[snippet]`）也算（与
/// extract_marker_tokens 同行为：`[` 起 + 首 `]` 收口）。
pub fn parse_snippet_marker(description: &str) -> Option<String> {
    let bytes = description.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'[' {
            i += 1;
            continue;
        }
        let rest = &description[i..];
        let close_rel = rest.find(']')?;
        let inner = &rest[1..close_rel];
        if let Some(after_key) = inner.strip_prefix("snippet") {
            // 后接 `]` (即 inner 仅为 "snippet") / `:` / `：` / ` `
            // 三类才算命中（防 [snippetXY] 这种碰撞）。
            if after_key.is_empty() {
                return Some(String::new());
            }
            let first = after_key.chars().next()?;
            if first == ':' || first == '：' || first == ' ' {
                let label = after_key
                    .trim_start_matches([':', '：', ' '])
                    .trim()
                    .to_string();
                return Some(label);
            }
        }
        i += close_rel + 1;
    }
    None
}

/// `/snippets` 命令回复文案。pure：
/// - 输入 views 已是 chat-scope filtered + 已含 [snippet] marker（caller 过）
/// - 空 → 友好兜底 + 教学例
/// - 非空 → `📎 snippets · N 条：` + 每行 status_emoji + title + [label]
///   （非空时显）+ body 前 80 字预览
///
/// body 预览：从 raw_description 提取 — strip [task pri=...] header 后取前
/// 80 字，flatten 多空白成单空格，超长 + …
pub const SNIPPET_BODY_PREVIEW_CHARS: usize = 80;
pub fn format_snippets_reply(views: &[crate::task_queue::TaskView]) -> String {
    use crate::task_queue::TaskStatus;
    if views.is_empty() {
        return "📎 本聊天派单还没标 snippet —— 在 /edit 中给可复用 task 加 `[snippet]` 或 `[snippet: <label>]` marker 后回来 audit。\n\n例：\n  /edit PR 评审模板 :: [snippet: PR template] checklist...\n  /edit 决策日志开头 :: [snippet] 今天的关键决策...\n\n之后再 /snippets 看「我都标了哪些可复用」。配合 /dup 一个 snippet 改装为新任务。".to_string();
    }
    let mut out = String::new();
    out.push_str(&format!("📎 snippets · {} 条：\n", views.len()));
    for v in views {
        let status_emoji = match v.status {
            TaskStatus::Pending => "🟢",
            TaskStatus::Done => "✅",
            TaskStatus::Error => "⚠️",
            TaskStatus::Cancelled => "🚫",
        };
        let label = parse_snippet_marker(&v.raw_description).unwrap_or_default();
        // body 预览：parse_task_header 抽 body，再 strip 所有 [...] markers 让
        // 预览是「实际内容」而非满屏 markers。collapse_whitespace 单空格化。
        let body_raw = match crate::task_queue::parse_task_header(&v.raw_description) {
            Some(h) => h.body,
            None => v.raw_description.clone(),
        };
        let body_clean: String = body_raw
            .chars()
            .filter(|_| true)
            .collect::<String>()
            // 简化：保 markers 在预览里 — owner 想看完整走 /show；这里
            // 主要让 owner 一眼能从预览认出 task。不另 strip。
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let body_preview: String = if body_clean.chars().count() > SNIPPET_BODY_PREVIEW_CHARS {
            let head: String = body_clean
                .chars()
                .take(SNIPPET_BODY_PREVIEW_CHARS)
                .collect();
            format!("{}…", head)
        } else {
            body_clean
        };
        if label.is_empty() {
            out.push_str(&format!("{} {}\n", status_emoji, v.title));
        } else {
            out.push_str(&format!("{} {} [{}]\n", status_emoji, v.title, label));
        }
        if !body_preview.is_empty() {
            out.push_str(&format!("   {}\n", body_preview));
        }
    }
    out
}

/// `/peek_pinned` 命令回复文案。pure：caller 已 chat-scope + pinned
/// filter，本 formatter 渲染 header + 每条 view 走 format_peek_reply
/// 单行（与 /peek 单 task 视图完全一致格式）。
///
/// 空 → 教学兜底指 /pin <title> + /pinned 入口。pinned task 通常少
/// （owner 钉自己最在意的几条），不需 cap。
pub fn format_peek_pinned_reply(
    views: &[crate::task_queue::TaskView],
) -> String {
    if views.is_empty() {
        return "📌 暂无 pinned task。\n用 /pin <title> 钉一条任务（owner 最在意的几条让 proactive 选单优先关注）；之后 /peek_pinned 一行紧凑批量看。".to_string();
    }
    let mut out = format!("📌 {} 条 pinned：", views.len());
    for v in views {
        out.push('\n');
        // 复用单 task /peek formatter — 每行一致 schedule + markers + 状态
        // emoji 渲染（含 [snippet] 等无需在此独立处理）
        out.push_str(&format_peek_reply(&v.title, &v.raw_description, v.status));
    }
    out
}

/// `/timeline` 中一行事件条目。`markers` 是该事件 snippet 内扫出的「状态
/// 变化」marker token 列表（保 `[done]` / `[result: 已发送]` 等完整原文），
/// 顺序保持 snippet 内出现顺序。
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct TimelineEntry {
    pub timestamp: String,
    pub action: String,
    pub markers: Vec<String>,
    /// rename event 专属：从 snippet 的 `[was: <old>]` 标记里取 old title。
    /// 其它 action（create / delete / update）始终 None。让 format_timeline_reply
    /// / format_recent_events_reply 能渲「重命名 from 「<old>」」而不是
    /// fallback 到「更新（无 marker 变化）」误判。
    pub was: Option<String>,
}

/// pure：从 butler_history snippet 抽出「状态变化」marker tokens。
///
/// 识别白名单：done / error / snooze / result / cancelled / pinned /
/// silent / blockedBy / archived。每命中一个 `[<key>...]` 段（直到首个
/// `]` 收口），保留原文整段（含闭合 `]`）— 让 owner 看到 `[result: 已
/// 发送]` 这种含 payload 的完整原话。
///
/// 不识别静态元数据：`[task pri=...]` / `[origin:...]` / `[every:...]` /
/// `[once:...]` / `[deadline:...]` / `[remind:...]` / `[tags:...]` 等 —
/// 这些是任务身份元数据非"状态变化"信号。
///
/// 同一 marker key 在 snippet 内多次出现都收（如多次 `[error: ...]`），
/// 由调用方决定是否去重。返回顺序 = 出现顺序。
pub fn extract_marker_tokens(snippet: &str) -> Vec<String> {
    // key 大小写敏感（与 task_queue 既有 marker 大小写约定一致：done /
    // error / snooze / result / cancelled / pinned / silent / blockedBy /
    // archived）。blockedBy 是唯一 camelCase key，匹配现网约定。
    const KEYS: &[&str] = &[
        "done", "error", "snooze", "result", "cancelled", "pinned",
        "silent", "blockedBy", "archived",
    ];
    let bytes = snippet.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'[' {
            i += 1;
            continue;
        }
        // 找匹配的闭合 ]（不嵌套；snippet 一行已 flatten）
        let close_rel = match snippet[i..].find(']') {
            Some(p) => p,
            None => break,
        };
        let inner_start = i + 1;
        let inner_end = i + close_rel;
        let inner = &snippet[inner_start..inner_end];
        // 命中白名单 key？inner 必须以某 key 开头 + 后接 ` ` / `:` /
        // `：` / `]`（即"key 单独存在"或"key + 值"），避免 "[doneish]"
        // 这种碰撞。
        let matched = KEYS.iter().any(|k| {
            if !inner.starts_with(k) {
                return false;
            }
            let rest = &inner[k.len()..];
            rest.is_empty()
                || rest.starts_with(':')
                || rest.starts_with('：')
                || rest.starts_with(' ')
        });
        if matched {
            // 收 [..] 完整原文（含两端方括号）
            out.push(snippet[i..=inner_end].to_string());
        }
        i = inner_end + 1;
    }
    out
}

/// pure：把 butler_history events（新→旧 顺序，与 `filter_history_for_task`
/// 返回一致）转 timeline entries（旧→新 顺序，给前端按时序读）。
///
/// 实现：
/// 1. 反转输入到 chronological（旧→新）
/// 2. 对每个事件，extract_marker_tokens 拿当前 snapshot 的 markers
/// 3. 第一个事件 / action != "update" / markers 集合相对前事件有差异 →
///    保留为 timeline entry；否则丢弃（去除连续无变化的 update 噪声 —
///    比如 LLM 多次 update detail 但 markers 不动）
///
/// 用 marker key 集合（提取 `[<key>` 前缀）作比较 — 让 `[snooze: A]` →
/// `[snooze: B]` 这种 payload 变化也算变化保留（key 相同但具体 token
/// 文本不同）。具体: 比对的是去重后的 `marker_keys + marker_full_tokens`
/// 联合 — 任一变化即保留。
pub fn compute_timeline_entries(
    events_newest_first: &[(String, String, String)],
) -> Vec<TimelineEntry> {
    // filter_history_for_task 输出已是 newest-first；这里 reverse 到 chronological（旧→新）
    let chronological: Vec<&(String, String, String)> =
        events_newest_first.iter().rev().collect();
    let mut out: Vec<TimelineEntry> = Vec::new();
    let mut prev_signature: Option<Vec<String>> = None;
    for (ts, action, snippet) in chronological {
        let markers = extract_marker_tokens(snippet);
        let signature = markers.clone();
        let is_first = prev_signature.is_none();
        let action_lc = action.to_ascii_lowercase();
        let force_keep = action_lc != "update"; // create / delete 总保
        let changed = match &prev_signature {
            None => true,
            Some(p) => *p != signature,
        };
        if is_first || force_keep || changed {
            // rename event：解 snippet 内的 `[was: <old>]` token 把 old title
            // 拎出来给 formatter 用。snippet 格式由 memory_rename 写入
            // butler_history.log 时硬编码（commands/memory.rs 内）。其它
            // action 始终 was=None。`[was: ...]` 80 字截断可能砍掉尾 `]`
            // — 兜底取到末尾整段当 old title 文本，prefix `[was: ` 长度固
            // 定（6 chars），strip 后剥尾 `]`（若存在）。
            let was = if action_lc == "rename" {
                extract_was_from_snippet(snippet)
            } else {
                None
            };
            out.push(TimelineEntry {
                timestamp: ts.clone(),
                action: action.clone(),
                markers,
                was,
            });
        }
        prev_signature = Some(signature);
    }
    out
}

/// pure：从 butler_history.log 的 rename 事件 snippet 抽 `[was: <old>]`
/// 标记里的 old title。format 由 memory_rename 写入端约定（commands/
/// memory.rs::memory_rename）。
///
/// 兜底：
/// - snippet 不含 `[was: ` prefix → None
/// - 含 prefix 但 80 字截断把尾 `]` 砍了 → 取到 snippet 末尾整段当 old
///   title（best-effort，old 极长会被截）
/// - snippet 含多个 `[was: ` token（不应发生但 defensive）→ 取第一个
pub fn extract_was_from_snippet(snippet: &str) -> Option<String> {
    let prefix = "[was: ";
    let start = snippet.find(prefix)?;
    let after = &snippet[start + prefix.len()..];
    // 截到首个 `]`；找不到（截断 / 异常）→ 取 after 全段，剥末尾 `…`
    let old = match after.find(']') {
        Some(p) => after[..p].to_string(),
        None => after.trim_end_matches('…').to_string(),
    };
    let trimmed = old.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// pure：把 `[ts]` 字段格式化成短显示 `MM-DD HH:MM`。butler_history 写
/// 的是 RFC3339 + tz（如 `2026-05-17T18:30:42+08:00`）— 直接用前 16 字
/// 节即可剥到日期 + 时刻 + 把 `T` 换成空格再切。解析失败 / 形式不识 →
/// 兜底返完整 ts 串（不丢信息）。
pub fn format_timeline_ts(ts: &str) -> String {
    // 期望形如 "YYYY-MM-DDTHH:MM:SS+08:00"。提取 "MM-DD HH:MM"。
    // 不引 chrono 重解析 — string slicing 已足够且 robust 对非标准 ts。
    let bytes = ts.as_bytes();
    if bytes.len() >= 16
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
    {
        return format!("{} {}", &ts[5..10], &ts[11..16]);
    }
    ts.to_string()
}

/// pure：`/timeline <title>` 命令回复文案。
/// - entries 空 → 兜底文案（"无 history 记录"），仍含 task title 让 owner
///   知道命中了对的 task（与 raw-empty 区分）
/// - 非空：标题行 `🕰️ 「<title>」时间线 · N 个事件` + 每条事件一行
///
/// 事件行格式：`<emoji> MM-DD HH:MM · <body>`。body 视 action / markers：
/// - action == "create" → `创建`
/// - action == "delete" → `删除`
/// - markers 非空 → markers 用空格连接（保原文 `[done]` `[result: ...]`）
/// - markers 空（仅是 update 但 snippet 截断 / 无状态变化 marker）→
///   `更新（无 marker 变化）` — 已被 compute_timeline_entries 大多去重
///   但仍可能落到第一个事件本身就是 update（如重启后首条 update）
///
/// 总条数超过 cap 时截前 N + overflow 行 — 防 TG 单消息 4096 字符炸。
pub fn format_timeline_reply(
    title: &str,
    entries: &[TimelineEntry],
    total_events: usize,
) -> String {
    const TIMELINE_ENTRY_CAP: usize = 30;
    let title = title.trim();
    if entries.is_empty() {
        return format!(
            "🕰️ 「{}」时间线\n\n（butler_history 内无该 task 的事件记录 — 可能是日志被轮转切掉，或 task 刚创建尚未写入。/show {} 查当前 snapshot。）",
            title, title
        );
    }
    let mut out = String::new();
    out.push_str(&format!(
        "🕰️ 「{}」时间线 · {} 个事件",
        title, total_events
    ));
    if entries.len() < total_events {
        out.push_str(&format!("（去重无变化 update 后保留 {} 条）", entries.len()));
    }
    out.push_str("\n\n");
    let show_count = entries.len().min(TIMELINE_ENTRY_CAP);
    for e in entries.iter().take(show_count) {
        let action_lc = e.action.to_ascii_lowercase();
        let emoji = match action_lc.as_str() {
            "create" => "📝",
            "delete" => "🗑️",
            "rename" => "🔁",
            _ => "✏️",
        };
        let ts_short = format_timeline_ts(&e.timestamp);
        let body = if action_lc == "create" {
            "创建".to_string()
        } else if action_lc == "delete" {
            "删除".to_string()
        } else if action_lc == "rename" {
            match &e.was {
                Some(old) => format!("重命名 from 「{}」", old),
                // 截断 / 异常 → 仍可见但不知 old；至少不误判为「无 marker」
                None => "重命名（old title 不可解）".to_string(),
            }
        } else if e.markers.is_empty() {
            "更新（无 marker 变化）".to_string()
        } else {
            e.markers.join(" ")
        };
        out.push_str(&format!("{} {} · {}\n", emoji, ts_short, body));
    }
    if entries.len() > TIMELINE_ENTRY_CAP {
        out.push_str(&format!(
            "\n…（保留前 {} 条；剩余 {} 条省略）",
            TIMELINE_ENTRY_CAP,
            entries.len() - TIMELINE_ENTRY_CAP
        ));
    }
    out
}

/// `/recent_events <title> [N]` 命令回复文案。pure，与 format_timeline_reply
/// 共享底层 entries 数据但 cap 到「最近 N」（slice 末尾 N 条，因 entries
/// 是 chronological 旧→新 序）。
///
/// 与 timeline 行为差异：
/// - timeline 显前 TIMELINE_ENTRY_CAP=30 条（chronological 起头）
/// - recent_events 显最后 N 条（最近优先；caller 已 clamp 1..=20）
///
/// 输出格式：
///   📜 「<title>」最近 N 个事件（共 M）：
///   📝 MM-DD HH:MM · 创建
///   ✏️ MM-DD HH:MM · [pinned]
///   ...
pub fn format_recent_events_reply(
    title: &str,
    entries: &[TimelineEntry],
    total_events: usize,
    n: u32,
) -> String {
    let title = title.trim();
    if entries.is_empty() {
        return format!(
            "📜 「{}」最近事件\n\n（butler_history 内无该 task 的事件记录 — 可能是日志被轮转切掉，或 task 刚创建尚未写入。/show {} 查当前 snapshot。）",
            title, title
        );
    }
    let show_count = entries.len().min(n as usize);
    // entries 是 chronological（旧→新）— 取末尾 N 即「最近 N」
    let start = entries.len().saturating_sub(show_count);
    let recent_slice = &entries[start..];
    let mut out = format!(
        "📜 「{}」最近 {} 个事件（共 {}）：\n\n",
        title,
        recent_slice.len(),
        total_events,
    );
    for e in recent_slice {
        let action_lc = e.action.to_ascii_lowercase();
        let emoji = match action_lc.as_str() {
            "create" => "📝",
            "delete" => "🗑️",
            "rename" => "🔁",
            _ => "✏️",
        };
        let ts_short = format_timeline_ts(&e.timestamp);
        let body = if action_lc == "create" {
            "创建".to_string()
        } else if action_lc == "delete" {
            "删除".to_string()
        } else if action_lc == "rename" {
            match &e.was {
                Some(old) => format!("重命名 from 「{}」", old),
                None => "重命名（old title 不可解）".to_string(),
            }
        } else if e.markers.is_empty() {
            "更新（无 marker 变化）".to_string()
        } else {
            e.markers.join(" ")
        };
        out.push_str(&format!("{} {} · {}\n", emoji, ts_short, body));
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
#[path = "commands_tests.rs"]
mod tests;
