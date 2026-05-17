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

/// iter #393: `/edit_due <title> <preset>` 命令的 preset 维度。比
/// `/due` 的 DuePreset（仅 audit 时间段）更广 — 含 tonight / 单
/// weekday / next-week weekday / +Nm/h/d 相对时长 / clear 多形态。
/// caller 把 preset 与 now 注入 `compute_edit_due_preset` 得到具体
/// NaiveDateTime（pure，便单测）。
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EditDuePreset {
    /// today 18:00；若 now 已过 18:00 → tomorrow 18:00（避免点完一下
    /// 子 "tonight" 又被解释成已过去时刻 footgun）
    Tonight,
    /// tomorrow 09:00
    TomorrowMorning,
    /// 本周（或最近未来）某 weekday 09:00。`weekday`: 0=Mon..6=Sun
    /// 与 chrono::Weekday::num_days_from_monday() 同 mapping
    Weekday(u8),
    /// 下周某 weekday 09:00（本周已过或本日 weekday 也算下周以避免
    /// 撞当日 footgun）
    NextWeekday(u8),
    /// now + minutes（+Nm）
    PlusMinutes(u32),
    /// now + hours（+Nh）
    PlusHours(u32),
    /// now + days 09:00（+Nd — 几天后早上 9 点，而非"几天后此刻"避
    /// 免 due 落到午夜 / 半夜的反直觉）
    PlusDays(u32),
    /// 清掉 due（"clear" / "none" / "0"）
    Clear,
}

/// pure：识别 owner 输入的 edit_due preset。tonight / morning / 单
/// weekday / next-week weekday / +Nm/h/d / clear 多形态；中英 alias
/// 同表；大小写不敏感。未识别返 None 让 handler 走 usage hint。
pub fn parse_edit_due_preset(s: &str) -> Option<EditDuePreset> {
    let lower = s.trim().to_lowercase();
    match lower.as_str() {
        "tonight" | "今晚" | "today_evening" | "today-evening" => {
            return Some(EditDuePreset::Tonight);
        }
        "tomorrow" | "tmr" | "tm" | "明天" | "明日" | "morning" | "早上" => {
            return Some(EditDuePreset::TomorrowMorning);
        }
        "clear" | "none" | "0" | "清除" | "取消" => {
            return Some(EditDuePreset::Clear);
        }
        _ => {}
    }
    // Weekday 单词：mon/tue/.../sun + 周一..周日
    let weekday_map: &[(&str, u8)] = &[
        ("monday", 0), ("mon", 0), ("周一", 0), ("星期一", 0),
        ("tuesday", 1), ("tue", 1), ("周二", 1), ("星期二", 1),
        ("wednesday", 2), ("wed", 2), ("周三", 2), ("星期三", 2),
        ("thursday", 3), ("thu", 3), ("周四", 3), ("星期四", 3),
        ("friday", 4), ("fri", 4), ("周五", 4), ("星期五", 4),
        ("saturday", 5), ("sat", 5), ("周六", 5), ("星期六", 5),
        ("sunday", 6), ("sun", 6), ("周日", 6), ("周天", 6), ("星期日", 6),
    ];
    for (alias, idx) in weekday_map {
        if lower == *alias {
            return Some(EditDuePreset::Weekday(*idx));
        }
        // next_<weekday> / next-mon / 下周一
        let next_prefixes = ["next_", "next-", "下"];
        for pfx in &next_prefixes {
            let key = format!("{}{}", pfx, alias);
            if lower == key {
                return Some(EditDuePreset::NextWeekday(*idx));
            }
        }
    }
    // 相对时长：+Nm / +Nh / +Nd
    if let Some(rest) = lower.strip_prefix('+') {
        let (digits, unit) = rest.split_at(rest.len().saturating_sub(1));
        if let Ok(n) = digits.parse::<u32>() {
            if n > 0 {
                return match unit {
                    "m" => Some(EditDuePreset::PlusMinutes(n)),
                    "h" => Some(EditDuePreset::PlusHours(n)),
                    "d" => Some(EditDuePreset::PlusDays(n)),
                    _ => None,
                };
            }
        }
    }
    None
}

/// pure：把 EditDuePreset + now 算出具体 NaiveDateTime。`None` = Clear
/// 语义（caller 传 None 给 task_set_due 清 due）；`Some(dt)` = 设
/// 该时刻。返回类型 `Option<Option<NaiveDateTime>>` 似乎冗余，但语义
/// 上 `Some(None)` 是 "明确 Clear"（不是错误），与 `Some(Some(dt))`
/// 区分；caller 把内层 Option 转 `Option<String>` 传给 task_set_due。
pub fn compute_edit_due_preset(
    preset: &EditDuePreset,
    now: chrono::NaiveDateTime,
) -> Option<chrono::NaiveDateTime> {
    use chrono::{Duration, NaiveTime};
    let today = now.date();
    let nine_am = NaiveTime::from_hms_opt(9, 0, 0)?;
    let six_pm = NaiveTime::from_hms_opt(18, 0, 0)?;
    match preset {
        EditDuePreset::Tonight => {
            let tonight = today.and_time(six_pm);
            if tonight > now {
                Some(tonight)
            } else {
                // 已过 18:00 → 明晚 18:00 防"tonight 已过去"footgun
                Some((today + Duration::days(1)).and_time(six_pm))
            }
        }
        EditDuePreset::TomorrowMorning => {
            Some((today + Duration::days(1)).and_time(nine_am))
        }
        EditDuePreset::Weekday(idx) => {
            // 当前 weekday → target weekday 之差（mod 7）；0 时算下周
            // （避免设到今天同 weekday 但当前已过 9 点 → 落已过时刻）
            use chrono::Datelike;
            let cur = today.weekday().num_days_from_monday() as i64;
            let target = *idx as i64;
            let mut diff = (target - cur).rem_euclid(7);
            if diff == 0 {
                // 当日 weekday：若 09:00 仍未来则当日，否则下周
                let target_today = today.and_time(nine_am);
                if target_today > now {
                    return Some(target_today);
                }
                diff = 7;
            }
            Some((today + Duration::days(diff)).and_time(nine_am))
        }
        EditDuePreset::NextWeekday(idx) => {
            use chrono::Datelike;
            let cur = today.weekday().num_days_from_monday() as i64;
            let target = *idx as i64;
            let base_diff = (target - cur).rem_euclid(7);
            // 显式 "next" 语义：至少 7 天之后（即使 base_diff > 0）
            let diff = if base_diff == 0 { 7 } else { base_diff + 7 };
            Some((today + Duration::days(diff)).and_time(nine_am))
        }
        EditDuePreset::PlusMinutes(n) => Some(now + Duration::minutes(*n as i64)),
        EditDuePreset::PlusHours(n) => Some(now + Duration::hours(*n as i64)),
        EditDuePreset::PlusDays(n) => {
            Some((today + Duration::days(*n as i64)).and_time(nine_am))
        }
        EditDuePreset::Clear => None,
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
    /// `/find_speech <keyword>` —— 在 speech_history.log 内搜 keyword
    /// （case-insensitive 子串），返回最多 8 条命中（ts MM-DD HH:MM +
    /// 命中点附近 60 字 snippet）。与 /find（标题 / 描述）/
    /// /find_in_detail（detail.md 内容）同搜索族 — 本命令搜 pet 说过
    /// 的话。空 keyword → handler 走 missing-argument hint。
    FindSpeech { keyword: String },
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
    /// `/show <title>` —— 显示指定任务的 raw_description（含全部 markers）
    /// + detail.md 内容预览（前 300 字符），让 owner 在 TG 端 audit 单条
    /// 任务详情不必回桌面。空 title 走 missing-arg；title resolve 三层
    /// （数字 index → fuzzy → 错误候选）与 /done /cancel /edit 同源。
    Show { title: String },
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
            TgCommand::FindSpeech { .. } => "find_speech",
            TgCommand::Blocked => "blocked",
            TgCommand::Forks { .. } => "forks",
            TgCommand::BlockedBy { .. } => "blocked_by",
            TgCommand::Snoozed => "snoozed",
            TgCommand::Mute { .. } => "mute",
            TgCommand::SleepUntil { .. } => "sleep_until",
            TgCommand::Note { .. } => "note",
            TgCommand::Digest { .. } => "digest",
            TgCommand::Edit { .. } => "edit",
            TgCommand::SwapPriority { .. } => "swap_priority",
            TgCommand::Reflect { .. } => "reflect",
            TgCommand::Due { .. } => "due",
            TgCommand::Show { .. } => "show",
            TgCommand::Timeline { .. } => "timeline",
            TgCommand::Now => "now",
            TgCommand::LastSpeech => "last_speech",
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
            | TgCommand::FindInDetail { keyword: title }
            | TgCommand::FindSpeech { keyword: title }
            | TgCommand::Tag { name: title }
            | TgCommand::TagsFor { title }
            | TgCommand::Touch { title }
            | TgCommand::Note { text: title }
            | TgCommand::Reflect { text: title }
            | TgCommand::Show { title }
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
            | TgCommand::SleepUntil { raw: title } => title.as_str(),
            TgCommand::Edit { title, .. } => title.as_str(),
            TgCommand::SwapPriority { title_a, .. } => title_a.as_str(),
            TgCommand::Task { title, .. } => title.as_str(),
            TgCommand::Tasks
            | TgCommand::Pinned
            | TgCommand::PinnedDue
            | TgCommand::Silenced
            | TgCommand::Markers
            | TgCommand::Tags
            | TgCommand::Stats
            | TgCommand::Buckets
            | TgCommand::Mood
            | TgCommand::Whoami
            | TgCommand::Today
            | TgCommand::Recent { .. }
            | TgCommand::OldestN { .. }
            | TgCommand::ActiveRecent { .. }
            | TgCommand::Blocked
            | TgCommand::Snoozed
            | TgCommand::Mute { .. }
            | TgCommand::Digest { .. }
            | TgCommand::FeedbackHistory { .. }
            | TgCommand::SilentAll { .. }
            | TgCommand::Alarms { .. }
            | TgCommand::RecentChats { .. }
            | TgCommand::Due { .. }
            | TgCommand::Now
            | TgCommand::LastSpeech
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
            ("find_speech", "Search speech_history.log by keyword — pet's past proactive utterances; complements /find / /find_in_detail"),
            ("show", "Show full raw description (with markers) + detail.md preview of a task"),
            ("timeline", "Timeline view: each butler_history event for a task with state-change markers"),
            ("blocked", "List active tasks blocked by [blockedBy: …] with their unresolved blockers"),
            ("forks", "Reverse: list active tasks that reference [blockedBy: <this>] — unlock impact audit"),
            ("blocked_by", "Focused: list unresolved blockers that <title> is waiting on"),
            ("snoozed", "List tasks currently in [snooze: …] with time until wake"),
            ("mute", "Mute proactive for N minutes (default 30; 0 to clear)"),
            ("sleep_until", "Mute proactive until an absolute local time (HH:MM) — complements /mute N relative minutes"),
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
            ("find_speech", "按 keyword 搜 speech_history.log — 搜 pet 说过的话（含命中点 snippet，至多 8 条）"),
            ("show", "显单条任务完整 raw description（含 markers）+ detail.md 预览"),
            ("timeline", "时间线：列出某任务历经的所有 butler_history 事件 + 当时的状态变化 markers"),
            ("blocked", "列出被 [blockedBy: …] 锁住的活跃 task + 仍未解决的 blocker 标题"),
            ("forks", "反向 audit：列引用 [blockedBy: <this>] 的活跃 task — 这条解锁后会让谁动起来"),
            ("blocked_by", "单条 audit：列 title 仍未解决的 blockers（与 /forks 反向 — 我在等谁）"),
            ("snoozed", "列出当前在 [snooze: …] 中的 task + 还多久醒"),
            ("mute", "临时静音 proactive N 分钟（默认 30；0 = 解除）"),
            ("sleep_until", "静音到指定本地时刻（HH:MM）— 与 /mute N 互补；目标时刻 ≤ now 时落明日同时"),
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
        // `/find_in_detail <keyword>`：所有 arg 作 keyword（含空格保留）。
        // 空 keyword 由 handler 走 missing-argument。snake_case 命名避开
        // dash drift-defense（与 /oldest_n / /active_recent 同模板）。
        "find_in_detail" => Some(TgCommand::FindInDetail { keyword: title }),
        // `/find_speech <keyword>`：所有 arg 作 keyword（含空格保留）。
        // 空 keyword 由 handler 走 missing-argument。snake_case 命名避
        // 开 dash drift-defense（与 /find_in_detail 同模板）。
        "find_speech" => Some(TgCommand::FindSpeech { keyword: title }),
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
/// 长版说明书 topic 名单。`format_help_for_topic("all", ...)` 与 drift-
/// defense 测试都引用这同一矩阵，保两侧不漂移。顺序也用于"all"渲染时
/// 段次序 — 高频创建命令在前、兜底 help 在末，与 /help 全表同节奏。
pub const ALL_HELP_TOPICS: &[&str] = &[
    "task", "tasks", "stats", "buckets", "done", "cancel", "retry", "snooze",
    "unsnooze", "pin", "unpin", "pinned", "pinned_due", "silent", "unsilent",
    "silenced", "silent_all", "markers", "tags", "tag", "tags_for", "touch", "mood",
    "whoami", "today", "today_done", "yesterday", "streak", "now", "last_speech",
    "aware", "here",
    "last", "random", "sleep", "sleep_until", "quick", "due", "recent", "oldest_n", "active_recent", "recent_chats",
    "digest", "alarms", "edit", "edit_due", "pri", "promote", "demote", "swap_priority",
    "reflect", "feedback", "feedback_history", "transient",
    "cancel_all_error", "promote_all_p7", "touch_all_p7", "pin_all_p7", "consolidate_now", "find", "find_in_detail", "find_speech", "show", "timeline",
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
        "last_speech" => "🗣 /last_speech\n\n用法：显 pet 最近一条主动开口（speech_history.log 末条），含 ts + 文本 + 相对时间「N 分前 / N 小时前 / N 天前」。无参；多余尾部忽略。\n\n与 ChatMini 顶部「⏱ pet 沉默 N 分」chip 对偶 — 那个显沉默时长触发关心；本命令显具体最近说了啥（原话 + 从那时起的分钟数）。\n\n输出格式：\n  🗣 pet 最近主动开口 · MM-DD HH:MM（N 分前）：\n  「<text 前 N 字 cap>」\n\n空 history（pet 还没主动开口过 / 刚 reset） → 友好兜底。\n\n示例：\n  /last_speech\n\n相关：/aware（pet 当前感知）；/here（owner 信号 snapshot）；/feedback_history（pet 接收的反馈）。",
        "last" => "🆕 /last\n\n用法：显本聊天派单中最近 created_at 的一条 task — title + status emoji + 相对创建时间 + raw_description 前 200 字符预览。无参。owner 想「我刚 /task 创的那条对不对」闪查时用 — 不必走 /tasks 全表扫。\n\n示例：\n  /last\n\n相关：/show <title>（看完整 raw + detail）；/recent（最近 N 条 done）；/tasks（全状态清单）。",
        "random" => "🎲 /random\n\n用法：从本聊天派单的 active 任务（pending / error）里随机抽 1 条让宠物推荐 — 给 owner「选择困难」/「不知道先做哪个」时让 pet 决定下一步。无参；多次调用会得到不同 task。无 active 任务时给兜底文案。\n\n示例：\n  /random\n\n相关：/tasks（看全清单）；/blocked（被锁住的）；/today（今日到期）。",
        "sleep" => "🌙 /sleep\n\n用法：一键让宠物 mute proactive 8 小时 + 友好「晚安」reply。无参。比手敲 `/mute 480` 更直觉 — owner 睡前 / 长会议 / 想 deep work 时一句话搞定。\n\n示例：\n  /sleep\n\n相关：/mute [N]（精确控制 N 分钟）；/sleep_until HH:MM（静音到指定时刻）；/mute 0（立刻解除静音）。",
        "sleep_until" => "🌙 /sleep_until <HH:MM>\n\n用法：静音 proactive 到指定本地时刻（HH:MM 24 小时制；H:MM / HH / H 也接受 — 单数字视为 HH:00）。与 /mute N（相对分钟数）/ /sleep（固定 8h）互补 — owner 想「安静到 8 点」/「安静到中午」更自然。\n\n语义：目标时刻 ≤ now → 落到明日同时刻（owner 凌晨 1 点说「到 8 点」视为今早 8:00，非次日 8:00 反直觉）；clamp 1..=10080 分钟（≤ 7 天）。\n\n示例：\n  /sleep_until 8:00    （静音到 8 点）\n  /sleep_until 22:30   （静音到 22:30）\n  /sleep_until 14      （静音到下午 2 点）\n\n相关：/mute [N]（相对分钟数）；/sleep（一键 8h）；/mute 0（立刻解除）。",
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
        "find_in_detail" => "🔬 /find_in_detail <keyword>\n\n用法：搜本聊天派单的 detail.md 内容（case-insensitive 子串），至多 8 条命中。与 /find（仅扫标题 / raw_description）互补 — pet 在 detail.md 写过相关进度 / 复盘但标题没体现时本命令命中。\n\n输出格式：\n  🔬 命中「<kw>」N 条（detail.md 内容搜索）：\n  🟢 <title>\n     …<snippet 含 keyword 60 字 context>…\n  ⚠️ <title>\n     …\n  ...\n\nsnippet 取 keyword 命中点附近 60 字 context；超长 + …。\n\n示例：\n  /find_in_detail rebase\n  /find_in_detail TODO\n  /find_in_detail 决策\n\n注：每次命令读所有派单的 detail.md（IO 较重），不必过分频繁。owner 想「快速过一遍标题」走 /find；想「我笔记里写过 X」走本命令。\n\n相关：/find（扫标题 + 描述）；/find_speech（搜 pet 说过的话）；/show <title>（看单条 raw + detail 预览）；/timeline（看历史变化）。",
        "find_speech" => "🗣 /find_speech <keyword>\n\n用法：在 speech_history.log 内搜 keyword（case-insensitive 子串），返回最多 8 条命中（ts MM-DD HH:MM + 命中点附近 60 字 snippet）。与 /find / /find_in_detail 同搜索族但 scope 是 **pet 说过的话**。\n\n输出格式：\n  🗣 speech 命中「<kw>」N 条：\n  · MM-DD HH:MM · …<snippet>…\n  · MM-DD HH:MM · …\n  ...\n\nsnippet 取 keyword 命中点附近 60 字 context；超长前后 + …。\n\n场景：owner 想「pet 之前提过 X 吗」/「pet 上次怎么说这件事」 audit — 比 /last_speech（仅最近 1 条）覆盖更广。\n\n示例：\n  /find_speech 周报\n  /find_speech rebase\n  /find_speech 心情\n\n相关：/last_speech（最近一条主动开口）；/find（任务标题 / 描述）；/find_in_detail（detail.md 内容）；/recent_chats（user ↔ pet 对话）。",
        "show" => "🔬 /show <title>\n\n用法：显单条任务完整 raw description（含 [task pri=...] / [every:] / [pinned] 等所有 markers）+ detail.md 内容预览（前 300 字符）。Title resolve 与 /done / /cancel 同三层（数字 index → fuzzy → 错误候选）。\n\n示例：\n  /show 整理 Downloads\n  /show 1  （/tasks 输出第 1 条）\n\n相关：/find 搜任务；/edit 改 description；/tasks 看清单。让 owner 在 TG 端 audit 任务详情不必回桌面。",
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
        "/find_speech <keyword>  —  搜 speech_history.log（pet 说过的话，含命中点 snippet，至多 8 条；与 /last_speech 单条对偶）".to_string(),
        "/show <title>  —  显单条任务完整 raw description（含 markers）+ detail.md 预览".to_string(),
        "/timeline <title>  —  时间线：列 butler_history 事件 + 当时状态变化 markers（[done]/[error:]/[snooze:]/[result:] 等）".to_string(),
        "/blocked  —  列出被 [blockedBy: …] 锁住的活跃 task + 仍未解决的 blocker".to_string(),
        "/forks <title>  —  反向 audit：哪些活跃 task 在 [blockedBy: <this>]（这条解锁会让谁动起来）".to_string(),
        "/blocked_by <title>  —  单条 audit：title 仍未解决的 blockers（与 /forks 反向 — 我在等谁）".to_string(),
        "/snoozed  —  列出当前在 [snooze: …] 中的 task + 还多久醒".to_string(),
        "/mute [N]  —  临时静音 proactive N 分钟（默认 30；0 = 解除）".to_string(),
        "/sleep_until <HH:MM>  —  静音到指定本地时刻（HH:MM；目标 ≤ now → 明日同时；与 /mute N 互补）".to_string(),
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

/// `/find_speech <keyword>` 命令回复文案。pure：handler 已扫
/// speech_history.log 全文 + 按行 case-insensitive 子串过滤 + 抽 snippet。
/// 本函数仅做字符串拼装。
///
/// 输入 hits 是 (ts_local_HH_MM, snippet) tuple — handler 已把 RFC3339
/// ts 转 `MM-DD HH:MM` 本地串。空 keyword → usage hint；无 hits → 友好
/// 兜底；有 hits → 拼 list 最多 8 条 + overflow hint。
pub fn format_find_speech_reply(
    hits: &[(String, String)],
    keyword: &str,
) -> String {
    let kw = keyword.trim();
    if kw.is_empty() {
        return "🗣 用法：/find_speech <keyword>\n按 keyword 搜 speech_history.log（pet 说过的话），返回最多 8 条命中（含 ts + snippet）。\n例：/find_speech 周报 / /find_speech rebase / /find_speech 心情\n\n与 /last_speech（最近 1 条）/ /find / /find_in_detail 互补。".to_string();
    }
    if hits.is_empty() {
        return format!(
            "🗣 speech_history 内没有命中「{}」的话。\n试更短的关键词；或 /last_speech 看最近一条；或 /recent_chats 看对话往返。",
            kw
        );
    }
    let cap = 8;
    let shown = &hits[..hits.len().min(cap)];
    let mut out = format!(
        "🗣 speech 命中「{}」{} 条：",
        kw,
        hits.len()
    );
    for (ts, snippet) in shown {
        out.push_str(&format!("\n· {} · …{}…", ts, snippet));
    }
    if hits.len() > cap {
        out.push_str(&format!(
            "\n…还有 {} 条命中（关键词太宽？试更精确的词）",
            hits.len() - cap
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

/// `/timeline` 中一行事件条目。`markers` 是该事件 snippet 内扫出的「状态
/// 变化」marker token 列表（保 `[done]` / `[result: 已发送]` 等完整原文），
/// 顺序保持 snippet 内出现顺序。
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TimelineEntry {
    pub timestamp: String,
    pub action: String,
    pub markers: Vec<String>,
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
            out.push(TimelineEntry {
                timestamp: ts.clone(),
                action: action.clone(),
                markers,
            });
        }
        prev_signature = Some(signature);
    }
    out
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
        let emoji = match e.action.to_ascii_lowercase().as_str() {
            "create" => "📝",
            "delete" => "🗑️",
            _ => "✏️",
        };
        let ts_short = format_timeline_ts(&e.timestamp);
        let body = if e.action.to_ascii_lowercase() == "create" {
            "创建".to_string()
        } else if e.action.to_ascii_lowercase() == "delete" {
            "删除".to_string()
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
            "unsnooze", "pin", "unpin", "pinned", "pinned_due", "silent",
            "unsilent", "silenced", "markers", "tags", "mood", "whoami", "today",
            "today_done", "yesterday", "streak", "now", "last_speech", "last", "random", "sleep", "sleep_until", "quick",
            "due", "recent", "oldest_n", "active_recent", "digest", "edit", "pri", "swap_priority", "promote", "demote",
            "reflect", "feedback", "feedback_history", "transient",
            "silent_all", "alarms", "recent_chats", "aware", "here",
            "tag", "tags_for", "touch", "edit_due", "cancel_all_error", "promote_all_p7", "touch_all_p7", "find", "find_in_detail", "find_speech",
            "show", "timeline", "blocked", "forks", "blocked_by", "snoozed", "reset",
            "version", "help", "pin_all_p7", "consolidate_now",
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
            "task", "tasks", "cancel", "retry", "done", "stats", "buckets", "mood",
            "whoami", "snooze", "unsnooze", "pin", "unpin", "pinned",
            "pinned_due", "today",
            "today_done", "yesterday", "streak", "now", "last_speech", "last", "random", "sleep", "sleep_until", "quick",
            "due", "edit", "edit_due", "pri", "swap_priority", "promote", "demote", "reflect",
            "feedback", "feedback_history", "transient", "silent_all",
            "alarms", "recent_chats", "aware", "here", "cancel_all_error",
            "promote_all_p7", "touch_all_p7", "pin_all_p7", "consolidate_now", "active_recent", "find_in_detail", "find_speech", "show", "timeline", "forks", "blocked_by",
            "tags", "tag", "tags_for", "touch", "reset", "version", "help",
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

    // -------- /buckets parse + format --------

    #[test]
    fn buckets_parses_no_args() {
        assert_eq!(parse_tg_command("/buckets"), Some(TgCommand::Buckets));
        assert_eq!(parse_tg_command("/buckets  "), Some(TgCommand::Buckets));
        assert_eq!(
            parse_tg_command("/buckets now"),
            Some(TgCommand::Buckets)
        );
        assert_eq!(parse_tg_command("/BUCKETS"), Some(TgCommand::Buckets));
    }

    #[test]
    fn buckets_reply_empty_shows_friendly_fallback() {
        let s = format_buckets_reply(&[]);
        assert!(s.contains("🎯"), "{s}");
        assert!(s.contains("无 active task"), "{s}");
        assert!(s.contains("/tasks"), "alt hint /tasks: {s}");
    }

    #[test]
    fn buckets_reply_groups_priorities_into_5_bands() {
        // 测试覆盖所有 5 桶：P0 / P1-2 / P3-4 / P5-6 / P7+
        let v_p0 = view("p0", 0, None, TaskStatus::Pending, None);
        let v_p1 = view("p1", 1, None, TaskStatus::Pending, None);
        let v_p2 = view("p2", 2, None, TaskStatus::Pending, None);
        let v_p3 = view("p3", 3, None, TaskStatus::Pending, None);
        let v_p4 = view("p4", 4, None, TaskStatus::Pending, None);
        let v_p5 = view("p5", 5, None, TaskStatus::Pending, None);
        let v_p6 = view("p6", 6, None, TaskStatus::Pending, None);
        let v_p7 = view("p7", 7, None, TaskStatus::Pending, None);
        let v_p8 = view("p8", 8, None, TaskStatus::Pending, None);
        let v_p9 = view("p9", 9, None, TaskStatus::Pending, None);
        let s = format_buckets_reply(&[
            v_p0, v_p1, v_p2, v_p3, v_p4, v_p5, v_p6, v_p7, v_p8, v_p9,
        ]);
        assert!(s.contains("10 条 active"), "total count: {s}");
        assert!(s.contains("P7+: 3"), "p7+ bucket: {s}");
        assert!(s.contains("P5-6: 2"), "p5-6 bucket: {s}");
        assert!(s.contains("P3-4: 2"), "p3-4 bucket: {s}");
        assert!(s.contains("P1-2: 2"), "p1-2 bucket: {s}");
        assert!(s.contains("P0: 1"), "p0 bucket: {s}");
    }

    #[test]
    fn buckets_reply_filters_to_active_only() {
        // done / cancelled 不计入 active
        let pending = view("p", 5, None, TaskStatus::Pending, None);
        let error = view("e", 3, None, TaskStatus::Error, Some("err"));
        let done = view("d", 7, None, TaskStatus::Done, Some("ok"));
        let cancelled = view("c", 5, None, TaskStatus::Cancelled, Some("drop"));
        let s = format_buckets_reply(&[pending, error, done, cancelled]);
        assert!(s.contains("2 条 active"), "active count: {s}");
        assert!(s.contains("P5-6: 1"), "{s}");
        assert!(s.contains("P3-4: 1"), "{s}");
        // done P7 不应入桶
        assert!(s.contains("P7+: 0"), "done excluded from P7+: {s}");
    }

    #[test]
    fn buckets_reply_p7_plus_includes_high_priorities() {
        // P7 / P8 / P9 都进 P7+ 桶
        let v7 = view("p7", 7, None, TaskStatus::Pending, None);
        let v8 = view("p8", 8, None, TaskStatus::Pending, None);
        let v9 = view("p9", 9, None, TaskStatus::Pending, None);
        let s = format_buckets_reply(&[v7, v8, v9]);
        assert!(s.contains("P7+: 3"), "{s}");
        assert!(s.contains("P5-6: 0"), "{s}");
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

    // -------- /pinned_due parse + format --------

    #[test]
    fn pinned_due_parses_no_args() {
        assert_eq!(
            parse_tg_command("/pinned_due"),
            Some(TgCommand::PinnedDue)
        );
        assert_eq!(
            parse_tg_command("/pinned_due  "),
            Some(TgCommand::PinnedDue)
        );
        assert_eq!(
            parse_tg_command("/pinned_due now"),
            Some(TgCommand::PinnedDue)
        );
        assert_eq!(
            parse_tg_command("/PINNED_DUE"),
            Some(TgCommand::PinnedDue)
        );
    }

    #[test]
    fn pinned_due_reply_empty_shows_friendly_fallback() {
        let s = format_pinned_due_reply(&[]);
        assert!(s.contains("🔥"), "{s}");
        assert!(s.contains("暂无"), "{s}");
        assert!(s.contains("/pinned"), "hint /pinned alt: {s}");
        assert!(s.contains("/due"), "hint /due alt: {s}");
    }

    #[test]
    fn pinned_due_reply_filters_active_pinned_and_due() {
        // 所有四个 filter 维度的测试矩阵：
        // - pinned + due + Pending → 应入
        // - pinned + due + Error → 应入
        // - pinned + due + Done → 应排除（非 active）
        // - pinned + no due + Pending → 应排除
        // - no pin + due + Pending → 应排除
        let mut a = view("活 pinned due", 3, Some("2026-05-20T10:00"), TaskStatus::Pending, None);
        a.pinned = true;
        let mut b = view("错 pinned due", 5, Some("2026-05-21T10:00"), TaskStatus::Error, Some("err"));
        b.pinned = true;
        let mut c = view("成 pinned due", 3, Some("2026-05-19T10:00"), TaskStatus::Done, Some("ok"));
        c.pinned = true;
        let mut d = view("pinned no due", 7, None, TaskStatus::Pending, None);
        d.pinned = true;
        let e = view("not pinned but due", 3, Some("2026-05-18T10:00"), TaskStatus::Pending, None);
        let s = format_pinned_due_reply(&[a, b, c, d, e]);
        assert!(s.contains("活 pinned due"), "active pending kept: {s}");
        assert!(s.contains("错 pinned due"), "active error kept: {s}");
        assert!(!s.contains("成 pinned due"), "done excluded: {s}");
        assert!(!s.contains("pinned no due"), "no-due excluded: {s}");
        assert!(!s.contains("not pinned but due"), "not-pinned excluded: {s}");
        assert!(s.contains("共 2 条"), "count reflects filter: {s}");
    }

    #[test]
    fn pinned_due_reply_sorts_by_due_asc() {
        // 最近到期在前
        let mut late = view("晚", 3, Some("2026-05-25T18:00"), TaskStatus::Pending, None);
        late.pinned = true;
        let mut early = view("早", 3, Some("2026-05-18T08:00"), TaskStatus::Pending, None);
        early.pinned = true;
        let mut mid = view("中", 3, Some("2026-05-20T14:00"), TaskStatus::Pending, None);
        mid.pinned = true;
        let s = format_pinned_due_reply(&[late, mid, early]);
        let idx_early = s.find("早").expect("早 in output");
        let idx_mid = s.find("中").expect("中 in output");
        let idx_late = s.find("晚").expect("晚 in output");
        assert!(idx_early < idx_mid, "早 before 中: {s}");
        assert!(idx_mid < idx_late, "中 before 晚: {s}");
    }

    #[test]
    fn pinned_due_reply_header_mentions_asc_sort_for_owner_clarity() {
        // header 应明确 "按 due 升序"让 owner 不必猜顺序
        let mut a = view("t", 3, Some("2026-05-20T10:00"), TaskStatus::Pending, None);
        a.pinned = true;
        let s = format_pinned_due_reply(&[a]);
        assert!(s.contains("按 due 升序"), "header explains sort: {s}");
    }

    #[test]
    fn pinned_due_reply_only_pinned_no_due_falls_back_empty() {
        // 边缘：所有 pinned task 都无 due → 兜底「暂无」（与彻底空 views 同）
        let mut a = view("pinned only", 7, None, TaskStatus::Pending, None);
        a.pinned = true;
        let s = format_pinned_due_reply(&[a]);
        assert!(s.contains("暂无"), "{s}");
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

    // -------- /oldest_n parse + format --------

    fn fixed_now_for_oldest(
        y: i32,
        mo: u32,
        d: u32,
        h: u32,
        mi: u32,
    ) -> chrono::DateTime<chrono::FixedOffset> {
        chrono::DateTime::parse_from_rfc3339(&format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:00+08:00",
            y, mo, d, h, mi
        ))
        .unwrap()
    }

    #[test]
    fn oldest_n_parses_default_5_when_no_arg() {
        assert_eq!(
            parse_tg_command("/oldest_n"),
            Some(TgCommand::OldestN { n: 5 })
        );
        assert_eq!(
            parse_tg_command("/oldest_n  "),
            Some(TgCommand::OldestN { n: 5 })
        );
    }

    #[test]
    fn oldest_n_parses_explicit_n() {
        assert_eq!(
            parse_tg_command("/oldest_n 10"),
            Some(TgCommand::OldestN { n: 10 })
        );
        assert_eq!(
            parse_tg_command("/oldest_n 1"),
            Some(TgCommand::OldestN { n: 1 })
        );
        // clamp 1..=20
        assert_eq!(
            parse_tg_command("/oldest_n 50"),
            Some(TgCommand::OldestN { n: 20 })
        );
        assert_eq!(
            parse_tg_command("/oldest_n 0"),
            Some(TgCommand::OldestN { n: 1 })
        );
    }

    #[test]
    fn oldest_n_reply_empty_pending_says_no_records() {
        let now = fixed_now_for_oldest(2026, 5, 17, 18, 0);
        let s = format_oldest_n_reply(&[], 5, now);
        assert!(s.contains("✨"), "{s}");
        assert!(s.contains("暂无 pending"), "{s}");
        assert!(s.contains("/tasks"), "alt hint: {s}");
        assert!(s.contains("/recent"), "alt hint /recent: {s}");
    }

    #[test]
    fn oldest_n_reply_orders_by_created_at_asc() {
        let now = fixed_now_for_oldest(2026, 5, 17, 18, 0);
        // 三条 pending，created_at 不同
        let mut old = view("最老的活", 3, None, TaskStatus::Pending, None);
        old.created_at = "2026-04-01T10:00:00+08:00".to_string();
        let mut mid = view("中间的", 3, None, TaskStatus::Pending, None);
        mid.created_at = "2026-05-10T10:00:00+08:00".to_string();
        let mut newest = view("最新的", 3, None, TaskStatus::Pending, None);
        newest.created_at = "2026-05-16T10:00:00+08:00".to_string();
        let s = format_oldest_n_reply(&vec![newest, mid, old], 3, now);
        let idx_old = s.find("最老的活").expect("最老 in output");
        let idx_mid = s.find("中间的").expect("中间 in output");
        let idx_new = s.find("最新的").expect("最新 in output");
        assert!(idx_old < idx_mid, "最老 before 中间: {s}");
        assert!(idx_mid < idx_new, "中间 before 最新: {s}");
    }

    #[test]
    fn oldest_n_reply_includes_age_label() {
        let now = fixed_now_for_oldest(2026, 5, 17, 18, 0);
        // 46 天前创建
        let mut old = view("挂了 46 天的活", 3, None, TaskStatus::Pending, None);
        old.created_at = "2026-04-01T18:00:00+08:00".to_string();
        let s = format_oldest_n_reply(&vec![old], 1, now);
        assert!(s.contains("46 天前"), "age label: {s}");
    }

    #[test]
    fn oldest_n_reply_skips_non_pending() {
        let now = fixed_now_for_oldest(2026, 5, 17, 18, 0);
        let mut pending = view("活的", 3, None, TaskStatus::Pending, None);
        pending.created_at = "2026-05-01T10:00:00+08:00".to_string();
        let mut error = view("出错的", 3, None, TaskStatus::Error, Some("err"));
        error.created_at = "2026-04-15T10:00:00+08:00".to_string();
        let mut done = view("做完的", 3, None, TaskStatus::Done, Some("ok"));
        done.created_at = "2026-04-01T10:00:00+08:00".to_string();
        let mut cancelled = view("取消的", 3, None, TaskStatus::Cancelled, Some("drop"));
        cancelled.created_at = "2026-03-15T10:00:00+08:00".to_string();
        let s = format_oldest_n_reply(&vec![pending, error, done, cancelled], 5, now);
        assert!(s.contains("活的"), "pending kept: {s}");
        assert!(!s.contains("出错的"), "error excluded: {s}");
        assert!(!s.contains("做完的"), "done excluded: {s}");
        assert!(!s.contains("取消的"), "cancelled excluded: {s}");
        assert!(s.contains("共 1"), "count reflects filter: {s}");
    }

    // -------- /active_recent parse + format --------

    fn fixed_now_for_active_recent(
        y: i32,
        mo: u32,
        d: u32,
        h: u32,
        mi: u32,
    ) -> chrono::DateTime<chrono::FixedOffset> {
        chrono::DateTime::parse_from_rfc3339(&format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:00+08:00",
            y, mo, d, h, mi
        ))
        .unwrap()
    }

    #[test]
    fn active_recent_parses_default_5_when_no_arg() {
        assert_eq!(
            parse_tg_command("/active_recent"),
            Some(TgCommand::ActiveRecent { n: 5 })
        );
        assert_eq!(
            parse_tg_command("/active_recent  "),
            Some(TgCommand::ActiveRecent { n: 5 })
        );
    }

    #[test]
    fn active_recent_parses_explicit_n() {
        assert_eq!(
            parse_tg_command("/active_recent 10"),
            Some(TgCommand::ActiveRecent { n: 10 })
        );
        assert_eq!(
            parse_tg_command("/active_recent 1"),
            Some(TgCommand::ActiveRecent { n: 1 })
        );
    }

    #[test]
    fn active_recent_clamps_to_1_20_range() {
        assert_eq!(
            parse_tg_command("/active_recent 0"),
            Some(TgCommand::ActiveRecent { n: 1 })
        );
        assert_eq!(
            parse_tg_command("/active_recent 21"),
            Some(TgCommand::ActiveRecent { n: 20 })
        );
        assert_eq!(
            parse_tg_command("/active_recent 9999"),
            Some(TgCommand::ActiveRecent { n: 20 })
        );
    }

    #[test]
    fn active_recent_garbage_arg_falls_back_to_default() {
        assert_eq!(
            parse_tg_command("/active_recent abc"),
            Some(TgCommand::ActiveRecent { n: 5 })
        );
    }

    #[test]
    fn active_recent_reply_empty_active_says_no_records() {
        let now = fixed_now_for_active_recent(2026, 5, 17, 18, 0);
        let s = format_active_recent_reply(&[], 5, now);
        assert!(s.contains("✨"), "active_recent reply: {s}");
        assert!(s.contains("暂无 active 任务"), "active_recent reply: {s}");
    }

    #[test]
    fn active_recent_reply_orders_by_created_at_desc() {
        let now = fixed_now_for_active_recent(2026, 5, 17, 18, 0);
        let mut old = view("最老的活", 0, None, TaskStatus::Pending, None);
        old.created_at = "2026-05-10T10:00:00+08:00".to_string();
        let mut newest = view("最新的活", 0, None, TaskStatus::Pending, None);
        newest.created_at = "2026-05-17T11:00:00+08:00".to_string();
        let mut mid = view("中间的活", 0, None, TaskStatus::Pending, None);
        mid.created_at = "2026-05-15T09:00:00+08:00".to_string();
        let s = format_active_recent_reply(&vec![old, newest, mid], 3, now);
        let pos_newest = s.find("最新的活").expect("newest present");
        let pos_mid = s.find("中间的活").expect("mid present");
        let pos_old = s.find("最老的活").expect("old present");
        assert!(pos_newest < pos_mid, "order: {s}");
        assert!(pos_mid < pos_old, "order: {s}");
        assert!(s.contains("共 3"), "header: {s}");
        assert!(s.contains("05-17 11:00"), "ts format: {s}");
    }

    #[test]
    fn active_recent_reply_includes_pending_and_error_skips_terminal() {
        let now = fixed_now_for_active_recent(2026, 5, 17, 18, 0);
        let mut pending = view("活的", 3, None, TaskStatus::Pending, None);
        pending.created_at = "2026-05-15T10:00:00+08:00".to_string();
        let mut error = view("出错的", 3, None, TaskStatus::Error, Some("err"));
        error.created_at = "2026-05-14T10:00:00+08:00".to_string();
        let mut done = view("做完的", 3, None, TaskStatus::Done, Some("ok"));
        done.created_at = "2026-05-16T10:00:00+08:00".to_string();
        let mut cancelled = view("取消的", 3, None, TaskStatus::Cancelled, Some("drop"));
        cancelled.created_at = "2026-05-16T11:00:00+08:00".to_string();
        let s = format_active_recent_reply(&vec![pending, error, done, cancelled], 5, now);
        assert!(s.contains("活的"), "pending kept: {s}");
        assert!(s.contains("出错的"), "error kept: {s}");
        assert!(!s.contains("做完的"), "done excluded: {s}");
        assert!(!s.contains("取消的"), "cancelled excluded: {s}");
        assert!(s.contains("共 2"), "count reflects filter: {s}");
        // status emoji 区分
        assert!(s.contains("🟢"), "pending emoji: {s}");
        assert!(s.contains("⚠️"), "error emoji: {s}");
    }

    #[test]
    fn active_recent_reply_truncates_to_n_with_overflow_hint() {
        let now = fixed_now_for_active_recent(2026, 5, 17, 18, 0);
        let mut views = Vec::new();
        for i in 0..7 {
            let mut v = view(&format!("塞 {}", i), 0, None, TaskStatus::Pending, None);
            // 升序 created_at → 索引 6 最新（formatter 倒序后在前）
            v.created_at = format!("2026-05-0{}T10:00:00+08:00", i + 1);
            views.push(v);
        }
        let s = format_active_recent_reply(&views, 3, now);
        assert!(s.contains("最近 3 条新建 active（共 7"), "header: {s}");
        // 倒序应显 塞 6 / 塞 5 / 塞 4
        assert!(s.contains("塞 6"), "{s}");
        assert!(s.contains("塞 5"), "{s}");
        assert!(s.contains("塞 4"), "{s}");
        assert!(!s.contains("塞 3"), "{s}");
        assert!(s.contains("还有 4 条更早创建 active"), "overflow hint: {s}");
    }

    #[test]
    fn active_recent_reply_includes_age_label() {
        let now = fixed_now_for_active_recent(2026, 5, 17, 18, 0);
        let mut old = view("挂 7 天的活", 3, None, TaskStatus::Pending, None);
        old.created_at = "2026-05-10T18:00:00+08:00".to_string();
        let s = format_active_recent_reply(&vec![old], 1, now);
        assert!(s.contains("7 天前"), "age label: {s}");
    }

    #[test]
    fn oldest_n_reply_truncates_to_n_with_overflow_hint() {
        let now = fixed_now_for_oldest(2026, 5, 17, 18, 0);
        let mut views = Vec::new();
        for i in 0..7 {
            let mut v = view(&format!("挂 {}", i), 0, None, TaskStatus::Pending, None);
            // 升序 created_at → 索引 0 最老
            v.created_at = format!("2026-04-0{}T10:00:00+08:00", i + 1);
            views.push(v);
        }
        let s = format_oldest_n_reply(&views, 3, now);
        assert!(s.contains("最老 3 条 pending（共 7"), "header: {s}");
        // 升序应显 挂 0 / 挂 1 / 挂 2
        assert!(s.contains("挂 0"), "{s}");
        assert!(s.contains("挂 1"), "{s}");
        assert!(s.contains("挂 2"), "{s}");
        assert!(!s.contains("挂 3"), "{s}");
        assert!(s.contains("还有 4 条更老"), "overflow hint: {s}");
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

    // -------- /find_in_detail parse + format + snippet --------

    #[test]
    fn find_in_detail_parses_keyword_arg() {
        assert_eq!(
            parse_tg_command("/find_in_detail rebase"),
            Some(TgCommand::FindInDetail {
                keyword: "rebase".to_string()
            })
        );
        assert_eq!(
            parse_tg_command("/find_in_detail 整理 桌面"),
            Some(TgCommand::FindInDetail {
                keyword: "整理 桌面".to_string()
            })
        );
    }

    #[test]
    fn find_in_detail_empty_keyword_returns_usage_hint() {
        let s = format_find_in_detail_reply(&[], "");
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/find_in_detail <keyword>"), "{s}");
    }

    #[test]
    fn find_in_detail_no_hits_shows_keyword_in_reply() {
        let s = format_find_in_detail_reply(&[], "周报");
        assert!(s.contains("没有 task 的 detail.md 含「周报」"), "{s}");
        assert!(s.contains("/find"), "推荐 /find 互补: {s}");
    }

    #[test]
    fn find_in_detail_reply_renders_hits_with_emoji_and_snippet() {
        let hits = vec![
            FindInDetailHit {
                title: "重构 router",
                status: TaskStatus::Pending,
                snippet: "前 30 字 rebase 后 30 字".to_string(),
            },
            FindInDetailHit {
                title: "fix login",
                status: TaskStatus::Error,
                snippet: "step 3: rebase before deploy".to_string(),
            },
        ];
        let s = format_find_in_detail_reply(&hits, "rebase");
        assert!(s.contains("🔬 命中「rebase」2 条"), "{s}");
        assert!(s.contains("🟢 重构 router"), "{s}");
        assert!(s.contains("⚠️ fix login"), "{s}");
        assert!(
            s.contains("…前 30 字 rebase 后 30 字…"),
            "snippet 双 ellipsis: {s}",
        );
    }

    #[test]
    fn find_in_detail_caps_at_8_with_overflow_hint() {
        let snippets: Vec<String> = (0..10).map(|i| format!("snip {}", i)).collect();
        let hits: Vec<FindInDetailHit> = (0..10)
            .map(|i| FindInDetailHit {
                title: match i {
                    0 => "t-0",
                    1 => "t-1",
                    2 => "t-2",
                    3 => "t-3",
                    4 => "t-4",
                    5 => "t-5",
                    6 => "t-6",
                    7 => "t-7",
                    8 => "t-8",
                    _ => "t-9",
                },
                status: TaskStatus::Pending,
                snippet: snippets[i].clone(),
            })
            .collect();
        let s = format_find_in_detail_reply(&hits, "kw");
        assert!(s.contains("命中「kw」10 条"), "{s}");
        // 前 8 条显
        assert!(s.contains("t-0"), "{s}");
        assert!(s.contains("t-7"), "{s}");
        assert!(!s.contains("t-8"), "{s}");
        assert!(s.contains("还有 2 条命中"), "overflow hint: {s}");
    }

    #[test]
    fn extract_snippet_returns_none_when_no_hit() {
        let s = extract_find_in_detail_snippet("hello world", "foobar");
        assert!(s.is_none());
    }

    #[test]
    fn extract_snippet_returns_none_when_empty_kw() {
        let s = extract_find_in_detail_snippet("hello world", "");
        assert!(s.is_none());
    }

    #[test]
    fn extract_snippet_case_insensitive_basic() {
        let s = extract_find_in_detail_snippet("Hello WORLD haha", "world");
        assert!(s.is_some());
        let snippet = s.unwrap();
        assert!(snippet.to_lowercase().contains("world"), "{snippet}");
    }

    #[test]
    fn extract_snippet_flattens_newlines() {
        let s = extract_find_in_detail_snippet(
            "line one\n\nline two with KEYWORD here\nline three",
            "keyword",
        );
        let snippet = s.expect("hit");
        assert!(!snippet.contains('\n'), "no newline: {snippet}");
        assert!(snippet.contains("KEYWORD"), "{snippet}");
    }

    #[test]
    fn extract_snippet_context_window_30_chars_each_side() {
        // 100-char text with hit at idx 50；window = ±30 chars 应覆盖 idx 20..80
        let text: String = "a".repeat(50) + "MATCH" + &"b".repeat(50);
        let snippet =
            extract_find_in_detail_snippet(&text, "match").expect("hit");
        // snippet 长度 ~60 chars (30 a + 5 MATCH + 25 b 因 hit 在 char 50)
        // 关键是 MATCH 在内
        assert!(snippet.contains("MATCH"), "{snippet}");
        // 不应含全部 100 chars
        assert!(snippet.len() < text.len(), "{snippet}");
    }

    // -------- /blocked parse + format --------

    // -------- /find_speech parse + format --------

    #[test]
    fn find_speech_parses_keyword_arg() {
        assert_eq!(
            parse_tg_command("/find_speech 周报"),
            Some(TgCommand::FindSpeech {
                keyword: "周报".to_string()
            })
        );
        assert_eq!(
            parse_tg_command("/find_speech 多 字 关键词"),
            Some(TgCommand::FindSpeech {
                keyword: "多 字 关键词".to_string()
            })
        );
    }

    #[test]
    fn find_speech_empty_keyword_returns_usage_hint() {
        let s = format_find_speech_reply(&[], "");
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/find_speech <keyword>"), "{s}");
    }

    #[test]
    fn find_speech_no_hits_shows_keyword_in_reply() {
        let s = format_find_speech_reply(&[], "周报");
        assert!(s.contains("speech_history 内没有命中"), "{s}");
        assert!(s.contains("周报"), "echoes keyword: {s}");
    }

    #[test]
    fn find_speech_reply_renders_hits_with_ts_and_snippet() {
        let hits = vec![
            (
                "05-17 14:30".to_string(),
                "想到要写周报突然慌了".to_string(),
            ),
            (
                "05-16 09:15".to_string(),
                "周报这事每周都要做".to_string(),
            ),
        ];
        let s = format_find_speech_reply(&hits, "周报");
        assert!(s.contains("🗣 speech 命中「周报」2 条"), "{s}");
        assert!(s.contains("05-17 14:30"), "{s}");
        assert!(s.contains("想到要写周报突然慌了"), "{s}");
        assert!(s.contains("05-16 09:15"), "{s}");
        assert!(s.contains("周报这事每周都要做"), "{s}");
        // 双 ellipsis snippet 框
        assert!(s.contains("…想到要写周报突然慌了…"), "{s}");
    }

    #[test]
    fn find_speech_caps_at_8_with_overflow_hint() {
        let hits: Vec<(String, String)> = (0..12)
            .map(|i| (format!("05-{:02} 10:00", i + 1), format!("snip {}", i)))
            .collect();
        let s = format_find_speech_reply(&hits, "snip");
        assert!(s.contains("speech 命中「snip」12 条"), "{s}");
        // 前 8 应显
        assert!(s.contains("snip 0"), "{s}");
        assert!(s.contains("snip 7"), "{s}");
        assert!(!s.contains("snip 8"), "{s}");
        // 溢出 hint
        assert!(s.contains("还有 4 条命中"), "{s}");
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

    // -------- /forks parse + format --------

    #[test]
    fn forks_parser_takes_all_args_as_title() {
        assert_eq!(
            parse_tg_command("/forks 整理 Downloads"),
            Some(TgCommand::Forks {
                title: "整理 Downloads".to_string()
            })
        );
    }

    #[test]
    fn forks_parser_empty_title_parses() {
        assert_eq!(
            parse_tg_command("/forks"),
            Some(TgCommand::Forks {
                title: String::new()
            })
        );
    }

    #[test]
    fn forks_reply_empty_target_shows_usage() {
        let s = format_forks_reply(&[], "");
        assert!(s.contains("用法"), "{s}");
    }

    #[test]
    fn forks_reply_no_dependents_friendly_leaf_node() {
        let a = view("整理 Downloads", 0, None, TaskStatus::Pending, None);
        let s = format_forks_reply(&[a], "整理 Downloads");
        assert!(s.contains("不会影响"), "{s}");
        assert!(s.contains("叶子节点"), "{s}");
    }

    #[test]
    fn forks_reply_lists_active_dependents() {
        let target = view("调研竞品", 0, None, TaskStatus::Pending, None);
        let mut a = view("写决策文档", 0, None, TaskStatus::Pending, None);
        a.blocked_by = vec!["调研竞品".to_string()];
        let mut b = view("整理报告", 0, None, TaskStatus::Pending, None);
        b.blocked_by = vec!["调研竞品".to_string()];
        let s = format_forks_reply(&[target, a, b], "调研竞品");
        assert!(s.contains("解锁「调研竞品」会松开 2 条 task"), "{s}");
        assert!(s.contains("🟢 写决策文档"), "{s}");
        assert!(s.contains("🟢 整理报告"), "{s}");
    }

    #[test]
    fn forks_reply_skips_inactive_dependents() {
        // done / cancelled 的依赖方不算"会被松开"— 它们已经超出 active 池
        let target = view("调研", 0, None, TaskStatus::Pending, None);
        let mut a = view("写报告", 0, None, TaskStatus::Done, None);
        a.blocked_by = vec!["调研".to_string()];
        let mut b = view("整理", 0, None, TaskStatus::Cancelled, None);
        b.blocked_by = vec!["调研".to_string()];
        let s = format_forks_reply(&[target, a, b], "调研");
        assert!(s.contains("不会影响"), "{s}");
    }

    #[test]
    fn forks_reply_error_state_dependents_also_count() {
        // error task 的依赖也算"会被松开"— retry 时同样需要 blocker 解锁
        let target = view("调研", 0, None, TaskStatus::Pending, None);
        let mut a = view("写报告", 0, None, TaskStatus::Error, Some("LLM 拒"));
        a.blocked_by = vec!["调研".to_string()];
        let s = format_forks_reply(&[target, a], "调研");
        assert!(s.contains("⚠️ 写报告"), "{s}");
        assert!(s.contains("会松开 1 条"), "{s}");
    }

    #[test]
    fn forks_reply_trim_matches_target_title() {
        // blocked_by 元素 trim 后字面比较 — 让 description 内的空白容忍
        let target = view("调研", 0, None, TaskStatus::Pending, None);
        let mut a = view("写报告", 0, None, TaskStatus::Pending, None);
        a.blocked_by = vec!["  调研  ".to_string()]; // 含周围空白
        let s = format_forks_reply(&[target, a], "调研");
        assert!(s.contains("写报告"), "trim should match: {s}");
    }

    #[test]
    fn forks_reply_target_with_no_self_self_loop_safe() {
        // 即使 target 引用了 target（自环不该有但防御性）— 也不会让 target
        // 把自己列进 forks 行。验：自己不会出现在 "会松开" 列表里。
        let mut target = view("调研", 0, None, TaskStatus::Pending, None);
        target.blocked_by = vec!["调研".to_string()];
        let s = format_forks_reply(&[target], "调研");
        // 一致逻辑：调研在 blocked_by 含 "调研" → 它会被列入 forks（虽然
        // 是自环也算"会被松开"）。这条测试就是 pin 这种边缘情况的当前
        // 行为 — 不静默 broken。
        assert!(s.contains("会松开 1 条"), "self-loop counted (current behavior): {s}");
    }

    // -------- /blocked_by parse + format --------

    #[test]
    fn blocked_by_parser_takes_all_args_as_title() {
        assert_eq!(
            parse_tg_command("/blocked_by 写决策文档"),
            Some(TgCommand::BlockedBy {
                title: "写决策文档".to_string()
            })
        );
    }

    #[test]
    fn blocked_by_parser_empty_title_parses() {
        assert_eq!(
            parse_tg_command("/blocked_by"),
            Some(TgCommand::BlockedBy {
                title: String::new()
            })
        );
    }

    #[test]
    fn blocked_by_reply_empty_target_shows_usage() {
        let s = format_blocked_by_reply(&[], "");
        assert!(s.contains("用法"), "{s}");
    }

    #[test]
    fn blocked_by_reply_target_not_found() {
        let v = view("别人", 0, None, TaskStatus::Pending, None);
        let s = format_blocked_by_reply(&[v], "不存在");
        assert!(s.contains("没找到"), "{s}");
    }

    #[test]
    fn blocked_by_reply_target_no_blockers_marker() {
        let v = view("孤立 task", 0, None, TaskStatus::Pending, None);
        let s = format_blocked_by_reply(&[v], "孤立 task");
        assert!(s.contains("无 `[blockedBy"), "{s}");
        assert!(s.contains("不在等任何"), "{s}");
    }

    #[test]
    fn blocked_by_reply_all_blockers_resolved() {
        // target 的 blockers 已全 done / cancelled → ✨ 提示
        let mut target = view("写决策文档", 0, None, TaskStatus::Pending, None);
        target.blocked_by = vec!["调研".to_string(), "审批".to_string()];
        let done_blocker = view("调研", 0, None, TaskStatus::Done, Some("ok"));
        let cancelled_blocker = view("审批", 0, None, TaskStatus::Cancelled, Some("drop"));
        let s = format_blocked_by_reply(
            &[target, done_blocker, cancelled_blocker],
            "写决策文档",
        );
        assert!(s.contains("✨"), "{s}");
        assert!(s.contains("均已解决"), "{s}");
        assert!(s.contains("2 条 blocker"), "total count: {s}");
    }

    #[test]
    fn blocked_by_reply_lists_unresolved_with_icons() {
        let mut target = view("写决策文档", 0, None, TaskStatus::Pending, None);
        target.blocked_by =
            vec!["调研".to_string(), "等审批".to_string(), "done blocker".to_string()];
        let pending_blocker = view("调研", 0, None, TaskStatus::Pending, None);
        let error_blocker = view("等审批", 0, None, TaskStatus::Error, Some("err"));
        let done_blocker = view("done blocker", 0, None, TaskStatus::Done, Some("ok"));
        let s = format_blocked_by_reply(
            &[target, pending_blocker, error_blocker, done_blocker],
            "写决策文档",
        );
        assert!(s.contains("被 2 条 blocker 卡住"), "active count: {s}");
        assert!(s.contains("共 3 条 marker"), "total marker count: {s}");
        assert!(s.contains("🟢 调研"), "pending icon: {s}");
        assert!(s.contains("⚠️ 等审批"), "error icon: {s}");
        // done blocker 不渲（被视作已解决）
        assert!(!s.contains("done blocker"), "done excluded: {s}");
    }

    #[test]
    fn blocked_by_reply_trim_matches_blocker_titles() {
        let mut target = view("a", 0, None, TaskStatus::Pending, None);
        target.blocked_by = vec!["  调研  ".to_string()]; // 含周围空白
        let blocker = view("调研", 0, None, TaskStatus::Pending, None);
        let s = format_blocked_by_reply(&[target, blocker], "a");
        assert!(s.contains("被 1 条 blocker 卡住"), "trim matched: {s}");
        assert!(s.contains("调研"), "{s}");
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

    // -------- /sleep_until parse + format --------

    #[test]
    fn sleep_until_parses_raw_arg() {
        assert_eq!(
            parse_tg_command("/sleep_until 8:00"),
            Some(TgCommand::SleepUntil {
                raw: "8:00".to_string(),
            })
        );
        assert_eq!(
            parse_tg_command("/sleep_until 22:30"),
            Some(TgCommand::SleepUntil {
                raw: "22:30".to_string(),
            })
        );
    }

    #[test]
    fn sleep_until_parses_empty_raw() {
        assert_eq!(
            parse_tg_command("/sleep_until"),
            Some(TgCommand::SleepUntil {
                raw: String::new(),
            })
        );
    }

    #[test]
    fn parse_sleep_until_time_accepts_hh_mm() {
        assert_eq!(parse_sleep_until_time("8:00"), Some((8, 0)));
        assert_eq!(parse_sleep_until_time("22:30"), Some((22, 30)));
        assert_eq!(parse_sleep_until_time("00:00"), Some((0, 0)));
        assert_eq!(parse_sleep_until_time("23:59"), Some((23, 59)));
    }

    #[test]
    fn parse_sleep_until_time_accepts_single_digit_hour_as_hh00() {
        assert_eq!(parse_sleep_until_time("8"), Some((8, 0)));
        assert_eq!(parse_sleep_until_time("14"), Some((14, 0)));
        assert_eq!(parse_sleep_until_time("0"), Some((0, 0)));
    }

    #[test]
    fn parse_sleep_until_time_rejects_out_of_range() {
        assert_eq!(parse_sleep_until_time("24:00"), None);
        assert_eq!(parse_sleep_until_time("12:60"), None);
        assert_eq!(parse_sleep_until_time("99"), None);
    }

    #[test]
    fn parse_sleep_until_time_rejects_garbage() {
        assert_eq!(parse_sleep_until_time(""), None);
        assert_eq!(parse_sleep_until_time("abc"), None);
        assert_eq!(parse_sleep_until_time("8:ab"), None);
        assert_eq!(parse_sleep_until_time("ab:30"), None);
    }

    #[test]
    fn parse_sleep_until_time_trims_whitespace() {
        assert_eq!(parse_sleep_until_time("  8:00  "), Some((8, 0)));
        assert_eq!(parse_sleep_until_time("\t14\t"), Some((14, 0)));
    }

    #[test]
    fn format_sleep_until_reply_empty_raw_shows_usage() {
        let s = format_sleep_until_reply("", None, 0, None, false);
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/sleep_until <HH:MM>"), "{s}");
    }

    #[test]
    fn format_sleep_until_reply_invalid_time_shows_error() {
        let s = format_sleep_until_reply("abc", None, 0, None, false);
        assert!(s.contains("不是合法时刻"), "{s}");
        assert!(s.contains("abc"), "echoes input: {s}");
    }

    #[test]
    fn format_sleep_until_reply_success_shows_target_and_duration() {
        let until = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 22, 30, 0)
            .unwrap();
        let s = format_sleep_until_reply(
            "22:30",
            Some((22, 30)),
            90,
            Some(until),
            false,
        );
        assert!(s.contains("🌙"), "{s}");
        assert!(s.contains("22:30"), "target: {s}");
        assert!(s.contains("1 小时 30 分钟"), "duration: {s}");
        assert!(!s.contains("明日同时刻"), "no cross-midnight hint: {s}");
    }

    #[test]
    fn format_sleep_until_reply_crosses_midnight_adds_hint() {
        let until = chrono::Local
            .with_ymd_and_hms(2026, 5, 18, 8, 0, 0)
            .unwrap();
        let s = format_sleep_until_reply(
            "8:00",
            Some((8, 0)),
            240,
            Some(until),
            true,
        );
        assert!(s.contains("明日同时刻"), "cross-midnight hint: {s}");
        assert!(s.contains("8:00") || s.contains("08:00"), "target: {s}");
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

    // -------- /help all (long-form) --------

    #[test]
    fn help_all_parses_to_help_with_topic_all() {
        assert_eq!(
            parse_tg_command("/help all"),
            Some(TgCommand::Help {
                topic: Some("all".to_string())
            })
        );
    }

    #[test]
    fn help_all_returns_long_version_with_header() {
        let s = format_help_for_topic("all", &[]);
        assert!(s.contains("长版说明书"), "should have all-version header: head=({})", &s[..s.len().min(80)]);
        // 长版本应远比短版长
        let short = format_help_for_topic("", &[]);
        assert!(s.len() > short.len() * 2, "all-version should be much longer than full-help: short={}, all={}", short.len(), s.len());
    }

    #[test]
    fn help_all_concatenates_all_listed_topic_bodies() {
        let s = format_help_for_topic("all", &[]);
        // 抽样命令的详细文案 anchors 应该都在
        for sample in ["📝 /task <title>", "🚫 /cancel <title>", "🏷 /tags", "🔬 /show <title>", "💤 /snooze <title> [preset]"] {
            assert!(s.contains(sample), "missing anchor for {sample} in all-version");
        }
    }

    #[test]
    fn help_all_uses_separator_between_entries() {
        let s = format_help_for_topic("all", &[]);
        // 多个 \n\n────\n\n 分隔（至少 N-1 个，N = ALL_HELP_TOPICS.len()）
        let sep_count = s.matches("────").count();
        assert!(
            sep_count >= ALL_HELP_TOPICS.len() - 1,
            "expected at least {} separators, got {}",
            ALL_HELP_TOPICS.len() - 1,
            sep_count,
        );
    }

    #[test]
    fn help_all_topic_list_includes_all_real_commands() {
        // ALL_HELP_TOPICS 与 format_help_for_topic 单条详情表保 sync
        // —— 每个 ALL_HELP_TOPICS 项都应能拿到非空 detail
        for name in ALL_HELP_TOPICS {
            let s = format_help_for_topic(name, &[]);
            assert!(s.contains("用法"), "{name} in ALL_HELP_TOPICS missing detail: {s}");
        }
    }

    // -------- /tags parse + format --------

    #[test]
    fn tags_parses_no_args() {
        assert_eq!(parse_tg_command("/tags"), Some(TgCommand::Tags));
        // 多余尾部忽略（与 /markers / /today 同模板）
        assert_eq!(parse_tg_command("/tags now"), Some(TgCommand::Tags));
    }

    fn view_with_tags(title: &str, tags: &[&str]) -> TaskView {
        let mut v = view(title, 3, None, TaskStatus::Pending, None);
        v.tags = tags.iter().map(|s| s.to_string()).collect();
        v
    }

    #[test]
    fn tags_reply_empty_views_shows_friendly_hint() {
        let s = format_tags_reply(&[]);
        assert!(s.contains("暂无 #tag"), "should show empty hint: {s}");
        assert!(s.contains("0 条任务无 tag"), "should report untagged 0: {s}");
    }

    #[test]
    fn tags_reply_lists_tags_sorted_by_count_desc() {
        let views = vec![
            view_with_tags("a", &["健身"]),
            view_with_tags("b", &["健身", "晨练"]),
            view_with_tags("c", &["健身"]),
            view_with_tags("d", &["读书"]),
            view_with_tags("e", &["读书"]),
        ];
        let s = format_tags_reply(&views);
        // 健身 3 / 读书 2 / 晨练 1 — 按 count desc
        let idx_jian = s.find("#健身 ×3").expect("健身 line");
        let idx_du = s.find("#读书 ×2").expect("读书 line");
        let idx_chen = s.find("#晨练 ×1").expect("晨练 line");
        assert!(idx_jian < idx_du, "健身 should come before 读书: {s}");
        assert!(idx_du < idx_chen, "读书 should come before 晨练: {s}");
    }

    #[test]
    fn tags_reply_excludes_untagged_from_tag_counts() {
        let views = vec![
            view_with_tags("a", &["健身"]),
            view_with_tags("b", &[]),
            view_with_tags("c", &[]),
        ];
        let s = format_tags_reply(&views);
        assert!(s.contains("#健身 ×1"), "{s}");
        // untagged 数也出现
        assert!(s.contains("无 #tag 任务：2 条"), "{s}");
    }

    #[test]
    fn tags_reply_caps_at_top_15_and_shows_overflow() {
        // 制造 20 个 tag，每个 1 条
        let mut views = Vec::new();
        for i in 0..20 {
            // 用前缀确保字典序与生成顺序一致让"哪 15 个被列出"有确定性
            // (count tied → name asc fallback by BTreeMap; sort_by 用 stable)
            views.push(view_with_tags(&format!("t{i}"), &[Box::leak(format!("tag{i:02}").into_boxed_str()) as &str]));
        }
        let s = format_tags_reply(&views);
        assert!(s.contains("共 20 个 tag"), "{s}");
        assert!(s.contains("…还有 5 个 tag"), "should show overflow hint: {s}");
    }

    #[test]
    fn tags_reply_skips_empty_tag_strings() {
        // 防御 trim 后空 tag（不应进矩阵）
        let mut v = view("a", 3, None, TaskStatus::Pending, None);
        v.tags = vec!["  ".to_string(), "健身".to_string()];
        let s = format_tags_reply(&[v]);
        assert!(s.contains("#健身 ×1"), "{s}");
        assert!(s.contains("共 1 个 tag"), "empty tag should be skipped: {s}");
    }

    #[test]
    fn tags_reply_counts_across_all_statuses() {
        // /tags 是 audit 维度，done / cancelled 也该计入（owner 想知道
        // "我用过哪些 tag"，不局限活跃）
        let active = view_with_tags("a", &["健身"]);
        let mut done = view_with_tags("b", &["健身"]);
        done.status = TaskStatus::Done;
        let mut cancelled = view_with_tags("c", &["健身"]);
        cancelled.status = TaskStatus::Cancelled;
        let s = format_tags_reply(&[active, done, cancelled]);
        assert!(s.contains("#健身 ×3"), "should count all statuses: {s}");
    }

    // -------- /help search <kw> --------

    #[test]
    fn help_search_empty_shows_usage_hint() {
        let s = format_help_search("", &[]);
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/help search <keyword>"), "{s}");
        assert!(s.contains("case-insensitive"), "{s}");
    }

    #[test]
    fn help_search_matches_command_name() {
        let s = format_help_search("done", &[]);
        assert!(s.contains("/done"), "should match command name: {s}");
    }

    #[test]
    fn help_search_matches_chinese_in_description() {
        // "复制" is in many command detail / descriptions
        let s = format_help_search("复制", &[]);
        assert!(s.contains("命中"), "{s}");
        // 应该不止 1 条命中（含"复制"的命令多个）
        assert!(s.matches("·").count() >= 1);
    }

    #[test]
    fn help_search_case_insensitive() {
        let lower = format_help_search("done", &[]);
        let upper = format_help_search("DONE", &[]);
        let mixed = format_help_search("Done", &[]);
        // 三种 case 应命中数量一致（同 keyword 不同大小写）
        let count_lower = lower.matches("·").count();
        let count_upper = upper.matches("·").count();
        let count_mixed = mixed.matches("·").count();
        assert_eq!(count_lower, count_upper);
        assert_eq!(count_lower, count_mixed);
    }

    #[test]
    fn help_search_no_match_shows_friendly_hint() {
        let s = format_help_search("zzzzzzzznoinmatchatall", &[]);
        assert!(s.contains("未在任何命令中命中"), "{s}");
        assert!(s.contains("/help all"), "should hint alternatives: {s}");
    }

    #[test]
    fn help_search_via_format_help_for_topic() {
        // /help search <kw> 入口由 format_help_for_topic 顶层 dispatch
        let s = format_help_for_topic("search done", &[]);
        assert!(s.contains("/done"), "dispatch via topic: {s}");
    }

    #[test]
    fn help_search_via_topic_bare_search_shows_usage() {
        // 仅 "search" 无 kw → usage hint
        let s = format_help_for_topic("search", &[]);
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/help search <keyword>"), "{s}");
    }

    #[test]
    fn help_search_via_topic_with_slash_prefix() {
        // "/search done" 前缀 `/` 由 trim_start_matches('/') 去掉后变成 "search done"
        let s = format_help_for_topic("/search done", &[]);
        assert!(s.contains("/done"), "{s}");
    }

    // -------- /cancel_all_error parse + format --------

    #[test]
    fn cancel_all_error_parses_without_confirm_token() {
        assert_eq!(
            parse_tg_command("/cancel_all_error"),
            Some(TgCommand::CancelAllError { confirmed: false })
        );
        // 任何非 "confirm" 尾部都视作未确认
        assert_eq!(
            parse_tg_command("/cancel_all_error yes"),
            Some(TgCommand::CancelAllError { confirmed: false })
        );
    }

    #[test]
    fn cancel_all_error_parses_with_confirm_token() {
        assert_eq!(
            parse_tg_command("/cancel_all_error confirm"),
            Some(TgCommand::CancelAllError { confirmed: true })
        );
        // case-insensitive
        assert_eq!(
            parse_tg_command("/cancel_all_error CONFIRM"),
            Some(TgCommand::CancelAllError { confirmed: true })
        );
        assert_eq!(
            parse_tg_command("/cancel_all_error Confirm"),
            Some(TgCommand::CancelAllError { confirmed: true })
        );
    }

    #[test]
    fn cancel_all_error_reply_unconfirmed_with_zero_errors() {
        let s = format_cancel_all_error_reply(false, 0, 0, 0);
        assert!(s.contains("暂无 error"), "{s}");
        assert!(s.contains("无需批量 cancel"), "{s}");
    }

    #[test]
    fn cancel_all_error_reply_unconfirmed_with_errors_demands_confirm() {
        let s = format_cancel_all_error_reply(false, 5, 0, 0);
        assert!(s.contains("5 条 error"), "{s}");
        assert!(s.contains("必须带 `confirm`"), "{s}");
        assert!(
            s.contains("/cancel_all_error confirm"),
            "should show exact command: {s}"
        );
    }

    #[test]
    fn cancel_all_error_reply_confirmed_zero_total_shows_idle() {
        let s = format_cancel_all_error_reply(true, 0, 0, 0);
        assert!(s.contains("暂无 error"), "{s}");
    }

    #[test]
    fn cancel_all_error_reply_confirmed_all_ok() {
        let s = format_cancel_all_error_reply(true, 3, 3, 0);
        assert!(s.contains("已批量 cancel 3"), "{s}");
        assert!(!s.contains("⚠️"), "no warning when all ok: {s}");
        assert!(s.contains("/tasks"), "should hint follow-up: {s}");
        assert!(s.contains("/retry"), "{s}");
    }

    #[test]
    fn cancel_all_error_reply_confirmed_partial_failure() {
        let s = format_cancel_all_error_reply(true, 5, 3, 2);
        assert!(s.contains("已批量 cancel 3"), "{s}");
        assert!(s.contains("2 条 cancel 失败"), "{s}");
        assert!(s.contains("⚠️"), "warning present: {s}");
    }

    // -------- /promote_all_p7 parse + format --------

    #[test]
    fn promote_all_p7_parses_no_arg_as_unconfirmed() {
        assert_eq!(
            parse_tg_command("/promote_all_p7"),
            Some(TgCommand::PromoteAllP7 { confirmed: false })
        );
        // 多余 trailing 空格不算 confirm
        assert_eq!(
            parse_tg_command("/promote_all_p7    "),
            Some(TgCommand::PromoteAllP7 { confirmed: false })
        );
    }

    #[test]
    fn promote_all_p7_parses_confirm_token() {
        assert_eq!(
            parse_tg_command("/promote_all_p7 confirm"),
            Some(TgCommand::PromoteAllP7 { confirmed: true })
        );
        // case-insensitive
        assert_eq!(
            parse_tg_command("/promote_all_p7 CONFIRM"),
            Some(TgCommand::PromoteAllP7 { confirmed: true })
        );
        assert_eq!(
            parse_tg_command("/promote_all_p7 Confirm"),
            Some(TgCommand::PromoteAllP7 { confirmed: true })
        );
    }

    #[test]
    fn promote_all_p7_other_trailing_not_confirmed() {
        // owner 误敲 yes / ok 等不该被当作 confirm
        assert_eq!(
            parse_tg_command("/promote_all_p7 yes"),
            Some(TgCommand::PromoteAllP7 { confirmed: false })
        );
        assert_eq!(
            parse_tg_command("/promote_all_p7 ok"),
            Some(TgCommand::PromoteAllP7 { confirmed: false })
        );
    }

    #[test]
    fn promote_all_p7_reply_unconfirmed_with_zero_targets() {
        let s = format_promote_all_p7_reply(false, 0, 0, 0);
        assert!(s.contains("暂无可升级"), "{s}");
        assert!(!s.contains("必须带"), "no scolding when nothing to do: {s}");
    }

    #[test]
    fn promote_all_p7_reply_unconfirmed_with_targets_demands_confirm() {
        let s = format_promote_all_p7_reply(false, 5, 0, 0);
        assert!(s.contains("5 条 active"), "preview count: {s}");
        assert!(s.contains("confirm"), "demands confirm token: {s}");
        assert!(s.contains("/promote_all_p7 confirm"), "shows full command: {s}");
    }

    #[test]
    fn promote_all_p7_reply_confirmed_zero_changes_shows_idle() {
        let s = format_promote_all_p7_reply(true, 0, 0, 0);
        assert!(s.contains("暂无可升级"), "{s}");
        assert!(s.contains("✨"), "{s}");
    }

    #[test]
    fn promote_all_p7_reply_confirmed_all_ok() {
        let s = format_promote_all_p7_reply(true, 3, 3, 0);
        assert!(s.contains("已批量升 3 条"), "{s}");
        assert!(s.contains("clamp 7"), "should mention clamp: {s}");
        assert!(!s.contains("⚠️"), "no warning when all ok: {s}");
        assert!(s.contains("/tasks"), "{s}");
        assert!(s.contains("/pri"), "fine-tune hint: {s}");
    }

    #[test]
    fn promote_all_p7_reply_confirmed_partial_failure() {
        let s = format_promote_all_p7_reply(true, 5, 3, 2);
        assert!(s.contains("已批量升 3 条"), "{s}");
        assert!(s.contains("2 条升级失败"), "{s}");
        assert!(s.contains("⚠️"), "warning present: {s}");
    }

    // -------- /touch_all_p7 parse + format --------

    #[test]
    fn touch_all_p7_parses_no_arg_as_unconfirmed() {
        assert_eq!(
            parse_tg_command("/touch_all_p7"),
            Some(TgCommand::TouchAllP7 { confirmed: false })
        );
    }

    #[test]
    fn touch_all_p7_parses_confirm_case_insensitive() {
        assert_eq!(
            parse_tg_command("/touch_all_p7 confirm"),
            Some(TgCommand::TouchAllP7 { confirmed: true })
        );
        assert_eq!(
            parse_tg_command("/touch_all_p7 CONFIRM"),
            Some(TgCommand::TouchAllP7 { confirmed: true })
        );
    }

    #[test]
    fn touch_all_p7_other_trailing_not_confirmed() {
        assert_eq!(
            parse_tg_command("/touch_all_p7 yes"),
            Some(TgCommand::TouchAllP7 { confirmed: false })
        );
    }

    #[test]
    fn touch_all_p7_reply_unconfirmed_with_zero_targets() {
        let s = format_touch_all_p7_reply(false, 0, 0, 0);
        assert!(s.contains("暂无 P7+"), "{s}");
        assert!(!s.contains("必须带"), "no scolding when nothing to do: {s}");
    }

    #[test]
    fn touch_all_p7_reply_unconfirmed_with_targets_demands_confirm() {
        let s = format_touch_all_p7_reply(false, 4, 0, 0);
        assert!(s.contains("4 条 P7+"), "preview count: {s}");
        assert!(s.contains("confirm"), "demands confirm: {s}");
        assert!(s.contains("/touch_all_p7 confirm"), "{s}");
    }

    #[test]
    fn touch_all_p7_reply_confirmed_all_ok() {
        let s = format_touch_all_p7_reply(true, 3, 3, 0);
        assert!(s.contains("已批量 touch 3 条"), "{s}");
        assert!(s.contains("挂着的高优重新冒头"), "explains effect: {s}");
        assert!(!s.contains("⚠️"), "no warning: {s}");
        assert!(s.contains("/tasks"), "{s}");
        assert!(s.contains("/oldest_n"), "{s}");
    }

    #[test]
    fn touch_all_p7_reply_confirmed_partial_failure() {
        let s = format_touch_all_p7_reply(true, 5, 3, 2);
        assert!(s.contains("已批量 touch 3 条"), "{s}");
        assert!(s.contains("2 条 touch 失败"), "{s}");
        assert!(s.contains("⚠️"), "{s}");
    }

    // -------- /pin_all_p7 parse + format --------

    #[test]
    fn pin_all_p7_parses_no_arg_as_unconfirmed() {
        assert_eq!(
            parse_tg_command("/pin_all_p7"),
            Some(TgCommand::PinAllP7 { confirmed: false })
        );
    }

    #[test]
    fn pin_all_p7_parses_confirm_case_insensitive() {
        assert_eq!(
            parse_tg_command("/pin_all_p7 confirm"),
            Some(TgCommand::PinAllP7 { confirmed: true })
        );
        assert_eq!(
            parse_tg_command("/pin_all_p7 CONFIRM"),
            Some(TgCommand::PinAllP7 { confirmed: true })
        );
    }

    #[test]
    fn pin_all_p7_other_trailing_not_confirmed() {
        assert_eq!(
            parse_tg_command("/pin_all_p7 yes"),
            Some(TgCommand::PinAllP7 { confirmed: false })
        );
    }

    #[test]
    fn pin_all_p7_reply_unconfirmed_with_zero_targets() {
        let s = format_pin_all_p7_reply(false, 0, 0, 0);
        assert!(s.contains("暂无可 pin"), "{s}");
        assert!(!s.contains("必须带"), "no scolding when nothing to do: {s}");
    }

    #[test]
    fn pin_all_p7_reply_unconfirmed_with_targets_demands_confirm() {
        let s = format_pin_all_p7_reply(false, 6, 0, 0);
        assert!(s.contains("6 条 P7+"), "preview count: {s}");
        assert!(s.contains("confirm"), "demands confirm: {s}");
        assert!(s.contains("/pin_all_p7 confirm"), "{s}");
    }

    #[test]
    fn pin_all_p7_reply_confirmed_all_ok() {
        let s = format_pin_all_p7_reply(true, 4, 4, 0);
        assert!(s.contains("已批量 pin 4 条"), "{s}");
        assert!(s.contains("[pinned] marker"), "explains effect: {s}");
        assert!(!s.contains("⚠️"), "no warning: {s}");
        assert!(s.contains("/pinned"), "follow-up hint: {s}");
    }

    #[test]
    fn pin_all_p7_reply_confirmed_partial_failure() {
        let s = format_pin_all_p7_reply(true, 5, 3, 2);
        assert!(s.contains("已批量 pin 3 条"), "{s}");
        assert!(s.contains("2 条 pin 失败"), "{s}");
        assert!(s.contains("⚠️"), "{s}");
    }

    #[test]
    fn pin_all_p7_reply_confirmed_zero_changes_idle() {
        // 全部已 pinned 时 candidates=0 → ok=0 + err=0 → 空闲态文案
        let s = format_pin_all_p7_reply(true, 0, 0, 0);
        assert!(s.contains("无可 pin"), "idle: {s}");
        assert!(s.contains("✨"), "{s}");
    }

    // -------- /consolidate_now parse + format --------

    #[test]
    fn consolidate_now_parses_no_arg_as_unconfirmed() {
        assert_eq!(
            parse_tg_command("/consolidate_now"),
            Some(TgCommand::ConsolidateNow { confirmed: false })
        );
    }

    #[test]
    fn consolidate_now_parses_confirm_case_insensitive() {
        assert_eq!(
            parse_tg_command("/consolidate_now confirm"),
            Some(TgCommand::ConsolidateNow { confirmed: true })
        );
        assert_eq!(
            parse_tg_command("/consolidate_now CONFIRM"),
            Some(TgCommand::ConsolidateNow { confirmed: true })
        );
    }

    #[test]
    fn consolidate_now_other_trailing_not_confirmed() {
        assert_eq!(
            parse_tg_command("/consolidate_now yes"),
            Some(TgCommand::ConsolidateNow { confirmed: false })
        );
    }

    #[test]
    fn format_consolidate_now_unconfirmed_shows_usage_hint() {
        let s = format_consolidate_now_reply(false, None);
        assert!(s.contains("🧹"), "{s}");
        assert!(s.contains("/consolidate_now confirm"), "{s}");
        assert!(s.contains("LLM-heavy"), "warns LLM cost: {s}");
    }

    #[test]
    fn format_consolidate_now_confirmed_ok_shows_summary() {
        let s = format_consolidate_now_reply(
            true,
            Some(Ok(
                "Consolidation finished in 12345 ms (50 items at start)".to_string()
            )),
        );
        assert!(s.contains("🧹"), "{s}");
        assert!(s.contains("Consolidation finished in 12345 ms"), "{s}");
    }

    #[test]
    fn format_consolidate_now_confirmed_user_cancel_friendly() {
        let s = format_consolidate_now_reply(true, Some(Err("用户取消".to_string())));
        assert!(s.contains("🧹"), "{s}");
        assert!(s.contains("已取消整理"), "{s}");
    }

    #[test]
    fn format_consolidate_now_confirmed_error_shows_reason() {
        let s = format_consolidate_now_reply(
            true,
            Some(Err("LLM call failed: timeout".to_string())),
        );
        assert!(s.contains("失败"), "{s}");
        assert!(s.contains("timeout"), "shows reason: {s}");
    }

    // -------- /demote parse + format --------

    #[test]
    fn demote_parses_title() {
        assert_eq!(
            parse_tg_command("/demote 写周报"),
            Some(TgCommand::Demote {
                title: "写周报".to_string()
            })
        );
    }

    #[test]
    fn demote_parses_empty_title() {
        assert_eq!(
            parse_tg_command("/demote"),
            Some(TgCommand::Demote {
                title: String::new()
            })
        );
    }

    #[test]
    fn demote_reply_empty_title_shows_usage() {
        let s = format_demote_reply("", Some(3), Ok(()));
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/demote <title>"), "{s}");
        assert!(s.contains("-1"), "{s}");
        assert!(s.contains("/pri"), "{s}");
        assert!(s.contains("/promote"), "{s}");
    }

    #[test]
    fn demote_reply_p0_shows_already_min() {
        let s = format_demote_reply("idea 抽屉", Some(0), Ok(()));
        assert!(s.contains("已是 P0"), "{s}");
        assert!(s.contains("不再降"), "{s}");
    }

    #[test]
    fn demote_reply_success_shows_transition() {
        let s = format_demote_reply("写周报", Some(5), Ok(()));
        assert!(s.contains("🎯"), "{s}");
        assert!(s.contains("已降"), "{s}");
        assert!(s.contains("P5 → P4"), "{s}");
    }

    #[test]
    fn demote_reply_failure_shows_error() {
        let s = format_demote_reply("写周报", Some(3), Err("backend kaboom"));
        assert!(s.contains("降 priority 失败"), "{s}");
        assert!(s.contains("backend kaboom"), "{s}");
    }

    #[test]
    fn demote_reply_no_old_priority_fallback() {
        let s = format_demote_reply("t", None, Ok(()));
        assert!(s.contains("已降"), "{s}");
        assert!(!s.contains("P"), "no priority detail in fallback: {s}");
    }

    // -------- /promote parse + format --------

    #[test]
    fn promote_parses_title() {
        assert_eq!(
            parse_tg_command("/promote 整理 Downloads"),
            Some(TgCommand::Promote {
                title: "整理 Downloads".to_string()
            })
        );
    }

    #[test]
    fn promote_parses_empty_title() {
        assert_eq!(
            parse_tg_command("/promote"),
            Some(TgCommand::Promote {
                title: String::new()
            })
        );
    }

    #[test]
    fn promote_reply_empty_title_shows_usage() {
        let s = format_promote_reply("", Some(3), Ok(()));
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/promote <title>"), "{s}");
        assert!(s.contains("+1"), "{s}");
        // 互补 /pri / /demote
        assert!(s.contains("/pri"), "{s}");
        assert!(s.contains("/demote"), "{s}");
    }

    #[test]
    fn promote_reply_p9_shows_already_max() {
        let s = format_promote_reply("写周报", Some(9), Ok(()));
        assert!(s.contains("已是 P9"), "{s}");
        assert!(s.contains("不再升"), "{s}");
    }

    #[test]
    fn promote_reply_success_shows_transition() {
        let s = format_promote_reply("写周报", Some(3), Ok(()));
        assert!(s.contains("🎯"), "{s}");
        assert!(s.contains("已升"), "{s}");
        assert!(s.contains("P3 → P4"), "{s}");
    }

    #[test]
    fn promote_reply_failure_shows_error() {
        let s = format_promote_reply("写周报", Some(3), Err("backend kaboom"));
        assert!(s.contains("升 priority 失败"), "{s}");
        assert!(s.contains("backend kaboom"), "{s}");
    }

    #[test]
    fn promote_reply_no_old_priority_fallback() {
        // view miss 兜底
        let s = format_promote_reply("t", None, Ok(()));
        assert!(s.contains("已升"), "{s}");
        // 不显具体 P 转换
        assert!(!s.contains("P"), "no priority detail in fallback: {s}");
    }

    // -------- /feedback parse + format --------

    #[test]
    fn feedback_parses_text_arg() {
        assert_eq!(
            parse_tg_command("/feedback 你最近说话太啰嗦"),
            Some(TgCommand::Feedback {
                text: "你最近说话太啰嗦".to_string()
            })
        );
    }

    #[test]
    fn feedback_parses_empty_text() {
        assert_eq!(
            parse_tg_command("/feedback"),
            Some(TgCommand::Feedback {
                text: String::new()
            })
        );
    }

    #[test]
    fn feedback_reply_empty_shows_usage_hint() {
        let s = format_feedback_reply("");
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/feedback <text>"), "{s}");
        assert!(s.contains("feedback_history"), "{s}");
        // 对比 /note / /reflect — 让 owner 知道三入口差异
        assert!(s.contains("/note"), "{s}");
        assert!(s.contains("/reflect"), "{s}");
    }

    #[test]
    fn feedback_reply_success_shows_preview() {
        let s = format_feedback_reply("这次主动选 task 选得很到位");
        assert!(s.contains("💬 已记到 feedback_history"), "{s}");
        assert!(s.contains("这次主动选 task 选得很到位"), "{s}");
        assert!(s.contains("pet 在下次主动开口前会读到"), "{s}");
    }

    #[test]
    fn feedback_reply_long_text_truncates_preview() {
        let long = "啰".repeat(100);
        let s = format_feedback_reply(&long);
        assert!(s.contains("…"), "long text should be truncated: {s}");
    }

    // -------- /transient parse + format --------

    #[test]
    fn transient_parses_text_with_minutes() {
        assert_eq!(
            parse_tg_command("/transient 在开会别打扰 30"),
            Some(TgCommand::Transient {
                text: "在开会别打扰".to_string(),
                minutes: 30,
            })
        );
    }

    #[test]
    fn transient_parses_text_without_minutes_defaults_60() {
        assert_eq!(
            parse_tg_command("/transient 心情不好别活泼"),
            Some(TgCommand::Transient {
                text: "心情不好别活泼".to_string(),
                minutes: 60,
            })
        );
    }

    #[test]
    fn transient_parses_single_token_as_text() {
        // 单 token 不解析为 minutes — 当 text 默认 60。owner 想"我累了"等单
        // 词指示也应被接受为 text，不应被吞为"数字"。
        assert_eq!(
            parse_tg_command("/transient 累"),
            Some(TgCommand::Transient {
                text: "累".to_string(),
                minutes: 60,
            })
        );
        // 单 token 是数字也按 text 处理 — 与 /pri 同模板（避免漏 title 时
        // 误把 N 当 priority 写入）。
        assert_eq!(
            parse_tg_command("/transient 30"),
            Some(TgCommand::Transient {
                text: "30".to_string(),
                minutes: 60,
            })
        );
    }

    #[test]
    fn transient_parses_minutes_out_of_range_falls_back() {
        // > 10080 (7 天) 越界 → 整段当 text, default 60
        assert_eq!(
            parse_tg_command("/transient 长会议 99999"),
            Some(TgCommand::Transient {
                text: "长会议 99999".to_string(),
                minutes: 60,
            })
        );
        // 0 / 负数也越界（1..=10080）→ 整段当 text
        assert_eq!(
            parse_tg_command("/transient 测试 0"),
            Some(TgCommand::Transient {
                text: "测试 0".to_string(),
                minutes: 60,
            })
        );
    }

    #[test]
    fn transient_parses_empty_text() {
        assert_eq!(
            parse_tg_command("/transient"),
            Some(TgCommand::Transient {
                text: String::new(),
                minutes: 60,
            })
        );
    }

    #[test]
    fn transient_parses_max_minutes() {
        // 10080 (7 天) 上限合法
        assert_eq!(
            parse_tg_command("/transient 长出差 10080"),
            Some(TgCommand::Transient {
                text: "长出差".to_string(),
                minutes: 10080,
            })
        );
    }

    #[test]
    fn transient_reply_empty_shows_usage_hint() {
        let s = format_transient_reply("", 60, None);
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/transient <text>"), "{s}");
        assert!(s.contains("不存盘"), "强调 in-memory 而非永久存盘: {s}");
        // 让 owner 一眼看到与其它写入命令的区别
        assert!(s.contains("/note"), "{s}");
        assert!(s.contains("/mute"), "{s}");
    }

    #[test]
    fn transient_reply_with_until_shows_clear_time() {
        let until = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 21, 30, 0)
            .unwrap();
        let s = format_transient_reply("在开会别打扰", 30, Some(until));
        assert!(s.contains("已设 transient_note"), "{s}");
        assert!(s.contains("在开会别打扰"), "{s}");
        assert!(s.contains("30 分钟"), "{s}");
        assert!(s.contains("21:30"), "show clear time: {s}");
    }

    #[test]
    fn transient_reply_hour_label() {
        // 90 分钟 → "1 小时 30 分钟"
        let until = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 22, 0, 0)
            .unwrap();
        let s = format_transient_reply("写文档", 90, Some(until));
        assert!(s.contains("1 小时 30 分钟"), "{s}");
        // 120 分钟 → "2 小时"（无余数）
        let s = format_transient_reply("写文档", 120, Some(until));
        assert!(s.contains("2 小时"), "{s}");
        assert!(!s.contains("2 小时 0 分钟"), "no zero remainder: {s}");
    }

    #[test]
    fn transient_reply_day_label() {
        // 60 * 24 = 1440 → "1 天"
        let until = chrono::Local
            .with_ymd_and_hms(2026, 5, 18, 18, 0, 0)
            .unwrap();
        let s = format_transient_reply("出差三天", 4320, Some(until));
        assert!(s.contains("3 天"), "{s}");
    }

    #[test]
    fn transient_reply_long_text_truncates_preview() {
        let long = "在".repeat(100);
        let until = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 21, 30, 0)
            .unwrap();
        let s = format_transient_reply(&long, 60, Some(until));
        assert!(s.contains("…"), "long text should be truncated: {s}");
    }

    #[test]
    fn transient_reply_without_until_fallback() {
        // until=None defensive fallback — 不应崩，依旧给可读 reply
        let s = format_transient_reply("测试", 30, None);
        assert!(s.contains("已设 transient_note"), "{s}");
        assert!(s.contains("测试"), "{s}");
        // 不能含 HH:MM 占位
        assert!(!s.contains("到 — 自动清除"), "no placeholder: {s}");
    }

    // -------- /feedback_history parse + format --------

    #[test]
    fn feedback_history_parses_default_n_5() {
        assert_eq!(
            parse_tg_command("/feedback_history"),
            Some(TgCommand::FeedbackHistory { n: 5 })
        );
    }

    #[test]
    fn feedback_history_parses_explicit_n() {
        assert_eq!(
            parse_tg_command("/feedback_history 10"),
            Some(TgCommand::FeedbackHistory { n: 10 })
        );
    }

    #[test]
    fn feedback_history_clamps_high() {
        assert_eq!(
            parse_tg_command("/feedback_history 999"),
            Some(TgCommand::FeedbackHistory { n: 20 })
        );
    }

    #[test]
    fn feedback_history_clamps_zero_to_one() {
        // 0 / 负数 clamp 到下限 1
        assert_eq!(
            parse_tg_command("/feedback_history 0"),
            Some(TgCommand::FeedbackHistory { n: 1 })
        );
    }

    #[test]
    fn feedback_history_non_numeric_falls_back_to_default() {
        // 非数字 trailing token 走默认 5
        assert_eq!(
            parse_tg_command("/feedback_history blah"),
            Some(TgCommand::FeedbackHistory { n: 5 })
        );
    }

    #[test]
    fn feedback_history_reply_empty_shows_friendly_bootstrap() {
        let s = format_feedback_history_reply(&[], 5);
        assert!(s.contains("暂无 feedback 记录"), "{s}");
        assert!(s.contains("/feedback"), "show write entry hint: {s}");
    }

    #[test]
    fn feedback_history_reply_renders_entries_with_emoji() {
        use crate::feedback_history::{FeedbackEntry, FeedbackKind};
        let entries = vec![
            FeedbackEntry {
                timestamp: "2026-05-17T18:30:00+08:00".to_string(),
                kind: FeedbackKind::Comment,
                excerpt: "说话太啰嗦".to_string(),
            },
            FeedbackEntry {
                timestamp: "2026-05-17T18:35:12+08:00".to_string(),
                kind: FeedbackKind::Liked,
                excerpt: "感谢提醒".to_string(),
            },
        ];
        let s = format_feedback_history_reply(&entries, 5);
        assert!(s.contains("最近 2 条 feedback"), "{s}");
        assert!(s.contains("18:30"), "{s}");
        assert!(s.contains("18:35"), "{s}");
        assert!(s.contains("💬"), "comment emoji: {s}");
        assert!(s.contains("👍"), "liked emoji: {s}");
        assert!(s.contains("说话太啰嗦"), "{s}");
        assert!(s.contains("感谢提醒"), "{s}");
    }

    #[test]
    fn feedback_history_reply_caps_to_n_with_overflow_hint() {
        use crate::feedback_history::{FeedbackEntry, FeedbackKind};
        let mut entries = Vec::new();
        for i in 0..10 {
            entries.push(FeedbackEntry {
                timestamp: format!("2026-05-17T18:{:02}:00+08:00", i),
                kind: FeedbackKind::Replied,
                excerpt: format!("entry {}", i),
            });
        }
        let s = format_feedback_history_reply(&entries, 3);
        assert!(s.contains("最近 3 条 feedback"), "{s}");
        // overflow hint 该出现，且建议看更多
        assert!(s.contains("还有 7 条"), "overflow hint: {s}");
        assert!(s.contains("/feedback_history"), "hint references command: {s}");
        // 只显前 3 条
        assert!(s.contains("entry 0"), "{s}");
        assert!(s.contains("entry 2"), "{s}");
        assert!(!s.contains("entry 3"), "should be capped: {s}");
    }

    // -------- /silent_all parse + format --------

    #[test]
    fn silent_all_parses_default_60() {
        assert_eq!(
            parse_tg_command("/silent_all"),
            Some(TgCommand::SilentAll { minutes: 60 })
        );
    }

    #[test]
    fn silent_all_parses_explicit_minutes() {
        assert_eq!(
            parse_tg_command("/silent_all 30"),
            Some(TgCommand::SilentAll { minutes: 30 })
        );
        assert_eq!(
            parse_tg_command("/silent_all 120"),
            Some(TgCommand::SilentAll { minutes: 120 })
        );
    }

    #[test]
    fn silent_all_parses_zero_as_release_intent() {
        // 0 是合法 — 走 release_active 路径（与 /mute 0 同协议）
        assert_eq!(
            parse_tg_command("/silent_all 0"),
            Some(TgCommand::SilentAll { minutes: 0 })
        );
    }

    #[test]
    fn silent_all_clamps_high_to_7d() {
        assert_eq!(
            parse_tg_command("/silent_all 99999"),
            Some(TgCommand::SilentAll { minutes: 10080 })
        );
    }

    #[test]
    fn silent_all_clamps_negative_to_zero() {
        // 负数被 clamp 到 0（release 语义）— 不引入新错误态
        assert_eq!(
            parse_tg_command("/silent_all -10"),
            Some(TgCommand::SilentAll { minutes: 0 })
        );
    }

    #[test]
    fn silent_all_non_numeric_falls_back_to_default() {
        assert_eq!(
            parse_tg_command("/silent_all blah"),
            Some(TgCommand::SilentAll { minutes: 60 })
        );
    }

    #[test]
    fn silent_all_reply_release_no_active() {
        // minutes=0 + released=0 → 友好兜底
        let s = format_silent_all_reply(0, 0, 0, None);
        assert!(s.contains("当前无 silent 窗口"), "{s}");
        assert!(s.contains("/silent_all"), "show usage hint: {s}");
    }

    #[test]
    fn silent_all_reply_release_with_active() {
        // minutes=0 + released>0 → 已解除
        let s = format_silent_all_reply(0, 5, 0, None);
        assert!(s.contains("已解除 5 条"), "{s}");
    }

    #[test]
    fn silent_all_reply_arm_no_candidates() {
        // minutes>0 + armed=0 → 友好兜底
        let s = format_silent_all_reply(0, 0, 60, None);
        assert!(s.contains("暂无可 silent"), "{s}");
    }

    #[test]
    fn silent_all_reply_arm_success() {
        let until = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 19, 30, 0)
            .unwrap();
        let s = format_silent_all_reply(7, 0, 60, Some(until));
        assert!(s.contains("已 silent 7 条"), "{s}");
        assert!(s.contains("1 小时"), "{s}");
        assert!(s.contains("19:30"), "show expires_at HH:MM: {s}");
        assert!(s.contains("/silent_all 0"), "show release shortcut: {s}");
    }

    #[test]
    fn silent_all_reply_arm_with_prior_release() {
        // minutes>0 + armed>0 + released>0 → 显含 "（先解除上轮 N 条）"
        let until = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 20, 0, 0)
            .unwrap();
        let s = format_silent_all_reply(5, 3, 120, Some(until));
        assert!(s.contains("已 silent 5 条"), "{s}");
        assert!(s.contains("先解除上轮 3 条"), "{s}");
        assert!(s.contains("2 小时"), "{s}");
    }

    #[test]
    fn silent_all_reply_day_label() {
        // 60 * 24 = 1440 → "1 天"
        let until = chrono::Local
            .with_ymd_and_hms(2026, 5, 18, 18, 0, 0)
            .unwrap();
        let s = format_silent_all_reply(3, 0, 1440, Some(until));
        assert!(s.contains("1 天"), "{s}");
    }

    // -------- /alarms parse + format --------

    #[test]
    fn alarms_parses_default_n_5() {
        assert_eq!(
            parse_tg_command("/alarms"),
            Some(TgCommand::Alarms { n: 5 })
        );
    }

    #[test]
    fn alarms_parses_explicit_n() {
        assert_eq!(
            parse_tg_command("/alarms 10"),
            Some(TgCommand::Alarms { n: 10 })
        );
    }

    #[test]
    fn alarms_clamps_high_and_zero() {
        assert_eq!(
            parse_tg_command("/alarms 999"),
            Some(TgCommand::Alarms { n: 20 })
        );
        assert_eq!(
            parse_tg_command("/alarms 0"),
            Some(TgCommand::Alarms { n: 1 })
        );
    }

    #[test]
    fn alarms_non_numeric_falls_back() {
        assert_eq!(
            parse_tg_command("/alarms blah"),
            Some(TgCommand::Alarms { n: 5 })
        );
    }

    #[test]
    fn alarms_reply_empty_shows_bootstrap_hint() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(18, 30, 0)
            .unwrap();
        let s = format_alarms_reply(&[], now, 5);
        assert!(s.contains("暂无 pending alarms"), "{s}");
        assert!(s.contains("PanelMemory"), "show create hint: {s}");
        assert!(s.contains("[remind:"), "show protocol hint: {s}");
    }

    #[test]
    fn alarms_reply_future_shows_remaining_minutes() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(18, 0, 0)
            .unwrap();
        let target = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(18, 45, 0)
            .unwrap();
        let rows = vec![(
            crate::proactive::ReminderTarget::Absolute(target),
            "准备会议材料".to_string(),
            "⏰ 准备会议材料 @ 18:45".to_string(),
        )];
        let s = format_alarms_reply(&rows, now, 5);
        assert!(s.contains("最近 1 条 pending alarms"), "{s}");
        assert!(s.contains("18:45"), "{s}");
        assert!(s.contains("剩 45 分"), "{s}");
        assert!(s.contains("准备会议材料"), "{s}");
    }

    #[test]
    fn alarms_reply_past_shows_overdue_label() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(18, 30, 0)
            .unwrap();
        let target = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(18, 15, 0)
            .unwrap();
        let rows = vec![(
            crate::proactive::ReminderTarget::Absolute(target),
            "喝水".to_string(),
            "⏰ 喝水 @ 18:15".to_string(),
        )];
        let s = format_alarms_reply(&rows, now, 5);
        assert!(s.contains("已逾期 15 分"), "{s}");
    }

    #[test]
    fn alarms_reply_hour_and_day_buckets() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        // 4 小时后 + 3 天后
        let t_hour = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(14, 0, 0)
            .unwrap();
        let t_day = chrono::NaiveDate::from_ymd_opt(2026, 5, 20)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let rows = vec![
            (
                crate::proactive::ReminderTarget::Absolute(t_hour),
                "topic1".to_string(),
                "title1".to_string(),
            ),
            (
                crate::proactive::ReminderTarget::Absolute(t_day),
                "topic2".to_string(),
                "title2".to_string(),
            ),
        ];
        let s = format_alarms_reply(&rows, now, 5);
        assert!(s.contains("剩 4 小时"), "{s}");
        assert!(s.contains("剩 3 天"), "{s}");
    }

    #[test]
    fn alarms_reply_caps_to_n_with_overflow_hint() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let mut rows = Vec::new();
        for i in 0..7 {
            let t = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
                .unwrap()
                .and_hms_opt(10, (10 + i * 5) as u32, 0)
                .unwrap();
            rows.push((
                crate::proactive::ReminderTarget::Absolute(t),
                format!("t{}", i),
                format!("title{}", i),
            ));
        }
        let s = format_alarms_reply(&rows, now, 3);
        assert!(s.contains("最近 3 条 pending alarms"), "{s}");
        assert!(s.contains("还有 4 条更晚"), "overflow hint: {s}");
        assert!(s.contains("t0"), "{s}");
        assert!(s.contains("t2"), "{s}");
        assert!(!s.contains("t3"), "should be capped: {s}");
    }

    #[test]
    fn alarms_reply_today_hour_target() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(13, 0, 0)
            .unwrap();
        // TodayHour 14:30 — 90 分钟后
        let rows = vec![(
            crate::proactive::ReminderTarget::TodayHour(14, 30),
            "下午茶".to_string(),
            "alarm1".to_string(),
        )];
        let s = format_alarms_reply(&rows, now, 5);
        assert!(s.contains("14:30"), "{s}");
        assert!(s.contains("剩 1 小时"), "90 min → 1 小时 bucket: {s}");
    }

    // -------- /recent_chats parse + format --------

    #[test]
    fn recent_chats_parses_default_5() {
        assert_eq!(
            parse_tg_command("/recent_chats"),
            Some(TgCommand::RecentChats { n: 5 })
        );
    }

    #[test]
    fn recent_chats_parses_explicit_n() {
        assert_eq!(
            parse_tg_command("/recent_chats 10"),
            Some(TgCommand::RecentChats { n: 10 })
        );
    }

    #[test]
    fn recent_chats_clamps_high_and_zero() {
        assert_eq!(
            parse_tg_command("/recent_chats 999"),
            Some(TgCommand::RecentChats { n: 20 })
        );
        assert_eq!(
            parse_tg_command("/recent_chats 0"),
            Some(TgCommand::RecentChats { n: 1 })
        );
    }

    #[test]
    fn recent_chats_non_numeric_falls_back() {
        assert_eq!(
            parse_tg_command("/recent_chats foo"),
            Some(TgCommand::RecentChats { n: 5 })
        );
    }

    #[test]
    fn recent_chats_reply_empty_shows_bootstrap() {
        let s = format_recent_chats_reply(&[], "", "", 5, 0);
        assert!(s.contains("暂无聊天记录"), "{s}");
        assert!(s.contains("ChatMini"), "show creation path: {s}");
    }

    #[test]
    fn recent_chats_reply_renders_role_glyphs() {
        let items = vec![
            ("user".to_string(), "怎么整理 Downloads".to_string()),
            ("assistant".to_string(), "建议按修改时间归档".to_string()),
        ];
        let s = format_recent_chats_reply(
            &items,
            "整理桌面对话",
            "2026-05-17T18:30:00.000",
            5,
            2,
        );
        assert!(s.contains("最近 2 条 chat"), "{s}");
        assert!(s.contains("整理桌面对话"), "show session title: {s}");
        assert!(s.contains("05-17 18:30"), "show session updated_at MM-DD HH:MM: {s}");
        assert!(s.contains("🧑"), "user glyph: {s}");
        assert!(s.contains("🐾"), "assistant glyph: {s}");
        assert!(s.contains("怎么整理 Downloads"), "{s}");
        assert!(s.contains("建议按修改时间归档"), "{s}");
    }

    #[test]
    fn recent_chats_reply_truncates_long_title() {
        let items = vec![("user".to_string(), "hello".to_string())];
        let long_title = "这是一个非常非常非常非常非常非常非常非常长的会话标题超过24字";
        let s = format_recent_chats_reply(
            &items,
            long_title,
            "2026-05-17T18:30:00.000",
            5,
            1,
        );
        assert!(s.contains("…"), "long title should be truncated: {s}");
    }

    #[test]
    fn recent_chats_reply_overflow_hint_when_total_exceeds() {
        let items = vec![
            ("user".to_string(), "q1".to_string()),
            ("assistant".to_string(), "a1".to_string()),
            ("user".to_string(), "q2".to_string()),
        ];
        // total 10 / shown 3 → overflow 7
        let s = format_recent_chats_reply(
            &items,
            "session",
            "2026-05-17T18:30:00.000",
            3,
            10,
        );
        assert!(s.contains("最近 3 条 chat"), "{s}");
        assert!(s.contains("还有 7 条更早"), "overflow hint: {s}");
    }

    #[test]
    fn recent_chats_reply_no_overflow_when_total_matches() {
        let items = vec![("user".to_string(), "q1".to_string())];
        let s = format_recent_chats_reply(
            &items,
            "session",
            "2026-05-17T18:30:00.000",
            5,
            1,
        );
        assert!(!s.contains("更早消息"), "no overflow hint: {s}");
    }

    #[test]
    fn recent_chats_reply_empty_title_fallback() {
        let items = vec![("user".to_string(), "hello".to_string())];
        let s = format_recent_chats_reply(
            &items,
            "",
            "2026-05-17T18:30:00.000",
            5,
            1,
        );
        assert!(s.contains("（无标题）"), "empty title fallback: {s}");
    }

    #[test]
    fn feedback_history_reply_handles_short_timestamp_fallback() {
        // 防御：legacy / malformed timestamp 不应 panic
        use crate::feedback_history::{FeedbackEntry, FeedbackKind};
        let entries = vec![FeedbackEntry {
            timestamp: "2026".to_string(), // < 16 chars
            kind: FeedbackKind::Ignored,
            excerpt: "test".to_string(),
        }];
        let s = format_feedback_history_reply(&entries, 5);
        assert!(s.contains("2026"), "{s}");
        assert!(s.contains("🙉"), "ignored emoji: {s}");
        assert!(s.contains("test"), "{s}");
    }

    // -------- /pri parse + format --------

    #[test]
    fn pri_parses_title_with_priority() {
        assert_eq!(
            parse_tg_command("/pri 写周报 5"),
            Some(TgCommand::Pri {
                title: "写周报".to_string(),
                priority: Some(5),
            })
        );
    }

    #[test]
    fn pri_parses_title_with_spaces_and_priority() {
        // title 含空格，最后一个 token 是 N
        assert_eq!(
            parse_tg_command("/pri 整理 Downloads 桌面 7"),
            Some(TgCommand::Pri {
                title: "整理 Downloads 桌面".to_string(),
                priority: Some(7),
            })
        );
    }

    #[test]
    fn pri_parses_priority_zero_and_nine_boundary() {
        assert_eq!(
            parse_tg_command("/pri t 0"),
            Some(TgCommand::Pri {
                title: "t".to_string(),
                priority: Some(0),
            })
        );
        assert_eq!(
            parse_tg_command("/pri t 9"),
            Some(TgCommand::Pri {
                title: "t".to_string(),
                priority: Some(9),
            })
        );
    }

    #[test]
    fn pri_rejects_priority_out_of_range() {
        // 10 / 100 越界 → priority=None，整段当 title
        assert_eq!(
            parse_tg_command("/pri t 10"),
            Some(TgCommand::Pri {
                title: "t 10".to_string(),
                priority: None,
            })
        );
    }

    #[test]
    fn pri_no_trailing_number_treats_all_as_title() {
        // 末 token 不是数字 → priority None，全做 title
        assert_eq!(
            parse_tg_command("/pri 整理 Downloads"),
            Some(TgCommand::Pri {
                title: "整理 Downloads".to_string(),
                priority: None,
            })
        );
    }

    #[test]
    fn pri_empty_yields_both_empty() {
        assert_eq!(
            parse_tg_command("/pri"),
            Some(TgCommand::Pri {
                title: String::new(),
                priority: None,
            })
        );
    }

    #[test]
    fn pri_single_token_returns_priority_none() {
        // 仅 "5" — 没空白，无法区分是 title='5' 还是 priority=5
        // parser 走"统一返 None handler 走 usage hint" 路径
        assert_eq!(
            parse_tg_command("/pri 5"),
            Some(TgCommand::Pri {
                title: "5".to_string(),
                priority: None,
            })
        );
    }

    #[test]
    fn pri_reply_empty_title_shows_usage() {
        let s = format_pri_reply("", Some(5), Ok(()));
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/pri <title> <N>"), "{s}");
        assert!(s.contains("0..=9"), "should describe range: {s}");
    }

    #[test]
    fn pri_reply_no_priority_shows_usage_even_with_title() {
        let s = format_pri_reply("写周报", None, Ok(()));
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("0-9 整数"), "should explain N: {s}");
    }

    #[test]
    fn pri_reply_success_shows_title_and_priority() {
        let s = format_pri_reply("写周报", Some(5), Ok(()));
        assert!(s.contains("🎯"), "{s}");
        assert!(s.contains("已设"), "{s}");
        assert!(s.contains("写周报"), "{s}");
        assert!(s.contains("P5"), "{s}");
    }

    #[test]
    fn pri_reply_failure_shows_error() {
        let s = format_pri_reply("写周报", Some(5), Err("task not found"));
        assert!(s.contains("改 priority 失败"), "{s}");
        assert!(s.contains("task not found"), "{s}");
    }

    // -------- /swap_priority parse + format --------

    #[test]
    fn swap_priority_parses_double_colon_separator() {
        assert_eq!(
            parse_tg_command("/swap_priority A :: B"),
            Some(TgCommand::SwapPriority {
                title_a: "A".to_string(),
                title_b: "B".to_string(),
            })
        );
        // title with spaces / chinese punctuation
        assert_eq!(
            parse_tg_command("/swap_priority 整理 Downloads :: 写周报"),
            Some(TgCommand::SwapPriority {
                title_a: "整理 Downloads".to_string(),
                title_b: "写周报".to_string(),
            })
        );
    }

    #[test]
    fn swap_priority_missing_separator_keeps_first_empty_second() {
        // 无 `::` 时整段作 a，b 为空 → handler 走 usage hint
        assert_eq!(
            parse_tg_command("/swap_priority just one title"),
            Some(TgCommand::SwapPriority {
                title_a: "just one title".to_string(),
                title_b: "".to_string(),
            })
        );
    }

    #[test]
    fn swap_priority_reply_missing_title_shows_usage() {
        let s = format_swap_priority_reply("", "B", None, None, Ok(()), Ok(()));
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("`::`"), "show separator: {s}");
        let s2 = format_swap_priority_reply("A", "", None, None, Ok(()), Ok(()));
        assert!(s2.contains("用法"), "{s2}");
    }

    #[test]
    fn swap_priority_reply_same_title_short_circuits() {
        let s = format_swap_priority_reply(
            "A", "A", Some(3), Some(3), Ok(()), Ok(()),
        );
        assert!(s.contains("无需互换"), "{s}");
        assert!(!s.contains("已互换"), "{s}");
    }

    #[test]
    fn swap_priority_reply_missing_resolve_shows_which() {
        let s = format_swap_priority_reply(
            "A", "B", None, Some(5), Ok(()), Ok(()),
        );
        assert!(s.contains("「A」"), "highlights missing A: {s}");
        assert!(s.contains("没找到"), "{s}");
        let s2 = format_swap_priority_reply(
            "A", "B", Some(3), None, Ok(()), Ok(()),
        );
        assert!(s2.contains("「B」"), "highlights missing B: {s2}");
        let s3 = format_swap_priority_reply(
            "A", "B", None, None, Ok(()), Ok(()),
        );
        assert!(s3.contains("「A」"), "{s3}");
        assert!(s3.contains("「B」"), "{s3}");
    }

    #[test]
    fn swap_priority_reply_success_format() {
        let s = format_swap_priority_reply(
            "整理 Downloads",
            "写周报",
            Some(3),
            Some(7),
            Ok(()),
            Ok(()),
        );
        assert!(s.contains("🔄"), "{s}");
        assert!(s.contains("已互换"), "{s}");
        // a: 3 → 7
        assert!(s.contains("整理 Downloads"), "{s}");
        assert!(s.contains("P3 → P7"), "{s}");
        // b: 7 → 3
        assert!(s.contains("写周报"), "{s}");
        assert!(s.contains("P7 → P3"), "{s}");
    }

    #[test]
    fn swap_priority_reply_partial_failure_shows_per_step() {
        let s = format_swap_priority_reply(
            "A",
            "B",
            Some(3),
            Some(7),
            Ok(()),
            Err("write failed"),
        );
        assert!(s.contains("部分失败"), "{s}");
        assert!(s.contains("✓ 「A」"), "A succeeded: {s}");
        assert!(s.contains("⚠️ 「B」"), "B failed: {s}");
        assert!(s.contains("write failed"), "{s}");
    }

    // -------- /streak parse + format --------

    #[test]
    fn streak_parses_no_args() {
        assert_eq!(parse_tg_command("/streak"), Some(TgCommand::Streak));
        assert_eq!(parse_tg_command("/streak now"), Some(TgCommand::Streak));
        assert_eq!(parse_tg_command("/STREAK"), Some(TgCommand::Streak));
    }

    fn date(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn streak_empty_set_returns_zero() {
        let today = date(2026, 5, 17);
        let set = std::collections::HashSet::new();
        assert_eq!(compute_done_streak(&set, today), 0);
    }

    #[test]
    fn streak_today_only_returns_1() {
        let today = date(2026, 5, 17);
        let mut set = std::collections::HashSet::new();
        set.insert(today);
        assert_eq!(compute_done_streak(&set, today), 1);
    }

    #[test]
    fn streak_yesterday_only_starts_from_yesterday() {
        let today = date(2026, 5, 17);
        let mut set = std::collections::HashSet::new();
        set.insert(today - chrono::Duration::days(1));
        // 今日无但昨日有 → streak 应从昨日往前数 = 1（仅昨日）
        assert_eq!(compute_done_streak(&set, today), 1);
    }

    #[test]
    fn streak_3_consecutive_days_ending_today() {
        let today = date(2026, 5, 17);
        let mut set = std::collections::HashSet::new();
        set.insert(today);
        set.insert(today - chrono::Duration::days(1));
        set.insert(today - chrono::Duration::days(2));
        assert_eq!(compute_done_streak(&set, today), 3);
    }

    #[test]
    fn streak_gap_breaks_count() {
        let today = date(2026, 5, 17);
        let mut set = std::collections::HashSet::new();
        set.insert(today); // day 0
        set.insert(today - chrono::Duration::days(2)); // skip day 1
        // 今日有 → 从今日往前数；day 1 缺 → break，streak = 1
        assert_eq!(compute_done_streak(&set, today), 1);
    }

    #[test]
    fn streak_no_today_no_yesterday_returns_zero_even_if_older() {
        let today = date(2026, 5, 17);
        let mut set = std::collections::HashSet::new();
        // 3 days ago done — 但 streak end 要求 today 或 yesterday，否则 0
        set.insert(today - chrono::Duration::days(3));
        assert_eq!(compute_done_streak(&set, today), 0);
    }

    #[test]
    fn done_dates_filters_to_done_and_parses_iso() {
        let mut a = view("a", 3, None, TaskStatus::Done, Some("ok"));
        a.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let mut b = view("b", 3, None, TaskStatus::Pending, None);
        b.updated_at = "2026-05-16T10:00:00+08:00".to_string();
        let mut c = view("c", 3, None, TaskStatus::Done, Some("r"));
        c.updated_at = "2026-05-15T10:00:00+08:00".to_string();
        let set = done_dates_from_views(&[a, b, c]);
        assert!(set.contains(&date(2026, 5, 17)));
        assert!(!set.contains(&date(2026, 5, 16)), "pending excluded");
        assert!(set.contains(&date(2026, 5, 15)));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn count_done_in_window_inclusive_boundaries() {
        let today = date(2026, 5, 17);
        let mut day0 = view("today", 3, None, TaskStatus::Done, Some("r"));
        day0.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let mut day6 = view("6 days ago", 3, None, TaskStatus::Done, Some("r"));
        day6.updated_at = "2026-05-11T10:00:00+08:00".to_string();
        let mut day7 = view("7 days ago", 3, None, TaskStatus::Done, Some("r"));
        day7.updated_at = "2026-05-10T10:00:00+08:00".to_string();
        let views = vec![day0, day6, day7];
        // 近 7 天 = [today-6, today] = 2026-05-11..2026-05-17，含 day0 + day6（2 条），不含 day7
        assert_eq!(count_done_in_window(&views, today, 7), 2);
        // 近 30 天 = [today-29, today] — 三条都进
        assert_eq!(count_done_in_window(&views, today, 30), 3);
    }

    #[test]
    fn count_done_excludes_non_done_status() {
        let today = date(2026, 5, 17);
        let mut pending = view("p", 3, None, TaskStatus::Pending, None);
        pending.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let mut error = view("e", 3, None, TaskStatus::Error, Some("err"));
        error.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let mut cancelled = view("c", 3, None, TaskStatus::Cancelled, Some("c"));
        cancelled.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        assert_eq!(
            count_done_in_window(&[pending, error, cancelled], today, 7),
            0,
        );
    }

    #[test]
    fn streak_reply_renders_fire_when_streak_gt_0() {
        let today = date(2026, 5, 17);
        let mut done = view("today done", 3, None, TaskStatus::Done, Some("r"));
        done.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let s = format_streak_reply(&[done], today);
        assert!(s.contains("🔥"), "{s}");
        assert!(s.contains("连续 1 天"), "{s}");
        assert!(s.contains("近 7 天 done：1 条"), "{s}");
        assert!(s.contains("近 30 天 done：1 条"), "{s}");
    }

    #[test]
    fn streak_reply_zero_streak_shows_seedling() {
        let today = date(2026, 5, 17);
        let s = format_streak_reply(&[], today);
        assert!(s.contains("🌱"), "{s}");
        assert!(s.contains("streak 中断"), "{s}");
        assert!(s.contains("近 7 天 done：0 条"), "{s}");
    }

    // -------- /yesterday parse + format --------

    #[test]
    fn yesterday_parses_no_args() {
        assert_eq!(parse_tg_command("/yesterday"), Some(TgCommand::Yesterday));
        assert_eq!(
            parse_tg_command("/yesterday please"),
            Some(TgCommand::Yesterday)
        );
        assert_eq!(parse_tg_command("/YESTERDAY"), Some(TgCommand::Yesterday));
    }

    #[test]
    fn yesterday_reply_empty_shows_quiet_hint() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let s = format_yesterday_reply(&[], today);
        assert!(s.contains("昨日（2026-05-16）无完成记录"), "{s}");
        assert!(s.contains("/recent"), "should hint alternatives: {s}");
    }

    #[test]
    fn yesterday_reply_filters_to_done_on_y_date_only() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut y_done = view("y_task", 3, None, TaskStatus::Done, Some("yesterday result"));
        y_done.updated_at = "2026-05-16T15:30:00+08:00".to_string();
        let mut today_done = view("today_task", 3, None, TaskStatus::Done, Some("today result"));
        today_done.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let mut y_pending = view("y_pending", 3, None, TaskStatus::Pending, None);
        y_pending.updated_at = "2026-05-16T11:00:00+08:00".to_string();
        let mut y_cancelled = view(
            "y_cancelled",
            3,
            None,
            TaskStatus::Cancelled,
            Some("dropped"),
        );
        y_cancelled.updated_at = "2026-05-16T12:00:00+08:00".to_string();
        let s = format_yesterday_reply(
            &[y_done, today_done, y_pending, y_cancelled],
            today,
        );
        assert!(s.contains("y_task"), "y_done should appear: {s}");
        assert!(s.contains("完成 1 条"), "count should reflect filter: {s}");
        assert!(!s.contains("today_task"), "today_done excluded: {s}");
        assert!(!s.contains("y_pending"), "pending excluded: {s}");
        assert!(!s.contains("y_cancelled"), "cancelled excluded: {s}");
    }

    #[test]
    fn yesterday_reply_sorts_by_updated_at_desc() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut early = view("早完成", 3, None, TaskStatus::Done, Some("e"));
        early.updated_at = "2026-05-16T08:00:00+08:00".to_string();
        let mut late = view("晚完成", 3, None, TaskStatus::Done, Some("l"));
        late.updated_at = "2026-05-16T22:30:00+08:00".to_string();
        let mut mid = view("中间", 3, None, TaskStatus::Done, Some("m"));
        mid.updated_at = "2026-05-16T14:00:00+08:00".to_string();
        let s = format_yesterday_reply(&[early, mid, late], today);
        let idx_late = s.find("晚完成").expect("晚完成 in output");
        let idx_mid = s.find("中间").expect("中间 in output");
        let idx_early = s.find("早完成").expect("早完成 in output");
        assert!(idx_late < idx_mid, "晚完成 before 中间: {s}");
        assert!(idx_mid < idx_early, "中间 before 早完成: {s}");
    }

    #[test]
    fn yesterday_reply_includes_result_summary() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut done = view("写周报", 3, None, TaskStatus::Done, Some("发了 Q2 周报到 Slack"));
        done.updated_at = "2026-05-16T18:00:00+08:00".to_string();
        let s = format_yesterday_reply(&[done], today);
        assert!(s.contains("写周报"), "{s}");
        assert!(s.contains("— 发了 Q2 周报到 Slack"), "result preview: {s}");
    }

    #[test]
    fn yesterday_reply_truncates_long_result() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let long_result = "x".repeat(80);
        let mut done = view("t", 3, None, TaskStatus::Done, Some(long_result.as_str()));
        done.updated_at = "2026-05-16T10:00:00+08:00".to_string();
        let s = format_yesterday_reply(&[done], today);
        assert!(s.contains("…"), "long result should be truncated: {s}");
    }

    #[test]
    fn yesterday_reply_omits_empty_result() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut done = view("t", 3, None, TaskStatus::Done, Some("   "));
        done.updated_at = "2026-05-16T10:00:00+08:00".to_string();
        let s = format_yesterday_reply(&[done], today);
        // 空白 result trim 后空 → 不渲染 " — ...." segment
        assert!(!s.contains(" — "), "no empty result segment: {s}");
        assert!(s.contains("t"), "title still rendered: {s}");
    }

    // -------- /today_done parse + format --------

    #[test]
    fn today_done_parses_no_args() {
        assert_eq!(
            parse_tg_command("/today_done"),
            Some(TgCommand::TodayDone)
        );
        assert_eq!(
            parse_tg_command("/today_done  "),
            Some(TgCommand::TodayDone)
        );
        assert_eq!(
            parse_tg_command("/today_done now"),
            Some(TgCommand::TodayDone)
        );
        // case-insensitive parse 与 /yesterday 一致
        assert_eq!(
            parse_tg_command("/TODAY_DONE"),
            Some(TgCommand::TodayDone)
        );
    }

    #[test]
    fn today_done_reply_empty_shows_friendly_fallback() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let s = format_today_done_reply(&[], today);
        assert!(s.contains("今日（2026-05-17）暂无完成记录"), "{s}");
        // 兜底里要建议两条 alt 入口
        assert!(s.contains("/today"), "alt hint /today: {s}");
        assert!(s.contains("/yesterday"), "alt hint /yesterday: {s}");
    }

    #[test]
    fn today_done_reply_filters_to_done_on_today_only() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut today_done = view("t_task", 3, None, TaskStatus::Done, Some("today result"));
        today_done.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let mut y_done = view("y_task", 3, None, TaskStatus::Done, Some("y"));
        y_done.updated_at = "2026-05-16T15:00:00+08:00".to_string();
        let mut t_pending = view("t_pending", 3, None, TaskStatus::Pending, None);
        t_pending.updated_at = "2026-05-17T09:00:00+08:00".to_string();
        let mut t_cancelled = view(
            "t_cancelled",
            3,
            None,
            TaskStatus::Cancelled,
            Some("dropped"),
        );
        t_cancelled.updated_at = "2026-05-17T11:00:00+08:00".to_string();
        let s = format_today_done_reply(
            &[today_done, y_done, t_pending, t_cancelled],
            today,
        );
        assert!(s.contains("t_task"), "today_done included: {s}");
        assert!(s.contains("完成 1 条"), "count reflects filter: {s}");
        assert!(!s.contains("y_task"), "yesterday excluded: {s}");
        assert!(!s.contains("t_pending"), "pending excluded: {s}");
        assert!(!s.contains("t_cancelled"), "cancelled excluded: {s}");
    }

    #[test]
    fn today_done_reply_sorts_by_updated_at_desc() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut early = view("早", 3, None, TaskStatus::Done, Some("e"));
        early.updated_at = "2026-05-17T08:00:00+08:00".to_string();
        let mut late = view("晚", 3, None, TaskStatus::Done, Some("l"));
        late.updated_at = "2026-05-17T22:30:00+08:00".to_string();
        let mut mid = view("中", 3, None, TaskStatus::Done, Some("m"));
        mid.updated_at = "2026-05-17T14:00:00+08:00".to_string();
        let s = format_today_done_reply(&[early, mid, late], today);
        let idx_late = s.find("晚").expect("晚 in output");
        let idx_mid = s.find("中").expect("中 in output");
        let idx_early = s.find("早").expect("早 in output");
        assert!(idx_late < idx_mid, "晚 before 中: {s}");
        assert!(idx_mid < idx_early, "中 before 早: {s}");
    }

    #[test]
    fn today_done_reply_includes_result_summary() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut done = view("写文档", 3, None, TaskStatus::Done, Some("提交到 PR #42"));
        done.updated_at = "2026-05-17T16:00:00+08:00".to_string();
        let s = format_today_done_reply(&[done], today);
        assert!(s.contains("写文档"), "{s}");
        assert!(s.contains("— 提交到 PR #42"), "result preview: {s}");
    }

    #[test]
    fn today_done_reply_truncates_long_result_at_40_chars() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let long_result = "x".repeat(80);
        let mut done = view("t", 3, None, TaskStatus::Done, Some(long_result.as_str()));
        done.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let s = format_today_done_reply(&[done], today);
        assert!(s.contains("…"), "long result should be truncated: {s}");
    }

    #[test]
    fn today_done_reply_omits_empty_result() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut done = view("t", 3, None, TaskStatus::Done, Some("   "));
        done.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let s = format_today_done_reply(&[done], today);
        assert!(!s.contains(" — "), "no empty result segment: {s}");
        assert!(s.contains("t"), "title still rendered: {s}");
    }

    // -------- /quick parse + format --------

    #[test]
    fn quick_parses_text_arg() {
        assert_eq!(
            parse_tg_command("/quick 整理 ~/Downloads"),
            Some(TgCommand::Quick {
                text: "整理 ~/Downloads".to_string()
            })
        );
    }

    #[test]
    fn quick_parses_empty_text() {
        assert_eq!(
            parse_tg_command("/quick"),
            Some(TgCommand::Quick {
                text: String::new()
            })
        );
        assert_eq!(
            parse_tg_command("/quick    "),
            Some(TgCommand::Quick {
                text: String::new()
            })
        );
    }

    #[test]
    fn quick_does_not_parse_priority_prefix() {
        // /quick "!!  写周报" — !! 不被解析为 P5；保留原 text
        assert_eq!(
            parse_tg_command("/quick !! 写周报"),
            Some(TgCommand::Quick {
                text: "!! 写周报".to_string()
            })
        );
    }

    #[test]
    fn quick_reply_empty_shows_usage_hint() {
        let s = format_quick_reply("", Ok(()));
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/quick <text>"), "{s}");
        assert!(s.contains("P3"), "should explain priority: {s}");
        assert!(s.contains("/task"), "should hint upgrade path: {s}");
    }

    #[test]
    fn quick_reply_success_is_minimal() {
        let s = format_quick_reply("整理 ~/Downloads", Ok(()));
        assert_eq!(s, "✓ 整理 ~/Downloads", "should be just check + title");
        // 极短 reply 不该含 /tasks / /cancel 等长指引（与 format_task_
        // created_success 反向）
        assert!(!s.contains("/tasks"));
        assert!(!s.contains("/cancel"));
    }

    #[test]
    fn quick_reply_trims_whitespace_from_title() {
        let s = format_quick_reply("  写周报  ", Ok(()));
        assert_eq!(s, "✓ 写周报", "trim leading / trailing whitespace: {s}");
    }

    #[test]
    fn quick_reply_save_failure_shows_error() {
        let s = format_quick_reply("写周报", Err("Title already exists"));
        assert!(s.contains("⚡"), "{s}");
        assert!(s.contains("创建失败"), "{s}");
        assert!(s.contains("Title already exists"), "{s}");
    }

    // -------- /sleep parse + format --------

    #[test]
    fn sleep_parses_no_args() {
        assert_eq!(parse_tg_command("/sleep"), Some(TgCommand::Sleep));
        assert_eq!(parse_tg_command("/sleep tight"), Some(TgCommand::Sleep));
        assert_eq!(parse_tg_command("/SLEEP"), Some(TgCommand::Sleep));
    }

    #[test]
    fn sleep_reply_includes_friendly_tone_and_until_time() {
        use chrono::{NaiveDate, TimeZone};
        // 模拟 caller 已经算好 8h 后 = 23:42
        let until = chrono::Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2026, 5, 17)
                    .unwrap()
                    .and_hms_opt(23, 42, 0)
                    .unwrap(),
            )
            .unwrap();
        let s = format_sleep_reply(Some(until));
        assert!(s.contains("🌙"), "{s}");
        assert!(s.contains("宠物去睡了"), "tone: {s}");
        assert!(s.contains("8 小时静音"), "duration label: {s}");
        assert!(s.contains("23:42"), "until time: {s}");
        assert!(s.contains("晚安"), "{s}");
        assert!(s.contains("/mute 0"), "should hint how to undo: {s}");
    }

    #[test]
    fn sleep_reply_until_none_uses_dash_placeholder() {
        let s = format_sleep_reply(None);
        assert!(s.contains("—"), "should use dash when until missing: {s}");
        assert!(s.contains("🌙"), "{s}");
    }

    #[test]
    fn sleep_mute_minutes_constant_is_8_hours() {
        assert_eq!(SLEEP_MUTE_MINUTES, 480, "8 * 60 = 480");
    }

    // -------- /random parse + format --------

    #[test]
    fn random_parses_no_args() {
        assert_eq!(parse_tg_command("/random"), Some(TgCommand::Random));
        assert_eq!(parse_tg_command("/random pick one"), Some(TgCommand::Random));
        assert_eq!(parse_tg_command("/RANDOM"), Some(TgCommand::Random));
    }

    #[test]
    fn random_reply_empty_actives_shows_friendly_hint() {
        // 全是 done / cancelled → 没 active 任务
        let mut done = view("做完的", 3, None, TaskStatus::Done, Some("结果"));
        done.created_at = "2026-05-15T10:00:00+08:00".to_string();
        let mut cancelled = view("取消的", 3, None, TaskStatus::Cancelled, Some("不做了"));
        cancelled.created_at = "2026-05-16T10:00:00+08:00".to_string();
        let s = format_random_reply(&[done, cancelled], 0);
        assert!(s.contains("暂无 active 任务"), "{s}");
        assert!(s.contains("/task <title>"), "should hint how to create: {s}");
    }

    #[test]
    fn random_reply_picks_pending_only() {
        let pending = view("待做", 3, None, TaskStatus::Pending, None);
        let done = view("做完", 3, None, TaskStatus::Done, Some("ok"));
        let cancelled = view("取消", 3, None, TaskStatus::Cancelled, None);
        // seed=0 → 第 0 个 candidate（filter 后是 pending 那条）
        let s = format_random_reply(&[done, pending.clone(), cancelled], 0);
        assert!(s.contains("待做"), "should pick pending: {s}");
        assert!(!s.contains("做完"), "{s}");
        assert!(!s.contains("取消"), "{s}");
    }

    #[test]
    fn random_reply_includes_error_in_actives() {
        let mut err = view("error 了", 3, None, TaskStatus::Error, Some("失败原因"));
        err.created_at = "2026-05-17T10:00:00+08:00".to_string();
        let s = format_random_reply(&[err], 0);
        assert!(s.contains("error 了"), "should include error: {s}");
        assert!(s.contains("⚠️"), "should show error emoji: {s}");
    }

    #[test]
    fn random_reply_seed_indexes_deterministically() {
        // 3 个 candidates；seed 0/1/2 应该索引到 candidates[0/1/2]
        let a = view("A", 3, None, TaskStatus::Pending, None);
        let b = view("B", 3, None, TaskStatus::Pending, None);
        let c = view("C", 3, None, TaskStatus::Pending, None);
        let views = vec![a, b, c];
        let s0 = format_random_reply(&views, 0);
        let s1 = format_random_reply(&views, 1);
        let s2 = format_random_reply(&views, 2);
        assert!(s0.contains("「A」"), "seed=0 → A: {s0}");
        assert!(s1.contains("「B」"), "seed=1 → B: {s1}");
        assert!(s2.contains("「C」"), "seed=2 → C: {s2}");
        // seed=3 wraps back to candidates[0]
        let s3 = format_random_reply(&views, 3);
        assert!(s3.contains("「A」"), "seed=3 wraps to A: {s3}");
    }

    #[test]
    fn random_reply_shows_active_count() {
        let p1 = view("p1", 3, None, TaskStatus::Pending, None);
        let p2 = view("p2", 3, None, TaskStatus::Pending, None);
        let done = view("done", 3, None, TaskStatus::Done, Some("ok"));
        let s = format_random_reply(&[p1, p2, done], 0);
        assert!(s.contains("共 2 条 active"), "{s}");
    }

    #[test]
    fn random_reply_truncates_long_raw_description() {
        let mut v = view("long", 3, None, TaskStatus::Pending, None);
        v.raw_description = "x".repeat(RANDOM_RAW_DESC_PREVIEW_CHARS + 50);
        let s = format_random_reply(&[v], 0);
        assert!(s.contains("…"), "should truncate: {s}");
    }

    #[test]
    fn random_reply_omits_raw_when_empty() {
        let mut v = view("t", 3, None, TaskStatus::Pending, None);
        v.raw_description = "".to_string();
        let s = format_random_reply(&[v], 0);
        // 头 + 尾鼓励语都在，中间 raw 段省略
        assert!(s.contains("抽中"), "{s}");
        assert!(s.contains("选择困难"), "{s}");
        // 验证没产生 "preview...\n\n"-then-tail 的空段
        let lines: Vec<&str> = s.split('\n').collect();
        // 空 line 数量应该 ≤ 1（仅 tail 前那一个）
        let blank_count = lines.iter().filter(|l| l.is_empty()).count();
        assert!(blank_count <= 1, "extra blank from empty raw: {s:?}");
    }

    #[test]
    fn random_reply_tail_has_encouragement() {
        let v = view("t", 3, None, TaskStatus::Pending, None);
        let s = format_random_reply(&[v], 0);
        assert!(s.contains("选择困难？就先做这条吧"), "tail: {s}");
    }

    // -------- /last parse + format --------

    #[test]
    fn last_parses_no_args() {
        assert_eq!(parse_tg_command("/last"), Some(TgCommand::Last));
        assert_eq!(parse_tg_command("/last anything"), Some(TgCommand::Last));
        assert_eq!(parse_tg_command("/LAST"), Some(TgCommand::Last));
    }

    fn ndt(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
    }

    #[test]
    fn last_reply_empty_views_shows_friendly_hint() {
        let s = format_last_reply(&[], ndt(2026, 5, 17, 14, 0));
        assert!(s.contains("还没派过单"), "{s}");
        assert!(s.contains("/task <title>"), "should hint how to create: {s}");
    }

    #[test]
    fn last_reply_picks_max_created_at_across_views() {
        let mut older = view("旧任务", 3, None, TaskStatus::Pending, None);
        older.created_at = "2026-05-15T10:00:00+08:00".to_string();
        older.raw_description = "[task pri=3] 旧任务 body".to_string();
        let mut newest = view("刚创的", 5, None, TaskStatus::Pending, None);
        newest.created_at = "2026-05-17T13:50:00+08:00".to_string();
        newest.raw_description = "[task pri=5 due=2026-05-20] 刚创的 body".to_string();
        let mut middle = view("中间", 3, None, TaskStatus::Done, Some("结果"));
        middle.created_at = "2026-05-16T09:00:00+08:00".to_string();
        let s = format_last_reply(&[older, newest, middle], ndt(2026, 5, 17, 14, 0));
        assert!(s.contains("刚创的"), "should pick newest: {s}");
        assert!(!s.contains("旧任务"), "older shouldn't appear: {s}");
        assert!(!s.contains("中间"), "middle shouldn't appear: {s}");
    }

    #[test]
    fn last_reply_shows_status_emoji_per_state() {
        let mut v = view("t", 3, None, TaskStatus::Pending, None);
        v.created_at = "2026-05-17T13:00:00+08:00".to_string();
        let s = format_last_reply(&[v.clone()], ndt(2026, 5, 17, 14, 0));
        assert!(s.contains("⏳"), "pending: {s}");
        v.status = TaskStatus::Done;
        assert!(format_last_reply(&[v.clone()], ndt(2026, 5, 17, 14, 0)).contains("✅"));
        v.status = TaskStatus::Error;
        assert!(format_last_reply(&[v.clone()], ndt(2026, 5, 17, 14, 0)).contains("⚠️"));
        v.status = TaskStatus::Cancelled;
        assert!(format_last_reply(&[v], ndt(2026, 5, 17, 14, 0)).contains("🚫"));
    }

    #[test]
    fn last_reply_truncates_long_raw_description() {
        let mut v = view("t", 3, None, TaskStatus::Pending, None);
        v.created_at = "2026-05-17T13:00:00+08:00".to_string();
        v.raw_description = "x".repeat(LAST_RAW_DESC_PREVIEW_CHARS + 100);
        let s = format_last_reply(&[v], ndt(2026, 5, 17, 14, 0));
        assert!(s.contains("…"), "should truncate: {s}");
    }

    #[test]
    fn last_reply_omits_raw_when_empty() {
        let mut v = view("t", 3, None, TaskStatus::Pending, None);
        v.created_at = "2026-05-17T13:00:00+08:00".to_string();
        v.raw_description = "".to_string();
        let s = format_last_reply(&[v], ndt(2026, 5, 17, 14, 0));
        // 头部仍渲染；只是没有 raw preview 段
        assert!(s.contains("最近创建"), "{s}");
        // 应不含双换行 + 空内容的"raw preview 空段"
        assert!(!s.contains("\n\n"), "no empty preview block: {s}");
    }

    // -------- format_created_relative buckets --------

    #[test]
    fn created_relative_just_now_within_60s() {
        let now = ndt(2026, 5, 17, 14, 0);
        // 30 秒前
        let c = "2026-05-17T13:59:30+08:00";
        // 这里 NaiveDateTime / FixedOffset 接合：format_created_relative
        // 走 rfc3339 parse → naive_local，与 ndt 参数同 timezone-stripped
        // 比较。30 秒差应该 → "刚创建"
        let s = format_created_relative(c, now);
        assert_eq!(s, "刚创建");
    }

    #[test]
    fn created_relative_minutes_bucket() {
        let now = ndt(2026, 5, 17, 14, 0);
        // 5 分钟前
        let c = "2026-05-17T13:55:00+08:00";
        let s = format_created_relative(c, now);
        assert_eq!(s, "5 分钟前");
    }

    #[test]
    fn created_relative_hours_bucket() {
        let now = ndt(2026, 5, 17, 14, 0);
        let c = "2026-05-17T11:00:00+08:00";
        let s = format_created_relative(c, now);
        assert_eq!(s, "3 小时前");
    }

    #[test]
    fn created_relative_days_bucket() {
        let now = ndt(2026, 5, 17, 14, 0);
        let c = "2026-05-14T14:00:00+08:00";
        let s = format_created_relative(c, now);
        assert_eq!(s, "3 天前");
    }

    #[test]
    fn created_relative_parse_failure_returns_hint() {
        let now = ndt(2026, 5, 17, 14, 0);
        let s = format_created_relative("not-a-date", now);
        assert!(s.contains("parse 失败"), "{s}");
    }

    // -------- /now parse + format --------

    #[test]
    fn now_parses_no_args() {
        assert_eq!(parse_tg_command("/now"), Some(TgCommand::Now));
        // 多余尾部忽略（与 /today / /mood / /version 同容忍策略）
        assert_eq!(parse_tg_command("/now please"), Some(TgCommand::Now));
        assert_eq!(parse_tg_command("/NOW"), Some(TgCommand::Now));
    }

    fn fixed_dt(y: i32, mo: u32, d: u32, h: u32, mi: u32, tz_hours: i32) -> chrono::DateTime<chrono::FixedOffset> {
        use chrono::{NaiveDate, TimeZone};
        let dt = NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap();
        let offset = chrono::FixedOffset::east_opt(tz_hours * 3600).unwrap();
        offset.from_local_datetime(&dt).unwrap()
    }

    #[test]
    fn now_reply_full_signal_renders_time_tz_days_mood() {
        let now = fixed_dt(2026, 5, 17, 14, 42, 8);
        let s = format_now_reply(now, Some(14), Some("今天特别专注"));
        assert!(s.contains("2026-05-17 14:42"), "{s}");
        assert!(s.contains("+08:00"), "{s}");
        assert!(s.contains("陪伴 14 天"), "{s}");
        assert!(s.contains("心情：今天特别专注"), "{s}");
    }

    #[test]
    fn now_reply_mood_emoji_prefix_matches_text() {
        // 复用 mood_emoji_for — "开心" → 😊
        let now = fixed_dt(2026, 5, 17, 14, 42, 8);
        let s = format_now_reply(now, Some(1), Some("今天很开心"));
        let first_line = s.lines().next().unwrap();
        assert!(first_line.starts_with("😊"), "expected 😊 prefix: {first_line}");
    }

    #[test]
    fn now_reply_paw_fallback_when_mood_missing() {
        let now = fixed_dt(2026, 5, 17, 14, 42, 8);
        let s = format_now_reply(now, Some(3), None);
        let first_line = s.lines().next().unwrap();
        assert!(first_line.starts_with("🐾"), "no-mood should fall back to 🐾: {first_line}");
        assert!(!s.contains("心情："), "no mood section should be rendered: {s}");
    }

    #[test]
    fn now_reply_paw_fallback_when_mood_empty() {
        let now = fixed_dt(2026, 5, 17, 14, 42, 8);
        let s = format_now_reply(now, Some(3), Some("   "));
        assert!(s.starts_with("🐾"), "empty mood text should fall back to 🐾: {s}");
    }

    #[test]
    fn now_reply_zero_days_says_today_init() {
        let now = fixed_dt(2026, 5, 17, 9, 0, 8);
        let s = format_now_reply(now, Some(0), None);
        assert!(s.contains("今天与你初识"), "{s}");
        assert!(!s.contains("陪伴 0 天"), "should switch wording at 0: {s}");
    }

    #[test]
    fn now_reply_no_companionship_no_mood_only_time_line() {
        let now = fixed_dt(2026, 5, 17, 14, 42, 8);
        let s = format_now_reply(now, None, None);
        // 第二行整段省略 — 仅时间行
        assert_eq!(s.lines().count(), 1, "should be single line: {s:?}");
        assert!(s.contains("2026-05-17 14:42"), "{s}");
        assert!(s.contains("+08:00"), "{s}");
    }

    #[test]
    fn now_reply_negative_tz_offset_renders_minus() {
        // -05:00（New York standard time）
        let now = fixed_dt(2026, 5, 17, 14, 42, -5);
        let s = format_now_reply(now, Some(7), None);
        assert!(s.contains("-05:00"), "should render negative tz: {s}");
    }

    // -------- /last_speech parse + format --------

    #[test]
    fn last_speech_parses_no_args() {
        assert_eq!(
            parse_tg_command("/last_speech"),
            Some(TgCommand::LastSpeech)
        );
        // 多余尾部忽略
        assert_eq!(
            parse_tg_command("/last_speech please"),
            Some(TgCommand::LastSpeech)
        );
    }

    fn fixed_local(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> chrono::DateTime<chrono::Local> {
        use chrono::TimeZone;
        chrono::Local
            .with_ymd_and_hms(y, mo, d, h, mi, 0)
            .unwrap()
    }

    #[test]
    fn last_speech_reply_none_says_no_history() {
        let now = fixed_local(2026, 5, 17, 14, 42);
        let s = format_last_speech_reply(None, now);
        assert!(s.contains("🗣"), "{s}");
        assert!(s.contains("还没主动开口过"), "{s}");
    }

    #[test]
    fn last_speech_reply_renders_text_and_relative_time_minutes() {
        // ts = now - 30 min（用 Local 本地时区构造 RFC3339 字符串）
        use chrono::TimeZone;
        let now = fixed_local(2026, 5, 17, 14, 42);
        let ts = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 14, 12, 0)
            .unwrap()
            .to_rfc3339();
        let s = format_last_speech_reply(
            Some((ts.as_str(), "今天工作怎么样？")),
            now,
        );
        assert!(s.contains("🗣"), "{s}");
        assert!(s.contains("今天工作怎么样？"), "{s}");
        assert!(s.contains("30 分前"), "expects '30 分前': {s}");
    }

    #[test]
    fn last_speech_reply_renders_relative_hours_when_over_60min() {
        use chrono::TimeZone;
        let now = fixed_local(2026, 5, 17, 18, 0);
        // 3 小时前
        let ts = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 15, 0, 0)
            .unwrap()
            .to_rfc3339();
        let s = format_last_speech_reply(Some((ts.as_str(), "hello")), now);
        assert!(s.contains("3 小时前"), "{s}");
    }

    #[test]
    fn last_speech_reply_renders_relative_days_when_over_24h() {
        use chrono::TimeZone;
        let now = fixed_local(2026, 5, 17, 18, 0);
        // 2 天前
        let ts = chrono::Local
            .with_ymd_and_hms(2026, 5, 15, 18, 0, 0)
            .unwrap()
            .to_rfc3339();
        let s = format_last_speech_reply(Some((ts.as_str(), "hi")), now);
        assert!(s.contains("2 天前"), "{s}");
    }

    #[test]
    fn last_speech_reply_truncates_long_text_to_200_with_ellipsis() {
        use chrono::TimeZone;
        let now = fixed_local(2026, 5, 17, 14, 42);
        let ts = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 14, 30, 0)
            .unwrap()
            .to_rfc3339();
        let long_text: String = "啊".repeat(250);
        let s = format_last_speech_reply(
            Some((ts.as_str(), long_text.as_str())),
            now,
        );
        assert!(s.contains("…"), "expected ellipsis: {s}");
        // chars count: 200 啊 + 一个 …
        let inner_chars = s.chars().filter(|&c| c == '啊').count();
        assert_eq!(inner_chars, 200, "expected 200 chars cap");
    }

    #[test]
    fn last_speech_reply_handles_invalid_ts_gracefully() {
        let now = fixed_local(2026, 5, 17, 14, 42);
        let s = format_last_speech_reply(
            Some(("not-a-valid-iso", "fallback text")),
            now,
        );
        assert!(s.contains("ts 解析失败"), "{s}");
        assert!(s.contains("fallback text"), "still shows text: {s}");
    }

    // -------- /aware parse + format --------

    #[test]
    fn aware_parses_no_args() {
        assert_eq!(parse_tg_command("/aware"), Some(TgCommand::Aware));
    }

    #[test]
    fn aware_parses_ignores_trailing_garbage() {
        // 与 /now 同模板：多余尾部一律忽略
        assert_eq!(parse_tg_command("/aware blah blah"), Some(TgCommand::Aware));
    }

    #[test]
    fn aware_reply_renders_all_signals() {
        let now = fixed_dt(2026, 5, 17, 18, 30, 8);
        let s = format_aware_reply(
            Some(("在开会，半小时别打扰我", 30)),
            5,
            Some("好奇"),
            now,
            Some(42),
        );
        assert!(s.contains("当前感知"), "header: {s}");
        assert!(s.contains("transient_note: 「在开会"), "transient text: {s}");
        assert!(s.contains("剩 30 分钟"), "remaining minutes: {s}");
        assert!(s.contains("active tasks: 5 条"), "{s}");
        assert!(s.contains("🤔"), "curious emoji: {s}");
        assert!(s.contains("好奇"), "mood text: {s}");
        assert!(s.contains("2026-05-17 18:30"), "{s}");
        assert!(s.contains("+08:00"), "tz: {s}");
        assert!(s.contains("陪伴 42 天"), "{s}");
    }

    #[test]
    fn aware_reply_empty_transient_shows_无() {
        let now = fixed_dt(2026, 5, 17, 18, 30, 8);
        let s = format_aware_reply(None, 0, None, now, Some(0));
        assert!(s.contains("transient_note: 无"), "{s}");
        assert!(s.contains("active tasks: 0 条"), "{s}");
        assert!(s.contains("今日初识"), "0 days: {s}");
    }

    #[test]
    fn aware_reply_empty_mood_shows_emoji_fallback() {
        let now = fixed_dt(2026, 5, 17, 18, 30, 8);
        let s = format_aware_reply(None, 3, Some("   "), now, Some(7));
        // mood 仅空白 → emoji 🐾 + "（暂无心情）" 兜底
        assert!(s.contains("🐾"), "{s}");
        assert!(s.contains("暂无心情"), "{s}");
    }

    #[test]
    fn aware_reply_long_transient_truncates() {
        let long = "在".repeat(100);
        let now = fixed_dt(2026, 5, 17, 18, 30, 8);
        let s = format_aware_reply(Some((&long, 30)), 1, None, now, Some(1));
        assert!(s.contains("…"), "long text should be truncated: {s}");
    }

    #[test]
    fn aware_reply_zero_minutes_clamps_to_1() {
        // 边界过期态：caller 传 mins=0 → formatter clamp 到 1 防"剩 0 分钟"
        let now = fixed_dt(2026, 5, 17, 18, 30, 8);
        let s = format_aware_reply(Some(("test", 0)), 1, None, now, Some(1));
        assert!(s.contains("剩 1 分钟"), "{s}");
    }

    #[test]
    fn aware_reply_no_companionship_no_mood_skips_tail() {
        // companionship_days = None → tail 只剩时间 + tz
        let now = fixed_dt(2026, 5, 17, 18, 30, 8);
        let s = format_aware_reply(None, 0, None, now, None);
        assert!(s.contains("2026-05-17 18:30"), "{s}");
        assert!(!s.contains("陪伴"), "no companionship tail: {s}");
        assert!(!s.contains("今日初识"), "no init tail: {s}");
    }

    // -------- /here parse + format --------

    #[test]
    fn here_parses_no_args() {
        assert_eq!(parse_tg_command("/here"), Some(TgCommand::Here));
    }

    #[test]
    fn here_parses_ignores_trailing() {
        assert_eq!(parse_tg_command("/here foo bar"), Some(TgCommand::Here));
    }

    #[test]
    fn here_reply_all_active_signals() {
        let s = format_here_reply(
            Some(("在开会别打扰", 15)),
            Some(30),
            "high_negative",
        );
        assert!(s.contains("当前 owner 信号"), "{s}");
        assert!(s.contains("transient_note: 「在开会别打扰"), "{s}");
        assert!(s.contains("剩 15 分钟"), "{s}");
        assert!(s.contains("mute: 剩 30 分钟"), "{s}");
        assert!(s.contains("high_negative"), "{s}");
        assert!(s.contains("×2.0"), "show factor: {s}");
    }

    #[test]
    fn here_reply_no_signals_shows_baselines() {
        let s = format_here_reply(None, None, "insufficient_samples");
        assert!(s.contains("transient_note: 未设"), "{s}");
        assert!(s.contains("mute: 未静音"), "{s}");
        assert!(s.contains("insufficient_samples"), "{s}");
        assert!(s.contains("样本不足"), "{s}");
    }

    #[test]
    fn here_reply_low_negative_band_says_pet_more_active() {
        let s = format_here_reply(None, None, "low_negative");
        assert!(s.contains("low_negative"), "{s}");
        assert!(s.contains("×0.7"), "{s}");
        assert!(s.contains("更主动"), "{s}");
    }

    #[test]
    fn here_reply_mid_band_says_neutral() {
        let s = format_here_reply(None, None, "mid");
        assert!(s.contains("mid"), "{s}");
        assert!(s.contains("×1.0"), "{s}");
        assert!(s.contains("中性"), "{s}");
    }

    #[test]
    fn here_reply_mute_zero_clamps_to_one() {
        // 边界过期态：caller 传 mute_minutes=0 → formatter clamp 到 1
        let s = format_here_reply(None, Some(0), "mid");
        assert!(s.contains("mute: 剩 1 分钟"), "{s}");
    }

    #[test]
    fn here_reply_long_transient_truncates() {
        let long = "在".repeat(100);
        let s = format_here_reply(Some((&long, 15)), None, "mid");
        assert!(s.contains("…"), "long text truncate: {s}");
    }

    #[test]
    fn here_reply_unknown_band_falls_back_to_insufficient() {
        // 未识别的 band 字符串 fallback 到 insufficient_samples 文案
        let s = format_here_reply(None, None, "unknown_label_xyz");
        assert!(s.contains("insufficient_samples"), "{s}");
        assert!(s.contains("样本不足"), "{s}");
    }

    // -------- /tag parse + format --------

    #[test]
    fn tag_parses_bare_name() {
        assert_eq!(
            parse_tg_command("/tag 工作"),
            Some(TgCommand::Tag {
                name: "工作".to_string()
            })
        );
    }

    #[test]
    fn tag_parses_hash_prefix_stripped() {
        // `#` 前缀允许 — 与桌面 PanelTasks #tag chip 同输入风格
        assert_eq!(
            parse_tg_command("/tag #urgent"),
            Some(TgCommand::Tag {
                name: "urgent".to_string()
            })
        );
    }

    #[test]
    fn tag_parses_trailing_garbage_ignored() {
        // 第二个 token 起一律忽略（与 parse_task_tags 无空格 tag 边界一致）
        assert_eq!(
            parse_tg_command("/tag 工作 extra trash"),
            Some(TgCommand::Tag {
                name: "工作".to_string()
            })
        );
    }

    #[test]
    fn tag_parses_empty_name() {
        assert_eq!(
            parse_tg_command("/tag"),
            Some(TgCommand::Tag {
                name: String::new()
            })
        );
        // 仅 `#` 前缀 + 空白 → 空 name（handler 走 usage hint）
        assert_eq!(
            parse_tg_command("/tag #"),
            Some(TgCommand::Tag {
                name: String::new()
            })
        );
    }

    #[test]
    fn tag_reply_empty_name_shows_usage_hint() {
        let s = format_tag_reply(&[], "");
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/tag <name>"), "{s}");
        assert!(s.contains("/tags"), "show tag-list cross-ref: {s}");
    }

    #[test]
    fn tag_reply_no_hits_shows_bootstrap() {
        let views = vec![view_with_tags("a", &["健身"])];
        let s = format_tag_reply(&views, "读书");
        assert!(s.contains("没有任务带 #读书"), "{s}");
        assert!(s.contains("/tags"), "推荐 /tags: {s}");
    }

    #[test]
    fn tag_reply_lists_matching_tasks_with_status_emoji() {
        let views = vec![
            view_with_tags("健身 morning", &["健身", "晨练"]),
            view_with_tags("读书", &["读书"]),
            view_with_tags("健身 evening", &["健身"]),
        ];
        let s = format_tag_reply(&views, "健身");
        assert!(s.contains("#健身 命中 2 条"), "{s}");
        assert!(s.contains("🟢"), "pending emoji: {s}");
        assert!(s.contains("健身 morning"), "{s}");
        assert!(s.contains("健身 evening"), "{s}");
        assert!(!s.contains("读书"), "should not include 读书: {s}");
    }

    #[test]
    fn tag_reply_case_insensitive_match() {
        let views = vec![view_with_tags("a", &["URGENT"])];
        let s = format_tag_reply(&views, "urgent");
        assert!(s.contains("#urgent 命中 1 条"), "{s}");
        // tag 数组里 raw 是 URGENT，但 caller 输 urgent —— exact lower-case
        // 比较应该命中。
    }

    #[test]
    fn tag_reply_pending_before_done() {
        let mut v_done = view_with_tags("done-a", &["x"]);
        v_done.status = crate::task_queue::TaskStatus::Done;
        let v_pending = view_with_tags("pending-a", &["x"]);
        let views = vec![v_done.clone(), v_pending];
        let s = format_tag_reply(&views, "x");
        // pending 应在 done 之前（status_rank sort）
        let p_idx = s.find("pending-a").unwrap();
        let d_idx = s.find("done-a").unwrap();
        assert!(p_idx < d_idx, "pending before done: {s}");
    }

    #[test]
    fn tag_reply_includes_due_label() {
        let mut v = view_with_tags("with-due", &["urgent"]);
        v.due = Some("2026-05-20T14:30".to_string());
        let s = format_tag_reply(&[v], "urgent");
        assert!(s.contains("05-20 14:30"), "compact due display: {s}");
    }

    #[test]
    fn tag_reply_overflow_hint_above_20() {
        let mut views = Vec::new();
        for i in 0..25 {
            views.push(view_with_tags(
                &format!("task-{}", i),
                &["bulk"],
            ));
        }
        let s = format_tag_reply(&views, "bulk");
        assert!(s.contains("#bulk 命中 25 条"), "{s}");
        assert!(s.contains("还有 5 条带本 tag"), "overflow: {s}");
    }

    // -------- /tags_for parse + format --------

    #[test]
    fn tags_for_parser_takes_all_args_as_title() {
        assert_eq!(
            parse_tg_command("/tags_for 整理 Downloads"),
            Some(TgCommand::TagsFor {
                title: "整理 Downloads".to_string()
            })
        );
    }

    #[test]
    fn tags_for_parser_empty_title_parses() {
        assert_eq!(
            parse_tg_command("/tags_for"),
            Some(TgCommand::TagsFor {
                title: String::new()
            })
        );
    }

    #[test]
    fn tags_for_reply_empty_target_shows_usage() {
        let s = format_tags_for_reply(&[], "");
        assert!(s.contains("用法"), "{s}");
    }

    #[test]
    fn tags_for_reply_target_not_found() {
        let v = view_with_tags("别人", &["foo"]);
        let s = format_tags_for_reply(&[v], "不存在");
        assert!(s.contains("没找到"), "{s}");
    }

    #[test]
    fn tags_for_reply_no_tags_teaches_syntax() {
        let v = view("无 tag", 3, None, TaskStatus::Pending, None);
        let s = format_tags_for_reply(&[v], "无 tag");
        assert!(s.contains("无 #tag 标记"), "{s}");
        assert!(s.contains("`#name`"), "should teach syntax: {s}");
    }

    #[test]
    fn tags_for_reply_lists_tags_with_count() {
        let v = view_with_tags("整理 Downloads", &["工作", "urgent", "整理"]);
        let s = format_tags_for_reply(&[v], "整理 Downloads");
        assert!(s.contains("3 个 tag"), "count: {s}");
        assert!(s.contains("#工作"), "{s}");
        assert!(s.contains("#urgent"), "{s}");
        assert!(s.contains("#整理"), "{s}");
    }

    // -------- /touch parse + format --------

    #[test]
    fn touch_parser_takes_all_args_as_title() {
        assert_eq!(
            parse_tg_command("/touch 整理 Downloads"),
            Some(TgCommand::Touch {
                title: "整理 Downloads".to_string()
            })
        );
    }

    #[test]
    fn touch_parser_empty_title_parses() {
        assert_eq!(
            parse_tg_command("/touch"),
            Some(TgCommand::Touch {
                title: String::new()
            })
        );
    }

    #[test]
    fn touch_reply_empty_title_shows_usage() {
        let s = format_touch_reply("", Ok(()));
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/touch"), "{s}");
        assert!(s.contains("updated_at"), "explains mechanism: {s}");
    }

    #[test]
    fn touch_reply_success_acknowledges_refresh() {
        let s = format_touch_reply("整理 Downloads", Ok(()));
        assert!(s.contains("✨"), "{s}");
        assert!(s.contains("已 touch"), "{s}");
        assert!(s.contains("整理 Downloads"), "{s}");
        assert!(s.contains("updated_at"), "{s}");
    }

    #[test]
    fn touch_reply_failure_shows_error() {
        let s = format_touch_reply("写周报", Err("cannot touch a finished task"));
        assert!(s.contains("touch 失败"), "{s}");
        assert!(s.contains("cannot touch"), "{s}");
    }

    // -------- /edit_due parse + compute + format --------

    #[test]
    fn edit_due_parse_preset_tonight_aliases() {
        assert_eq!(parse_edit_due_preset("tonight"), Some(EditDuePreset::Tonight));
        assert_eq!(parse_edit_due_preset("今晚"), Some(EditDuePreset::Tonight));
    }

    #[test]
    fn edit_due_parse_preset_tomorrow_aliases() {
        for s in &["tomorrow", "tmr", "明天", "morning", "早上"] {
            assert_eq!(
                parse_edit_due_preset(s),
                Some(EditDuePreset::TomorrowMorning),
                "alias {} should map to TomorrowMorning",
                s,
            );
        }
    }

    #[test]
    fn edit_due_parse_preset_clear_aliases() {
        for s in &["clear", "none", "0", "清除", "取消"] {
            assert_eq!(
                parse_edit_due_preset(s),
                Some(EditDuePreset::Clear),
                "alias {} should map to Clear",
                s,
            );
        }
    }

    #[test]
    fn edit_due_parse_preset_weekday() {
        // Monday = 0
        assert_eq!(parse_edit_due_preset("monday"), Some(EditDuePreset::Weekday(0)));
        assert_eq!(parse_edit_due_preset("周一"), Some(EditDuePreset::Weekday(0)));
        // Sunday = 6
        assert_eq!(parse_edit_due_preset("sunday"), Some(EditDuePreset::Weekday(6)));
        assert_eq!(parse_edit_due_preset("周日"), Some(EditDuePreset::Weekday(6)));
    }

    #[test]
    fn edit_due_parse_preset_next_weekday() {
        assert_eq!(
            parse_edit_due_preset("next_monday"),
            Some(EditDuePreset::NextWeekday(0)),
        );
        assert_eq!(
            parse_edit_due_preset("下周五"),
            Some(EditDuePreset::NextWeekday(4)),
        );
    }

    #[test]
    fn edit_due_parse_preset_relative_duration() {
        assert_eq!(parse_edit_due_preset("+30m"), Some(EditDuePreset::PlusMinutes(30)));
        assert_eq!(parse_edit_due_preset("+2h"), Some(EditDuePreset::PlusHours(2)));
        assert_eq!(parse_edit_due_preset("+1d"), Some(EditDuePreset::PlusDays(1)));
        // 0 / invalid 拒
        assert_eq!(parse_edit_due_preset("+0m"), None);
        assert_eq!(parse_edit_due_preset("+xyz"), None);
        assert_eq!(parse_edit_due_preset("+5s"), None); // 秒不支持
    }

    #[test]
    fn edit_due_parse_preset_unknown_returns_none() {
        assert_eq!(parse_edit_due_preset("blahblah"), None);
        assert_eq!(parse_edit_due_preset(""), None);
    }

    #[test]
    fn edit_due_compute_tonight_before_18() {
        let now = ndt(2026, 5, 17, 14, 0);
        let result = compute_edit_due_preset(&EditDuePreset::Tonight, now);
        assert_eq!(result, Some(ndt(2026, 5, 17, 18, 0)));
    }

    #[test]
    fn edit_due_compute_tonight_after_18_rolls_to_next_day() {
        let now = ndt(2026, 5, 17, 22, 0);
        let result = compute_edit_due_preset(&EditDuePreset::Tonight, now);
        assert_eq!(result, Some(ndt(2026, 5, 18, 18, 0)));
    }

    #[test]
    fn edit_due_compute_tomorrow_morning() {
        let now = ndt(2026, 5, 17, 14, 0);
        let result = compute_edit_due_preset(&EditDuePreset::TomorrowMorning, now);
        assert_eq!(result, Some(ndt(2026, 5, 18, 9, 0)));
    }

    #[test]
    fn edit_due_compute_weekday_future_in_week() {
        // 2026-05-17 is Sunday (weekday 6). Monday(0) is +1 day.
        let now = ndt(2026, 5, 17, 14, 0);
        let result = compute_edit_due_preset(&EditDuePreset::Weekday(0), now);
        assert_eq!(result, Some(ndt(2026, 5, 18, 9, 0)));
    }

    #[test]
    fn edit_due_compute_weekday_today_before_9_today() {
        // 2026-05-17 is Sunday (weekday 6). Sunday(6) at 08:00 → today 09:00.
        let now = ndt(2026, 5, 17, 8, 0);
        let result = compute_edit_due_preset(&EditDuePreset::Weekday(6), now);
        assert_eq!(result, Some(ndt(2026, 5, 17, 9, 0)));
    }

    #[test]
    fn edit_due_compute_weekday_today_after_9_next_week() {
        // 2026-05-17 is Sunday. Sunday(6) at 10:00 → next Sunday 2026-05-24.
        let now = ndt(2026, 5, 17, 10, 0);
        let result = compute_edit_due_preset(&EditDuePreset::Weekday(6), now);
        assert_eq!(result, Some(ndt(2026, 5, 24, 9, 0)));
    }

    #[test]
    fn edit_due_compute_next_weekday_always_at_least_7d_out() {
        // 2026-05-17 (Sun) + next_monday(0) → 2026-05-25（下下周一）
        let now = ndt(2026, 5, 17, 8, 0);
        let result = compute_edit_due_preset(&EditDuePreset::NextWeekday(0), now);
        assert_eq!(result, Some(ndt(2026, 5, 25, 9, 0)));
    }

    #[test]
    fn edit_due_compute_plus_minutes() {
        let now = ndt(2026, 5, 17, 14, 30);
        let result = compute_edit_due_preset(&EditDuePreset::PlusMinutes(45), now);
        assert_eq!(result, Some(ndt(2026, 5, 17, 15, 15)));
    }

    #[test]
    fn edit_due_compute_plus_hours() {
        let now = ndt(2026, 5, 17, 14, 0);
        let result = compute_edit_due_preset(&EditDuePreset::PlusHours(3), now);
        assert_eq!(result, Some(ndt(2026, 5, 17, 17, 0)));
    }

    #[test]
    fn edit_due_compute_plus_days_lands_morning_9am() {
        let now = ndt(2026, 5, 17, 14, 30);
        let result = compute_edit_due_preset(&EditDuePreset::PlusDays(2), now);
        assert_eq!(result, Some(ndt(2026, 5, 19, 9, 0)));
    }

    #[test]
    fn edit_due_compute_clear_returns_none() {
        let now = ndt(2026, 5, 17, 14, 0);
        assert_eq!(compute_edit_due_preset(&EditDuePreset::Clear, now), None);
    }

    #[test]
    fn edit_due_parse_command_title_and_preset() {
        assert_eq!(
            parse_tg_command("/edit_due 整理 Downloads tonight"),
            Some(TgCommand::EditDue {
                title: "整理 Downloads".to_string(),
                preset: Some(EditDuePreset::Tonight),
            }),
        );
    }

    #[test]
    fn edit_due_parse_command_unknown_preset_treated_as_title() {
        // preset 无法识别 → 整段当 title，preset=None（handler usage hint）
        assert_eq!(
            parse_tg_command("/edit_due 整理 Downloads invalidpreset"),
            Some(TgCommand::EditDue {
                title: "整理 Downloads invalidpreset".to_string(),
                preset: None,
            }),
        );
    }

    #[test]
    fn edit_due_parse_command_single_token_preset_only() {
        // 仅 preset 缺 title → handler 走 usage hint
        assert_eq!(
            parse_tg_command("/edit_due tonight"),
            Some(TgCommand::EditDue {
                title: String::new(),
                preset: Some(EditDuePreset::Tonight),
            }),
        );
    }

    #[test]
    fn edit_due_parse_command_empty() {
        assert_eq!(
            parse_tg_command("/edit_due"),
            Some(TgCommand::EditDue {
                title: String::new(),
                preset: None,
            }),
        );
    }

    #[test]
    fn edit_due_reply_empty_shows_usage() {
        let s = format_edit_due_reply("", None, None, Ok(()));
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/edit_due <title> <preset>"), "{s}");
        assert!(s.contains("tonight"), "show preset names: {s}");
        assert!(s.contains("+30m"), "show relative example: {s}");
        assert!(s.contains("clear"), "show clear option: {s}");
    }

    #[test]
    fn edit_due_reply_set_success() {
        let s = format_edit_due_reply(
            "整理 Downloads",
            Some(&EditDuePreset::Tonight),
            Some(ndt(2026, 5, 17, 18, 0)),
            Ok(()),
        );
        assert!(s.contains("已设「整理 Downloads」"), "{s}");
        assert!(s.contains("05-17 18:00"), "{s}");
    }

    #[test]
    fn edit_due_reply_clear_success() {
        let s = format_edit_due_reply(
            "整理 Downloads",
            Some(&EditDuePreset::Clear),
            None,
            Ok(()),
        );
        assert!(s.contains("已清「整理 Downloads」"), "{s}");
    }

    #[test]
    fn edit_due_reply_save_err() {
        let s = format_edit_due_reply(
            "missing-task",
            Some(&EditDuePreset::Tonight),
            Some(ndt(2026, 5, 17, 18, 0)),
            Err("task not found: missing-task"),
        );
        assert!(s.contains("设 due 失败"), "{s}");
        assert!(s.contains("not found"), "show err msg: {s}");
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

    // -------- /timeline parse + extract_marker_tokens + entries + format --------

    #[test]
    fn timeline_parser_takes_all_args_as_title() {
        assert_eq!(
            parse_tg_command("/timeline 整理 Downloads"),
            Some(TgCommand::Timeline {
                title: "整理 Downloads".to_string()
            })
        );
    }

    #[test]
    fn timeline_parser_empty_title_parses() {
        // 与 /show 同模板：空 title 让 handler 走 missing-arg hint，parser
        // 仍命中变体（避免走 Unknown 兜底）
        assert_eq!(
            parse_tg_command("/timeline"),
            Some(TgCommand::Timeline {
                title: String::new()
            })
        );
    }

    #[test]
    fn timeline_extract_markers_finds_known_keys() {
        let snippet = "update 写周报 :: [task pri=3] [pinned] body [done] [result: 已发送]";
        let tokens = extract_marker_tokens(snippet);
        assert_eq!(
            tokens,
            vec![
                "[pinned]".to_string(),
                "[done]".to_string(),
                "[result: 已发送]".to_string()
            ],
            "should pick pinned/done/result, skip [task pri=...]: {:?}",
            tokens
        );
    }

    #[test]
    fn timeline_extract_markers_skips_metadata_brackets() {
        // [task pri=...] / [origin:...] / [every:...] / [once:...] / [tags:...]
        // 都是静态元数据 — 不应入 timeline state-change list
        let snippet = "[task pri=5] [origin:tg:12345] [every: 09:00] [tags: 工作 #urgent] body";
        let tokens = extract_marker_tokens(snippet);
        assert!(tokens.is_empty(), "should ignore metadata brackets: {:?}", tokens);
    }

    #[test]
    fn timeline_extract_markers_handles_chinese_colon_in_error() {
        let snippet = "[error：网络超时] body";
        let tokens = extract_marker_tokens(snippet);
        assert_eq!(tokens, vec!["[error：网络超时]".to_string()]);
    }

    #[test]
    fn timeline_extract_markers_picks_blocked_by_camelcase() {
        let snippet = "[blockedBy: 整理 Downloads] body";
        let tokens = extract_marker_tokens(snippet);
        assert_eq!(tokens, vec!["[blockedBy: 整理 Downloads]".to_string()]);
    }

    #[test]
    fn timeline_extract_markers_avoids_false_match_on_similar_prefix() {
        // "[doneish]" / "[errorlike]" 不应命中（key 需后接 ` ` / `:` / `]`）
        let snippet = "[doneish] [errorlike: x] body";
        let tokens = extract_marker_tokens(snippet);
        assert!(tokens.is_empty(), "should reject prefix-only matches: {:?}", tokens);
    }

    #[test]
    fn timeline_extract_markers_handles_unclosed_bracket_gracefully() {
        // 无闭合 ] 时 break 不 panic
        let snippet = "[done] [snooze: 永远";
        let tokens = extract_marker_tokens(snippet);
        assert_eq!(tokens, vec!["[done]".to_string()]);
    }

    fn ev(ts: &str, action: &str, snippet: &str) -> (String, String, String) {
        (ts.to_string(), action.to_string(), snippet.to_string())
    }

    #[test]
    fn timeline_compute_entries_reverses_to_chronological() {
        // filter_history_for_task 输出 newest-first；compute 应输出 oldest-first
        let events = vec![
            ev("2026-05-17T18:00:00+08:00", "update", "[done]"),
            ev("2026-05-15T09:30:00+08:00", "create", ""),
        ];
        let entries = compute_timeline_entries(&events);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].timestamp.starts_with("2026-05-15"));
        assert!(entries[1].timestamp.starts_with("2026-05-17"));
    }

    #[test]
    fn timeline_compute_entries_dedupes_consecutive_unchanged_updates() {
        // create + 三条都标 [pinned] 的 update → 第二第三条同 marker set 应去重
        let events = vec![
            ev("2026-05-17T12:00:00+08:00", "update", "[pinned]"),
            ev("2026-05-17T11:00:00+08:00", "update", "[pinned]"),
            ev("2026-05-17T10:00:00+08:00", "update", "[pinned]"),
            ev("2026-05-17T09:00:00+08:00", "create", ""),
        ];
        let entries = compute_timeline_entries(&events);
        // 期望：create + 第一个 [pinned] update（剩两条 update 因 marker 集合
        // 与前事件相同被去重）
        assert_eq!(entries.len(), 2, "{:?}", entries);
        assert_eq!(entries[0].action, "create");
        assert_eq!(entries[1].markers, vec!["[pinned]".to_string()]);
    }

    #[test]
    fn timeline_compute_entries_keeps_create_and_delete_force() {
        // create + 一条 update（[pinned]）+ delete → 三条都保。
        // 验证 force_keep 让 create/delete 不受 marker-dedup 影响 — 哪怕
        // delete 与上一 update 一样 marker 集合（pinned）也要保（owner 关
        // 心"任务被删除了"这件事本身，非 marker 变化）。
        let events = vec![
            ev("2026-05-17T15:00:00+08:00", "delete", "[pinned]"),
            ev("2026-05-17T14:00:00+08:00", "update", "[pinned]"),
            ev("2026-05-17T09:00:00+08:00", "create", ""),
        ];
        let entries = compute_timeline_entries(&events);
        assert_eq!(entries.len(), 3, "{:?}", entries);
        assert_eq!(entries[0].action, "create");
        assert_eq!(entries[1].action, "update");
        assert_eq!(entries[2].action, "delete");
    }

    #[test]
    fn timeline_compute_entries_drops_noise_update_with_no_markers() {
        // create + 中间一条 update（无 markers，与 create 同空集合）→
        // 中间事件去重，仅保 create。owner 不关心 detail.md silent 写。
        let events = vec![
            ev("2026-05-17T14:00:00+08:00", "update", ""),
            ev("2026-05-17T09:00:00+08:00", "create", ""),
        ];
        let entries = compute_timeline_entries(&events);
        assert_eq!(entries.len(), 1, "{:?}", entries);
        assert_eq!(entries[0].action, "create");
    }

    #[test]
    fn timeline_compute_entries_payload_change_counts_as_change() {
        // [snooze: A] → [snooze: B] 应保留（payload 变化即 token 文本变化）
        let events = vec![
            ev("2026-05-17T14:00:00+08:00", "update", "[snooze: 2026-05-20 18:00]"),
            ev("2026-05-17T10:00:00+08:00", "update", "[snooze: 2026-05-18 18:00]"),
            ev("2026-05-17T09:00:00+08:00", "create", ""),
        ];
        let entries = compute_timeline_entries(&events);
        assert_eq!(entries.len(), 3, "{:?}", entries);
        assert!(entries[1].markers[0].contains("2026-05-18"));
        assert!(entries[2].markers[0].contains("2026-05-20"));
    }

    #[test]
    fn timeline_format_ts_extracts_md_hm() {
        assert_eq!(format_timeline_ts("2026-05-17T18:30:42+08:00"), "05-17 18:30");
    }

    #[test]
    fn timeline_format_ts_falls_back_on_unrecognized_format() {
        assert_eq!(format_timeline_ts("not-a-ts"), "not-a-ts");
    }

    #[test]
    fn timeline_reply_empty_entries_shows_friendly_fallback() {
        let s = format_timeline_reply("写周报", &[], 0);
        assert!(s.contains("写周报"), "should include title: {s}");
        assert!(s.contains("无该 task 的事件记录"), "{s}");
    }

    #[test]
    fn timeline_reply_lists_entries_in_order_with_emoji() {
        let entries = vec![
            TimelineEntry {
                timestamp: "2026-05-15T09:30:00+08:00".to_string(),
                action: "create".to_string(),
                markers: vec![],
            },
            TimelineEntry {
                timestamp: "2026-05-17T14:00:00+08:00".to_string(),
                action: "update".to_string(),
                markers: vec!["[done]".to_string(), "[result: 已发送]".to_string()],
            },
        ];
        let s = format_timeline_reply("写周报", &entries, 2);
        assert!(s.contains("📝 05-15 09:30 · 创建"), "create line: {s}");
        assert!(
            s.contains("✏️ 05-17 14:00 · [done] [result: 已发送]"),
            "update line: {s}"
        );
    }

    #[test]
    fn timeline_reply_caps_at_30_entries_with_overflow_hint() {
        let entries: Vec<TimelineEntry> = (0..50)
            .map(|i| TimelineEntry {
                timestamp: format!("2026-05-17T{:02}:00:00+08:00", i % 24),
                action: "update".to_string(),
                markers: vec![format!("[result: r{}]", i)],
            })
            .collect();
        let s = format_timeline_reply("t", &entries, 50);
        assert!(s.contains("保留前 30 条"), "should show cap notice: {s}");
        assert!(s.contains("剩余 20 条"), "{s}");
    }

    #[test]
    fn timeline_reply_header_shows_deduped_count_when_smaller() {
        let entries = vec![TimelineEntry {
            timestamp: "2026-05-17T09:00:00+08:00".to_string(),
            action: "create".to_string(),
            markers: vec![],
        }];
        // total_events=5 but entries=1 → header notes dedup
        let s = format_timeline_reply("t", &entries, 5);
        assert!(s.contains("5 个事件"), "{s}");
        assert!(s.contains("保留 1 条"), "{s}");
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
