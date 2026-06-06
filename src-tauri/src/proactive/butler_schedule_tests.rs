    use super::*;

    fn fixed_now() -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
            .unwrap()
            .and_hms_opt(14, 30, 0)
            .unwrap()
    }

    #[test]
    fn format_butler_tasks_block_empty_returns_empty() {
        assert_eq!(
            format_butler_tasks_block(&[], 6, 100, fixed_now()),
            String::new()
        );
    }

    #[test]
    fn format_butler_tasks_block_zero_max_returns_empty() {
        let items = vec![("t".into(), "d".into(), "2026-05-03T10:00:00+08:00".into())];
        assert_eq!(
            format_butler_tasks_block(&items, 0, 100, fixed_now()),
            String::new()
        );
    }

    #[test]
    fn format_butler_tasks_block_filters_blocked_tasks() {
        // 「先决」未完成 → 「主任务」被卡，不该出现在 prompt block 里。
        let items = vec![
            (
                "先决任务".into(),
                "[task pri=3] 先做这个".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
            (
                "主任务".into(),
                "[blockedBy: 先决任务] 等先决完成后再做".into(),
                "2026-04-02T10:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("先决任务"));
        assert!(!out.contains("主任务"), "blocked 任务不该出现");
        assert!(
            out.contains("1 条被 [blockedBy: …] 依赖卡住"),
            "header 应透明告知有任务被卡：{out}"
        );
    }

    #[test]
    fn format_butler_tasks_block_filters_silent_tasks() {
        // [silent] 任务从主 list 消失但 header 透明告知 owner 标了 N 条
        let items = vec![
            (
                "活跃任务".into(),
                "[task pri=3] 该做的".into(),
                "2026-04-02T10:00:00+08:00".into(),
            ),
            (
                "静默任务".into(),
                "[silent] 知道但不要选".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("活跃任务"));
        assert!(!out.contains("静默任务"), "silent 任务不该出现在主 list");
        assert!(
            out.contains("1 条被 owner 标 [silent] 不选"),
            "header 应透明告知 silent 计数：{out}"
        );
    }

    #[test]
    fn format_butler_tasks_block_all_silent_returns_special_msg() {
        // 全部任务都被 owner 标 [silent] → 不挂主 list，仅告知 LLM "有任务但全静默"
        let items = vec![
            (
                "X".into(),
                "[silent] A".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
            (
                "Y".into(),
                "[silent] B".into(),
                "2026-04-02T10:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("全部被 owner 标 [silent]"));
        assert!(out.contains("共 2 条"));
        assert!(!out.contains("X"), "silent 任务 title 不该出现");
        assert!(!out.contains("Y"), "silent 任务 title 不该出现");
    }

    #[test]
    fn format_butler_tasks_block_unblocks_after_dep_done() {
        // 先决任务已 [done] → 主任务解锁，应出现在 prompt block 里。
        let items = vec![
            (
                "先决任务".into(),
                "[task pri=3] 已经做完 [done]".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
            (
                "主任务".into(),
                "[blockedBy: 先决任务] 终于可以做了".into(),
                "2026-04-02T10:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("主任务"), "blocker done 后主任务解锁");
        assert!(
            !out.contains("依赖卡住"),
            "全解锁时不该出现 blocked 横幅"
        );
    }

    #[test]
    fn format_butler_tasks_block_filters_snoozed_tasks() {
        // [snooze: future] 的任务不该出现在 prompt block；header 透明告知
        // "另有 N 条处于 snooze 暂停期"。
        // fixed_now() 是 2026-05-04 12:00；snooze 至 2026-05-20 09:00 仍在
        // 未来。
        let items = vec![
            (
                "活跃任务".into(),
                "[task pri=3] 现在就做".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
            (
                "暂停任务".into(),
                "[snooze: 2026-05-20 09:00] 等下个 sprint".into(),
                "2026-04-02T10:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("活跃任务"));
        assert!(!out.contains("暂停任务"), "snoozed 任务不该出现");
        assert!(
            out.contains("1 条处于 [snooze: …] 暂停期"),
            "header 应透明告知 snooze 数：{out}"
        );
    }

    #[test]
    fn format_butler_tasks_block_past_snooze_passes_through() {
        // [snooze: 过去] → 自然失效，任务恢复出现。
        let items = vec![(
            "原暂停".into(),
            "[snooze: 2020-01-01 09:00] 早就该醒".into(),
            "2026-04-02T10:00:00+08:00".into(),
        )];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("原暂停"));
        assert!(!out.contains("暂停期"), "过点 snooze 不应触发 header 标记");
    }

    #[test]
    fn format_butler_tasks_block_blocked_and_snoozed_header() {
        // 同时有 blocked 和 snoozed：transparency line 应列两段。
        let items = vec![
            (
                "blocker".into(),
                "[task pri=3] 先决".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
            (
                "blocked".into(),
                "[blockedBy: blocker] 等先决".into(),
                "2026-04-02T10:00:00+08:00".into(),
            ),
            (
                "snoozed".into(),
                "[snooze: 2026-05-20 09:00] 等等".into(),
                "2026-04-03T10:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("blocker"));
        assert!(!out.contains("blocked："), "blocked 任务不该出现");
        assert!(!out.contains("snoozed："), "snoozed 任务不该出现");
        assert!(out.contains("1 条被 [blockedBy: …] 依赖卡住"));
        assert!(out.contains("1 条处于 [snooze: …] 暂停期"));
    }

    #[test]
    fn format_butler_tasks_block_all_blocked_returns_summary() {
        // 极端：所有任务都被某个 blocker 卡住 → 输出兜底说明而非空。
        let items = vec![
            (
                "blocker".into(),
                "[task pri=3] 先决任务还没做".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
            (
                "a".into(),
                "[blockedBy: blocker] a".into(),
                "2026-04-02T10:00:00+08:00".into(),
            ),
            (
                "b".into(),
                "[blockedBy: blocker] b".into(),
                "2026-04-03T10:00:00+08:00".into(),
            ),
        ];
        // 把 blocker 也变成被卡的：构造循环依赖（极端 footgun，应不死锁）。
        // blocker 本身没 blocked_by 所以 active，主任务被卡 —— 输出含 blocker。
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("blocker"));
        assert!(out.contains("依赖卡住"));
        // title 列以 "- {title}：" 形态出现；用全角冒号锚定避免 "- b" 误匹
        // 配到 "- blocker：" 前缀。
        assert!(!out.contains("- a："));
        assert!(!out.contains("- b："));
    }

    #[test]
    fn format_butler_tasks_block_sorts_oldest_first() {
        let items = vec![
            (
                "新任务".into(),
                "d-new".into(),
                "2026-05-03T10:00:00+08:00".into(),
            ),
            (
                "老任务".into(),
                "d-old".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
            (
                "中任务".into(),
                "d-mid".into(),
                "2026-04-20T10:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        let old_idx = out.find("老任务").unwrap();
        let mid_idx = out.find("中任务").unwrap();
        let new_idx = out.find("新任务").unwrap();
        assert!(
            old_idx < mid_idx,
            "oldest should be first (don't let tasks rot)"
        );
        assert!(mid_idx < new_idx);
    }

    #[test]
    fn format_butler_tasks_block_footer_teaches_speech_mention() {
        // Iter D6: pin the "记得在开口里简短提一下" guidance so a future refactor
        // can't silently drop it.
        let items = vec![(
            "morning-report".into(),
            "[every: 09:00] write today.md".into(),
            "2026-05-03T09:30:00+08:00".into(),
        )];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(
            out.contains("记得在你这一轮的开口里简短提一下") || out.contains("简短提一下"),
            "footer should instruct LLM to mention butler execution in its speech"
        );
        assert!(
            out.contains("我帮你") || out.contains("整理完了"),
            "footer should give concrete example phrasings"
        );
    }

    #[test]
    fn format_butler_tasks_block_caps_count_and_includes_footer() {
        let items: Vec<(String, String, String)> = (0..10)
            .map(|i| {
                (
                    format!("task-{i}"),
                    format!("desc-{i}"),
                    format!("2026-05-{:02}T10:00:00+08:00", i + 1),
                )
            })
            .collect();
        let out = format_butler_tasks_block(&items, 3, 100, fixed_now());
        assert!(out.contains("共 3 条"));
        assert!(out.contains("task-0"));
        assert!(out.contains("task-2"));
        assert!(!out.contains("task-3"), "4th-oldest should be excluded");
        assert!(
            out.contains("memory_edit update") || out.contains("memory_edit delete"),
            "footer should tell LLM how to retire completed tasks"
        );
    }

    #[test]
    fn has_butler_error_detects_marker() {
        assert!(has_butler_error("[error: file not found] write report"));
        assert!(has_butler_error(
            "[every: 09:00] [error: permission denied] morning"
        ));
        assert!(has_butler_error("some text [error] more text"));
        assert!(has_butler_error("[error :spaced] x"));
    }

    #[test]
    fn has_butler_error_negative_cases() {
        assert!(!has_butler_error(""));
        assert!(!has_butler_error("normal task description"));
        assert!(!has_butler_error("[every: 09:00] write daily.md"));
        assert!(!has_butler_error("[once: 2026-05-10 14:00] one-shot"));
        assert!(!has_butler_error("had an error earlier but recovered"));
    }

    #[test]
    fn format_butler_tasks_block_marks_errored_tasks() {
        let items = vec![(
            "morning-report".into(),
            "[every: 09:00] [error: file not found] write today.md".into(),
            "2026-05-03T09:30:00+08:00".into(),
        )];
        let out = format_butler_tasks_block(&items, 6, 200, fixed_now());
        assert!(out.contains("❌ 错误"));
        let header = out.lines().next().unwrap();
        assert!(header.contains("上次执行失败"), "header: {}", header);
    }

    #[test]
    fn format_butler_tasks_block_due_and_errored_co_occur() {
        let items = vec![(
            "report".into(),
            "[every: 09:00] [error: prev fail] write today.md".into(),
            "2026-05-02T08:00:00+08:00".into(),
        )];
        let out = format_butler_tasks_block(&items, 6, 200, fixed_now());
        let body = out.lines().nth(1).unwrap();
        let err_idx = body.find("❌ 错误").unwrap();
        let due_idx = body.find("⏰ 到期").unwrap();
        assert!(err_idx < due_idx, "error marker should precede due marker");
        let header = out.lines().next().unwrap();
        assert!(header.contains("到期"));
        assert!(header.contains("失败"));
    }

    #[test]
    fn format_butler_tasks_block_truncates_long_descriptions() {
        let long = "条".repeat(150);
        let items = vec![("task".into(), long, "2026-05-03T10:00:00+08:00".into())];
        let out = format_butler_tasks_block(&items, 6, 30, fixed_now());
        assert!(out.contains("…"));
        let body_chars = out
            .lines()
            .nth(1)
            .unwrap()
            .chars()
            .filter(|c| *c == '条')
            .count();
        assert_eq!(body_chars, 30);
    }

    #[test]
    fn format_butler_tasks_block_due_task_bubbles_to_top_with_marker() {
        let items = vec![
            (
                "plain-old".into(),
                "do something whenever".into(),
                "2026-04-01T08:00:00+08:00".into(),
            ),
            (
                "morning-report".into(),
                "[every: 09:00] write today.md".into(),
                "2026-05-02T09:30:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(
            out.contains("⏰ 到期"),
            "due task should carry ⏰ 到期 marker"
        );
        assert!(out.contains("其中 1 条到期"));
        let due_idx = out.find("morning-report").unwrap();
        let plain_idx = out.find("plain-old").unwrap();
        assert!(due_idx < plain_idx, "due task ranks above plain older one");
    }

    fn count_task_lines_with_marker(out: &str) -> usize {
        out.lines()
            .filter(|l| l.starts_with("- ") && l.contains("⏰ 到期 · "))
            .count()
    }

    #[test]
    fn format_butler_tasks_block_pinned_task_bubbles_to_top_with_marker() {
        // owner [pinned] 任务上浮到第一行，line 带 "📌 钉住" marker，header 含
        // "其中 1 条由 owner 钉住"。
        let items = vec![
            (
                "plain-old".into(),
                "do something whenever".into(),
                "2026-04-01T08:00:00+08:00".into(),
            ),
            (
                "key-task".into(),
                "[task pri=3] crucial work [pinned]".into(),
                "2026-05-02T08:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("📌 钉住"), "pinned should carry marker: {out}");
        assert!(
            out.contains("1 条由 owner 钉住"),
            "header should reflect pin count: {out}"
        );
        let pinned_idx = out.find("key-task").unwrap();
        let plain_idx = out.find("plain-old").unwrap();
        assert!(pinned_idx < plain_idx, "pinned ranks above plain older");
    }

    #[test]
    fn format_butler_tasks_block_pinned_dominates_due_in_ordering() {
        // pinned 优先级高于 due —— owner 的标注覆盖系统的时间窗信号。
        // due 任务（[every: 09:00] now=12:00）和 pinned 任务都"该做"，
        // pinned 排在前。两者的 marker 都正确显示。
        let items = vec![
            (
                "morning-report".into(),
                "[every: 09:00] write today.md".into(),
                "2026-05-02T09:30:00+08:00".into(),
            ),
            (
                "pinned-task".into(),
                "[task pri=3] 主人盯的 [pinned]".into(),
                "2026-05-02T08:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        let pin_idx = out.find("pinned-task").unwrap();
        let due_idx = out.find("morning-report").unwrap();
        assert!(
            pin_idx < due_idx,
            "pinned should outrank due — owner intent over system signal: {out}"
        );
        // 双 marker 都正确显示（不互相吃掉）
        assert!(out.contains("📌 钉住"));
        assert!(out.contains("⏰ 到期"));
        // 两个 count 都在 header 里
        assert!(out.contains("1 条由 owner 钉住"));
        assert!(out.contains("1 条到期"));
    }

    #[test]
    fn format_butler_tasks_block_no_pinned_means_no_pin_phrase_in_header() {
        // 无 pinned 任务时 header 行 + task line 都不出现 "📌 钉住" 标记 ——
        // 避免给 LLM 假信号。footer 始终带 "看到「📌 钉住」" 教学文案，故
        // 仅校验前 N 行（header + task lines）而非整体 output。
        let items = vec![(
            "plain".into(),
            "[task pri=3] 普通任务".into(),
            "2026-04-01T08:00:00+08:00".into(),
        )];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        let header = out.lines().next().unwrap();
        assert!(
            !header.contains("钉住"),
            "no pinned task → header doesn't mention 钉住: {header}"
        );
        // task line 也不能带 📌 marker
        let task_lines: Vec<&str> = out.lines().filter(|l| l.starts_with("- ")).collect();
        for line in &task_lines {
            assert!(!line.contains("📌"), "task line: {line}");
        }
    }

    #[test]
    fn format_butler_tasks_block_already_done_today_not_due() {
        let items = vec![(
            "morning-report".into(),
            "[every: 09:00] write today.md".into(),
            "2026-05-03T09:15:00+08:00".into(),
        )];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert_eq!(
            count_task_lines_with_marker(&out),
            0,
            "no task line should carry the marker"
        );
        let header = out.lines().next().unwrap();
        assert!(!header.contains("条到期"), "header: {}", header);
        assert!(out.contains("morning-report"));
    }

    #[test]
    fn parse_butler_schedule_prefix_parses_every() {
        let (sched, topic) = parse_butler_schedule_prefix("[every: 09:00] write today.md").unwrap();
        assert_eq!(sched, ButlerSchedule::Every(9, 0));
        assert_eq!(topic, "write today.md");
    }

    #[test]
    fn parse_weekday_set_keyword_basic() {
        // 工作日 / 周末（中英）
        assert_eq!(parse_weekday_set_keyword("工作日"), Some(WEEKDAY_MASK_WORKDAYS));
        assert_eq!(parse_weekday_set_keyword("周一到周五"), Some(WEEKDAY_MASK_WORKDAYS));
        assert_eq!(parse_weekday_set_keyword("weekday"), Some(WEEKDAY_MASK_WORKDAYS));
        assert_eq!(parse_weekday_set_keyword("WEEKDAYS"), Some(WEEKDAY_MASK_WORKDAYS));
        assert_eq!(parse_weekday_set_keyword("周末"), Some(WEEKDAY_MASK_WEEKEND));
        assert_eq!(parse_weekday_set_keyword("双休"), Some(WEEKDAY_MASK_WEEKEND));
        assert_eq!(parse_weekday_set_keyword("Weekend"), Some(WEEKDAY_MASK_WEEKEND));
        // 单 weekday（中英）
        assert_eq!(parse_weekday_set_keyword("周一"), Some(1 << 0));
        assert_eq!(parse_weekday_set_keyword("星期三"), Some(1 << 2));
        assert_eq!(parse_weekday_set_keyword("周日"), Some(1 << 6));
        assert_eq!(parse_weekday_set_keyword("Friday"), Some(1 << 4));
        assert_eq!(parse_weekday_set_keyword("Sat"), Some(1 << 5));
        // 不识别
        assert_eq!(parse_weekday_set_keyword(""), None);
        assert_eq!(parse_weekday_set_keyword("noday"), None);
        assert_eq!(parse_weekday_set_keyword("后天"), None);
    }

    #[test]
    fn parse_butler_schedule_prefix_parses_every_weekday_set() {
        let (sched, topic) =
            parse_butler_schedule_prefix("[every: 工作日 09:00] standup").unwrap();
        assert_eq!(
            sched,
            ButlerSchedule::EveryOnWeekdays(WEEKDAY_MASK_WORKDAYS, 9, 0)
        );
        assert_eq!(topic, "standup");
        let (sched2, _) =
            parse_butler_schedule_prefix("[every: 周末 10:00] 整理桌面").unwrap();
        assert_eq!(
            sched2,
            ButlerSchedule::EveryOnWeekdays(WEEKDAY_MASK_WEEKEND, 10, 0)
        );
        let (sched3, _) =
            parse_butler_schedule_prefix("[every: 周一 09:00] mon-standup").unwrap();
        assert_eq!(
            sched3,
            ButlerSchedule::EveryOnWeekdays(1 << 0, 9, 0)
        );
    }

    #[test]
    fn parse_butler_schedule_prefix_rejects_invalid_weekday() {
        // weekday-set 识别失败 → 整段 None（不退化为纯 HH:MM 解释 "后天 09:00"）
        assert!(parse_butler_schedule_prefix("[every: 后天 09:00] x").is_none());
    }

    #[test]
    fn is_butler_due_every_weekday_set() {
        // 2026-05-04 周一 10:00 —— 工作日 mask + 09:00 触发：今日 mask 命中 + 时刻已过
        let mon_10am = chrono::NaiveDate::from_ymd_opt(2026, 5, 4).unwrap()
            .and_hms_opt(10, 0, 0).unwrap();
        let workdays_9am = ButlerSchedule::EveryOnWeekdays(WEEKDAY_MASK_WORKDAYS, 9, 0);
        assert!(is_butler_due(&workdays_9am, mon_10am, ""), "周一 10:00 工作日 09:00 fire 过应 due");
        // 上次更新在今日 fire 之后 → 不 due
        assert!(!is_butler_due(&workdays_9am, mon_10am, "2026-05-04T09:30:00+08:00"));

        // 2026-05-09 周六 10:00 —— 工作日 mask 不命中今日 → 看回最近周五 09:00
        let sat_10am = chrono::NaiveDate::from_ymd_opt(2026, 5, 9).unwrap()
            .and_hms_opt(10, 0, 0).unwrap();
        // 上次更新在周五 fire 之前 → 还 due（周五的 fire 还没人做）
        assert!(is_butler_due(&workdays_9am, sat_10am, "2026-05-08T08:00:00+08:00"));
        // 上次更新在周五 fire 之后 → 不 due
        assert!(!is_butler_due(&workdays_9am, sat_10am, "2026-05-08T15:00:00+08:00"));

        // 周末 mask + 周一 10:00 → 看回最近周日 10:00 → due if not done
        let weekend_10am = ButlerSchedule::EveryOnWeekdays(WEEKDAY_MASK_WEEKEND, 10, 0);
        assert!(is_butler_due(&weekend_10am, mon_10am, ""), "周一 10:00 周末 10:00 fire 看周日 应 due");
    }

    #[test]
    fn parse_butler_schedule_prefix_parses_once() {
        let (sched, topic) =
            parse_butler_schedule_prefix("[once: 2026-05-10 14:00] one-shot").unwrap();
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 5, 10)
            .unwrap()
            .and_hms_opt(14, 0, 0)
            .unwrap();
        assert_eq!(sched, ButlerSchedule::Once(expected));
        assert_eq!(topic, "one-shot");
    }

    #[test]
    fn parse_butler_schedule_prefix_rejects_malformed() {
        assert!(parse_butler_schedule_prefix("no prefix").is_none());
        assert!(parse_butler_schedule_prefix("[every: 25:00] x").is_none());
        assert!(parse_butler_schedule_prefix("[every: 09:60] x").is_none());
        assert!(parse_butler_schedule_prefix("[once: not-a-date] x").is_none());
        assert!(
            parse_butler_schedule_prefix("[every: 09:00]").is_none(),
            "empty topic"
        );
        assert!(parse_butler_schedule_prefix("[remind: 09:00] reminder").is_none());
    }

    #[test]
    fn is_butler_due_every_basic_window() {
        let now = fixed_now();
        assert!(is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "2026-05-02T08:00:00+08:00"
        ));
        assert!(is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "2026-05-02T10:00:00+08:00"
        ));
        assert!(!is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "2026-05-03T09:30:00+08:00"
        ));
    }

    #[test]
    fn is_butler_due_every_before_today_target() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap();
        assert!(!is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "2026-05-02T09:30:00+08:00"
        ));
        assert!(is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "2026-05-01T08:00:00+08:00"
        ));
    }

    #[test]
    fn is_butler_due_once_semantics() {
        let now = fixed_now();
        let target = ButlerSchedule::Once(
            chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
                .unwrap()
                .and_hms_opt(10, 0, 0)
                .unwrap(),
        );
        assert!(is_butler_due(&target, now, ""));
        assert!(is_butler_due(&target, now, "2026-05-03T09:00:00+08:00"));
        assert!(!is_butler_due(&target, now, "2026-05-03T11:00:00+08:00"));
        let future = ButlerSchedule::Once(
            chrono::NaiveDate::from_ymd_opt(2026, 5, 10)
                .unwrap()
                .and_hms_opt(10, 0, 0)
                .unwrap(),
        );
        assert!(!is_butler_due(&future, now, ""));
    }

    #[test]
    fn is_completed_once_basic_flow() {
        let desc = "[once: 2026-05-03 10:00] do something";
        let target_done = "2026-05-03T10:30:00+08:00";
        let now1 = chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
            .unwrap()
            .and_hms_opt(11, 30, 0)
            .unwrap();
        assert!(!is_completed_once(desc, target_done, now1, 48));
        let now2 = chrono::NaiveDate::from_ymd_opt(2026, 5, 5)
            .unwrap()
            .and_hms_opt(11, 0, 0)
            .unwrap();
        assert!(is_completed_once(desc, target_done, now2, 48));
    }

    #[test]
    fn is_completed_once_not_yet_executed() {
        let desc = "[once: 2026-05-03 10:00] do something";
        let last = "2026-05-02T08:00:00+08:00";
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 6)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert!(!is_completed_once(desc, last, now, 48));
    }

    #[test]
    fn is_completed_once_skips_every_tasks() {
        let desc = "[every: 09:00] daily report";
        let last = "2026-05-03T09:30:00+08:00";
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 10)
            .unwrap()
            .and_hms_opt(15, 0, 0)
            .unwrap();
        assert!(!is_completed_once(desc, last, now, 48));
    }

    #[test]
    fn is_completed_once_skips_unprefixed_tasks() {
        let desc = "no schedule prefix here";
        let last = "2026-05-03T09:30:00+08:00";
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 10)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert!(!is_completed_once(desc, last, now, 48));
    }

    #[test]
    fn is_completed_once_unparseable_updated_at_keeps_task() {
        let desc = "[once: 2026-05-03 10:00] x";
        let now = chrono::NaiveDate::from_ymd_opt(2026, 6, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert!(!is_completed_once(desc, "garbage", now, 48));
        assert!(!is_completed_once(desc, "", now, 48));
    }

    #[test]
    fn is_butler_due_unparseable_updated_at_treated_as_never() {
        let now = fixed_now();
        assert!(is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "not-a-timestamp"
        ));
        assert!(is_butler_due(&ButlerSchedule::Every(9, 0), now, ""));
    }

    // -- is_archive_candidate -----------------------------------------------

    fn day(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// 把本地日期 + 时分铸成 RFC3339 字符串，与 `now_iso()` 写盘格式一致：
    /// `YYYY-MM-DDTHH:MM:SS+HH:MM`。
    fn local_iso(y: i32, m: u32, d: u32, hour: u32, minute: u32) -> String {
        use chrono::TimeZone;
        let naive = day(y, m, d).and_hms_opt(hour, minute, 0).unwrap();
        chrono::Local
            .from_local_datetime(&naive)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string()
    }

    #[test]
    fn is_archive_candidate_done_past_threshold_returns_true() {
        let updated = local_iso(2026, 4, 1, 10, 0);
        assert!(is_archive_candidate(
            "[task pri=1] 整理 [done] [result: 完成]",
            &updated,
            day(2026, 5, 10),
            30,
        ));
    }

    #[test]
    fn is_archive_candidate_cancelled_past_threshold_returns_true() {
        let updated = local_iso(2026, 4, 1, 10, 0);
        assert!(is_archive_candidate(
            "[task pri=1] 整理 [cancelled: 不需要了]",
            &updated,
            day(2026, 5, 10),
            30,
        ));
    }

    #[test]
    fn is_archive_candidate_pending_never_archived() {
        let updated = local_iso(2026, 1, 1, 10, 0);
        assert!(!is_archive_candidate(
            "[task pri=1] 整理",
            &updated,
            day(2026, 5, 10),
            30,
        ));
    }

    #[test]
    fn is_archive_candidate_error_never_archived() {
        let updated = local_iso(2026, 1, 1, 10, 0);
        assert!(!is_archive_candidate(
            "[task pri=1] 整理 [error: 文件不存在]",
            &updated,
            day(2026, 5, 10),
            30,
        ));
    }

    #[test]
    fn is_archive_candidate_within_threshold_returns_false() {
        let updated = local_iso(2026, 5, 5, 10, 0);
        assert!(!is_archive_candidate(
            "[task pri=1] 整理 [done]",
            &updated,
            day(2026, 5, 10),
            30,
        ));
    }

    #[test]
    fn is_archive_candidate_zero_retention_disables_archive() {
        let updated = local_iso(2020, 1, 1, 10, 0);
        assert!(!is_archive_candidate(
            "[task pri=1] 整理 [done]",
            &updated,
            day(2026, 5, 10),
            0,
        ));
    }

    #[test]
    fn is_archive_candidate_unparseable_updated_at_skipped() {
        assert!(!is_archive_candidate(
            "[task pri=1] 整理 [done]",
            "garbage",
            day(2026, 5, 10),
            30,
        ));
        assert!(!is_archive_candidate(
            "[task pri=1] 整理 [done]",
            "",
            day(2026, 5, 10),
            30,
        ));
    }

    #[test]
    fn is_archive_candidate_exactly_at_threshold_archives() {
        // updated_at 距 today 整 30 天 → 满足 >= 30 等号；归档。
        let updated = local_iso(2026, 4, 10, 10, 0);
        assert!(is_archive_candidate(
            "[task pri=1] 整理 [done]",
            &updated,
            day(2026, 5, 10),
            30,
        ));
    }

    // -- Iter R77: deadline parsing + urgency + format ----------------------

    fn dt(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, minute, 0)
            .unwrap()
    }

    #[test]
    fn parse_deadline_clean() {
        let (when, topic) =
            parse_butler_deadline_prefix("[deadline: 2026-05-10 14:00] reply to email").unwrap();
        assert_eq!(when, dt(2026, 5, 10, 14, 0));
        assert_eq!(topic, "reply to email");
    }

    #[test]
    fn parse_deadline_rejects_malformed() {
        assert!(parse_butler_deadline_prefix("no prefix").is_none());
        assert!(parse_butler_deadline_prefix("[deadline: not-a-date 14:00] x").is_none());
        assert!(parse_butler_deadline_prefix("[deadline: 2026-05-10 25:00] x").is_none());
        assert!(parse_butler_deadline_prefix("[deadline: 2026-05-10 14:00]").is_none());
        // Wrong prefix kind shouldn't match.
        assert!(parse_butler_deadline_prefix("[once: 2026-05-10 14:00] x").is_none());
        assert!(parse_butler_deadline_prefix("[every: 09:00] x").is_none());
    }

    #[test]
    fn urgency_overdue_when_now_at_or_past_deadline() {
        let dl = dt(2026, 5, 10, 14, 0);
        assert_eq!(compute_deadline_urgency(dl, dl), DeadlineUrgency::Overdue);
        assert_eq!(
            compute_deadline_urgency(dl, dt(2026, 5, 10, 15, 0)),
            DeadlineUrgency::Overdue
        );
    }

    #[test]
    fn urgency_imminent_within_one_hour() {
        let dl = dt(2026, 5, 10, 14, 0);
        // 30 min away.
        assert_eq!(
            compute_deadline_urgency(dl, dt(2026, 5, 10, 13, 30)),
            DeadlineUrgency::Imminent
        );
        // 59 min away — still Imminent (< 1h boundary).
        assert_eq!(
            compute_deadline_urgency(dl, dt(2026, 5, 10, 13, 1)),
            DeadlineUrgency::Imminent
        );
    }

    #[test]
    fn urgency_approaching_between_1_and_6_hours() {
        let dl = dt(2026, 5, 10, 14, 0);
        // 1h 30min away.
        assert_eq!(
            compute_deadline_urgency(dl, dt(2026, 5, 10, 12, 30)),
            DeadlineUrgency::Approaching
        );
        // Exactly 6h away — boundary case (≥ 6 → Distant per impl).
        assert_eq!(
            compute_deadline_urgency(dl, dt(2026, 5, 10, 8, 0)),
            DeadlineUrgency::Distant
        );
        // 5h 30min away — Approaching.
        assert_eq!(
            compute_deadline_urgency(dl, dt(2026, 5, 10, 8, 30)),
            DeadlineUrgency::Approaching
        );
    }

    #[test]
    fn urgency_distant_when_far_away() {
        let dl = dt(2026, 5, 10, 14, 0);
        assert_eq!(
            compute_deadline_urgency(dl, dt(2026, 5, 9, 14, 0)),
            DeadlineUrgency::Distant
        );
    }

    #[test]
    fn format_deadlines_hint_skips_distant_only() {
        let now = dt(2026, 5, 10, 8, 0);
        let items = vec![(dt(2026, 5, 12, 14, 0), "tomorrow's report".to_string())];
        assert_eq!(format_butler_deadlines_hint(&items, now), "");
    }

    #[test]
    fn format_deadlines_hint_renders_imminent_with_minutes() {
        let now = dt(2026, 5, 10, 13, 30);
        let items = vec![(dt(2026, 5, 10, 14, 0), "send draft".to_string())];
        let out = format_butler_deadlines_hint(&items, now);
        assert!(out.contains("[逼近的 deadline]"));
        assert!(out.contains("send draft"));
        assert!(out.contains("仅剩 30 分钟"));
    }

    #[test]
    fn format_deadlines_hint_renders_approaching_with_hours() {
        let now = dt(2026, 5, 10, 11, 0);
        let items = vec![(dt(2026, 5, 10, 14, 0), "review PR".to_string())];
        let out = format_butler_deadlines_hint(&items, now);
        assert!(out.contains("review PR"));
        assert!(out.contains("约 3 小时后"));
    }

    #[test]
    fn format_deadlines_hint_renders_overdue_minutes_then_hours() {
        let now = dt(2026, 5, 10, 14, 30);
        let items = vec![(dt(2026, 5, 10, 14, 0), "overdue thing".to_string())];
        let out = format_butler_deadlines_hint(&items, now);
        assert!(out.contains("已过 30 分钟"));
        // Overdue > 1 hour → format in hours.
        let now2 = dt(2026, 5, 10, 17, 0);
        let out2 = format_butler_deadlines_hint(&items, now2);
        assert!(out2.contains("已过 3 小时"));
    }

    #[test]
    fn format_deadlines_hint_handles_mixed_items() {
        // Mix of distant + approaching + overdue. Distant skipped, others rendered.
        let now = dt(2026, 5, 10, 12, 0);
        let items = vec![
            (dt(2026, 5, 15, 14, 0), "future task".to_string()), // Distant
            (dt(2026, 5, 10, 14, 0), "soon thing".to_string()),  // Approaching
            (dt(2026, 5, 10, 11, 0), "missed thing".to_string()), // Overdue
        ];
        let out = format_butler_deadlines_hint(&items, now);
        assert!(!out.contains("future task")); // distant filtered
        assert!(out.contains("soon thing"));
        assert!(out.contains("missed thing"));
    }

    // -- Iter R78: count_urgent_butler_deadlines tests ----------------------

    #[test]
    fn urgent_count_zero_for_distant_and_approaching_only() {
        // Approaching (1-6h) and Distant (>6h) don't count toward "urgent".
        let now = dt(2026, 5, 10, 8, 0);
        let items = vec![
            (dt(2026, 5, 10, 12, 0), "approaching".to_string()), // 4h away
            (dt(2026, 5, 12, 12, 0), "distant".to_string()),     // 2 days away
        ];
        assert_eq!(count_urgent_butler_deadlines(&items, now), 0);
    }

    #[test]
    fn urgent_count_includes_imminent_and_overdue() {
        let now = dt(2026, 5, 10, 12, 0);
        let items = vec![
            (dt(2026, 5, 10, 12, 30), "imminent".to_string()), // 30 min away
            (dt(2026, 5, 10, 11, 0), "overdue".to_string()),   // 1h ago
            (dt(2026, 5, 10, 14, 0), "approaching".to_string()), // 2h away — no
        ];
        assert_eq!(count_urgent_butler_deadlines(&items, now), 2);
    }

    #[test]
    fn urgent_count_empty_input_zero() {
        let now = dt(2026, 5, 10, 12, 0);
        assert_eq!(count_urgent_butler_deadlines(&[], now), 0);
    }

    // -- Iter R81: deadline_urgency_factor tests ----------------------------

    #[test]
    fn deadline_factor_zero_urgent_returns_one() {
        // No urgent deadlines → no shrink. Steady-state.
        assert_eq!(deadline_urgency_factor(0), 1.0);
    }

    #[test]
    fn deadline_factor_single_urgent_halves_cooldown() {
        // One Imminent or Overdue deadline → cooldown × 0.5.
        assert_eq!(deadline_urgency_factor(1), 0.5);
    }

    #[test]
    fn deadline_factor_many_urgent_still_half() {
        // Discrete switch — count > 1 doesn't shrink further. Magnitude is
        // expressed in the prompt-side hint (R77/R79), not the gate factor.
        assert_eq!(deadline_urgency_factor(5), 0.5);
        assert_eq!(deadline_urgency_factor(100), 0.5);
    }
