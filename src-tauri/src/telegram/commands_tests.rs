    use super::*;
    use chrono::TimeZone;

    // -------- GOAL 061 essential command registry --------

    #[test]
    fn essential_registry_under_tg_api_limit() {
        // TG setMyCommands 上限 100 / 单 scope；essential = ≤ 20，custom budget
        // 也 ≤ 20，总和必须 < 100 留余量，单 essential 必须 ≤ 20。
        assert!(
            ESSENTIAL_TG_COMMAND_NAMES.len() <= 20,
            "ESSENTIAL_TG_COMMAND_NAMES 不能超 20（spec「日常高频 + 用户必知 ≤ 20」）"
        );
        assert!(
            ESSENTIAL_TG_COMMAND_NAMES.len() + ESSENTIAL_TG_CUSTOM_BUDGET < 100,
            "essential + custom 总预算应 < 100 留余量"
        );
    }

    #[test]
    fn essential_registry_returns_only_essential_hardcoded() {
        let out = essential_tg_command_registry(&[], "zh");
        let names: std::collections::HashSet<&str> = out.iter().map(|(n, _)| n.as_str()).collect();
        // 所有返回名都必须在 essential 列表内（无 custom 时）
        for name in &names {
            assert!(
                ESSENTIAL_TG_COMMAND_NAMES.contains(name),
                "{} 不应出现在无 custom 的 essential registry",
                name
            );
        }
        // 应大体匹配 essential 名单——所有 essential 都在 hardcoded 里时全命中；
        // 个别 essential 名（如 "aware"）可能未在 lang="zh" 的 hardcoded 矩阵
        // 里 → registry 自然跳过，断言「≥ essential / 2」给宽松下限
        assert!(
            out.len() >= ESSENTIAL_TG_COMMAND_NAMES.len() / 2,
            "registry 命中数应覆盖 essential 列表大半；当前 {} / {}",
            out.len(),
            ESSENTIAL_TG_COMMAND_NAMES.len()
        );
    }

    #[test]
    fn essential_registry_includes_custom_within_budget() {
        let custom = vec![
            crate::commands::settings::TgCustomCommand {
                name: "my_custom_a".to_string(),
                description: "测试自定义 A".to_string(),
            },
            crate::commands::settings::TgCustomCommand {
                name: "my_custom_b".to_string(),
                description: "测试自定义 B".to_string(),
            },
        ];
        let out = essential_tg_command_registry(&custom, "zh");
        let names: std::collections::HashSet<&str> = out.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains("my_custom_a"));
        assert!(names.contains("my_custom_b"));
        // essential 命令应仍在
        assert!(names.contains("task"));
        assert!(names.contains("done"));
    }

    #[test]
    fn essential_registry_caps_custom_at_budget() {
        // 超 budget 时多余 custom 应被丢弃
        let custom: Vec<crate::commands::settings::TgCustomCommand> =
            (0..(ESSENTIAL_TG_CUSTOM_BUDGET + 10))
                .map(|i| crate::commands::settings::TgCustomCommand {
                    name: format!("cust_{}", i),
                    description: format!("cust {}", i),
                })
                .collect();
        let out = essential_tg_command_registry(&custom, "zh");
        let custom_count = out
            .iter()
            .filter(|(n, _)| n.starts_with("cust_"))
            .count();
        assert!(
            custom_count <= ESSENTIAL_TG_CUSTOM_BUDGET,
            "custom 超 budget 应截断；实际 {} / budget {}",
            custom_count,
            ESSENTIAL_TG_CUSTOM_BUDGET
        );
    }

    #[test]
    fn essential_registry_total_well_under_100() {
        // 与上限 100 留至少 50 余量——给 TG API 端的隐式规则（如未来加 scope
        // 维度命令）留缓冲
        let custom: Vec<crate::commands::settings::TgCustomCommand> = (0..50)
            .map(|i| crate::commands::settings::TgCustomCommand {
                name: format!("c_{}", i),
                description: format!("desc {}", i),
            })
            .collect();
        let out = essential_tg_command_registry(&custom, "zh");
        assert!(
            out.len() < 100,
            "essential registry 总条数应 < 100；当前 {}",
            out.len()
        );
    }

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
            "today_done", "yesterday", "streak", "now", "last_speech", "show_speech", "last", "random", "sleep", "sleep_until", "snooze_until", "quick",
            "due", "recent", "oldest_n", "active_recent", "digest", "edit", "pri", "swap_priority", "promote", "demote",
            "reflect", "feedback", "feedback_history", "transient",
            "silent_all", "alarms", "recent_chats", "aware", "here",
            "tag", "tags_for", "touch", "edit_due", "cancel_all_error", "promote_all_p7", "touch_all_p7", "find", "find_in_detail",
            "show", "peek", "peek_pinned", "dup", "snippets", "recent_events", "touched_today", "touched_yesterday", "touched_thisweek", "oldest_done", "edit_title", "cascade_rename", "mute_today", "digest_yesterday", "digest_thisweek", "search_today", "search_yesterday", "search_thisweek", "alarms_today", "alarms_thisweek", "tags_today", "tags_yesterday", "tags_thisweek", "find_in_detail_today", "find_in_detail_yesterday", "random_pinned", "idle_7d", "recent_pins", "help_table", "audit_summary", "cat_top", "timeline", "blocked", "forks", "blocked_by", "snoozed", "reset",
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
            "today_done", "yesterday", "streak", "now", "last_speech", "show_speech", "last", "random", "sleep", "sleep_until", "snooze_until", "quick",
            "due", "edit", "edit_due", "pri", "swap_priority", "promote", "demote", "reflect",
            "feedback", "feedback_history", "transient", "silent_all",
            "alarms", "recent_chats", "aware", "here", "cancel_all_error",
            "promote_all_p7", "touch_all_p7", "pin_all_p7", "consolidate_now", "active_recent", "find_in_detail", "find_in_detail_today", "find_in_detail_yesterday", "search_today", "search_yesterday", "search_thisweek", "show", "peek", "peek_pinned", "dup", "snippets", "recent_events", "touched_today", "touched_yesterday", "touched_thisweek", "oldest_done", "edit_title", "cascade_rename", "mute_today", "digest_yesterday", "digest_thisweek", "alarms_today", "alarms_thisweek", "tags_today", "tags_yesterday", "tags_thisweek", "random_pinned", "idle_7d", "recent_pins", "help_table", "audit_summary", "cat_top", "timeline", "forks", "blocked_by",
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

    // -------- /snooze_until parse + format --------

    #[test]
    fn snooze_until_parses_title_and_time() {
        assert_eq!(
            parse_tg_command("/snooze_until 整理 Downloads 18:00"),
            Some(TgCommand::SnoozeUntil {
                title: "整理 Downloads".to_string(),
                time: Some((18, 0)),
            })
        );
        assert_eq!(
            parse_tg_command("/snooze_until 写周报 9"),
            Some(TgCommand::SnoozeUntil {
                title: "写周报".to_string(),
                time: Some((9, 0)),
            })
        );
    }

    #[test]
    fn snooze_until_parses_empty_arg() {
        assert_eq!(
            parse_tg_command("/snooze_until"),
            Some(TgCommand::SnoozeUntil {
                title: String::new(),
                time: None,
            })
        );
    }

    #[test]
    fn snooze_until_invalid_time_falls_into_title_time_none() {
        // 末尾不是合法 HH:MM → 整段当 title，time=None
        assert_eq!(
            parse_tg_command("/snooze_until 整理 Downloads laterxx"),
            Some(TgCommand::SnoozeUntil {
                title: "整理 Downloads laterxx".to_string(),
                time: None,
            })
        );
    }

    #[test]
    fn format_snooze_until_empty_title_shows_usage() {
        let s = format_snooze_until_reply("", None, None, false, Ok(()));
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/snooze_until <title> <HH:MM>"), "{s}");
    }

    #[test]
    fn format_snooze_until_invalid_time_shows_error() {
        let s = format_snooze_until_reply(
            "整理 Downloads",
            None,
            None,
            false,
            Ok(()),
        );
        assert!(s.contains("不是合法时刻"), "{s}");
        assert!(s.contains("整理 Downloads"), "echoes title: {s}");
    }

    #[test]
    fn format_snooze_until_save_failure_shows_reason() {
        let s = format_snooze_until_reply(
            "missing_task",
            Some((18, 0)),
            None,
            false,
            Err("task not found: missing_task".to_string()),
        );
        assert!(s.contains("设 snooze 失败"), "{s}");
        assert!(s.contains("not found"), "{s}");
    }

    #[test]
    fn format_snooze_until_success_shows_target() {
        let until = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 18, 0, 0)
            .unwrap();
        let s = format_snooze_until_reply(
            "整理 Downloads",
            Some((18, 0)),
            Some(until),
            false,
            Ok(()),
        );
        assert!(s.contains("💤"), "{s}");
        assert!(s.contains("整理 Downloads"), "{s}");
        assert!(s.contains("18:00"), "{s}");
        assert!(s.contains("/unsnooze 整理 Downloads"), "follow-up hint: {s}");
        assert!(!s.contains("明日同时刻"), "no cross-midnight: {s}");
    }

    #[test]
    fn format_snooze_until_cross_midnight_adds_hint() {
        let until = chrono::Local
            .with_ymd_and_hms(2026, 5, 18, 9, 0, 0)
            .unwrap();
        let s = format_snooze_until_reply(
            "写周报",
            Some((9, 0)),
            Some(until),
            true,
            Ok(()),
        );
        assert!(s.contains("明日同时刻"), "cross-midnight hint: {s}");
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

    // -------- /tags_thisweek parse + format --------

    #[test]
    fn tags_thisweek_parser_no_arg() {
        assert_eq!(
            parse_tg_command("/tags_thisweek"),
            Some(TgCommand::TagsThisweek),
        );
        assert_eq!(
            parse_tg_command("/TAGS_THISWEEK"),
            Some(TgCommand::TagsThisweek),
        );
    }

    #[test]
    fn tags_thisweek_empty_shows_week_specific_fallback() {
        let ws = chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap(); // Mon
        let s = format_tags_thisweek_reply(&[], ws);
        assert!(s.contains("本周（2026-05-11 起）"), "{s}");
        assert!(s.contains("都无 #tag"), "{s}");
        assert!(s.contains("/tags_today"), "{s}");
        assert!(s.contains("/touched_thisweek"), "{s}");
    }

    #[test]
    fn tags_thisweek_aggregates_week_only() {
        let ws = chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap();
        // Mon - 周内
        let mut mon = view("a", 3, None, TaskStatus::Pending, None);
        mon.updated_at = "2026-05-11T09:00:00+08:00".to_string();
        mon.tags = vec!["work".to_string(), "API".to_string()];
        // Wed - 周内
        let mut wed = view("b", 3, None, TaskStatus::Done, Some("r"));
        wed.updated_at = "2026-05-13T15:00:00+08:00".to_string();
        wed.tags = vec!["work".to_string()];
        // 上周日 - 周外
        let mut last_sun = view("c", 3, None, TaskStatus::Done, Some("r"));
        last_sun.updated_at = "2026-05-10T20:00:00+08:00".to_string();
        last_sun.tags = vec!["lastweek".to_string()];
        let s = format_tags_thisweek_reply(&[mon, wed, last_sun], ws);
        assert!(s.contains("· #work ×2"), "this-week work counted twice: {s}");
        assert!(s.contains("· #API ×1"), "{s}");
        assert!(!s.contains("lastweek"), "previous week excluded: {s}");
        assert!(s.contains("2 个 tag"), "{s}");
    }

    // -------- /tags_yesterday parse + format --------

    #[test]
    fn tags_yesterday_parser_no_arg() {
        assert_eq!(
            parse_tg_command("/tags_yesterday"),
            Some(TgCommand::TagsYesterday),
        );
        assert_eq!(
            parse_tg_command("/TAGS_YESTERDAY"),
            Some(TgCommand::TagsYesterday),
        );
    }

    #[test]
    fn tags_yesterday_empty_shows_yesterday_specific_fallback() {
        let y = chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let s = format_tags_yesterday_reply(&[], y);
        assert!(s.contains("昨日（2026-05-16）"), "{s}");
        assert!(s.contains("都无 #tag"), "{s}");
        assert!(s.contains("/tags_today"), "alt /tags_today: {s}");
        assert!(s.contains("/touched_yesterday"), "alt /touched_yesterday: {s}");
    }

    #[test]
    fn tags_yesterday_filters_yesterday_only() {
        let y = chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        // 昨日 + tags
        let mut t1 = view("a", 3, None, TaskStatus::Pending, None);
        t1.updated_at = "2026-05-16T09:00:00+08:00".to_string();
        t1.tags = vec!["work".to_string()];
        // 今日 + tags（应被排除）
        let mut t2 = view("b", 3, None, TaskStatus::Done, Some("r"));
        t2.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        t2.tags = vec!["today-only".to_string()];
        let s = format_tags_yesterday_reply(&[t1, t2], y);
        assert!(s.contains("· #work ×1"), "yesterday tag included: {s}");
        assert!(!s.contains("today-only"), "today tag excluded: {s}");
        assert!(s.contains("1 个 tag"), "{s}");
    }

    // -------- /tags_today parse + format --------

    #[test]
    fn tags_today_parser_no_arg() {
        assert_eq!(parse_tg_command("/tags_today"), Some(TgCommand::TagsToday));
        assert_eq!(parse_tg_command("/TAGS_TODAY"), Some(TgCommand::TagsToday));
        // 尾部 token 容忍
        assert_eq!(
            parse_tg_command("/tags_today extra"),
            Some(TgCommand::TagsToday),
        );
    }

    #[test]
    fn tags_today_empty_shows_friendly_fallback() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let s = format_tags_today_reply(&[], today);
        assert!(s.contains("今日（2026-05-17）"), "{s}");
        assert!(s.contains("都无 #tag"), "{s}");
        assert!(s.contains("/tags"), "should mention alt /tags: {s}");
    }

    #[test]
    fn tags_today_aggregates_today_tags_only() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        // 今日 + 含 tag
        let mut t1 = view("写 API doc", 3, None, TaskStatus::Pending, None);
        t1.updated_at = "2026-05-17T09:00:00+08:00".to_string();
        t1.tags = vec!["work".to_string(), "API".to_string()];
        let mut t2 = view("review PR", 3, None, TaskStatus::Done, Some("done"));
        t2.updated_at = "2026-05-17T11:00:00+08:00".to_string();
        t2.tags = vec!["work".to_string()];
        // 昨日 + 含 tag（应被日期过滤排除）
        let mut y = view("yest", 3, None, TaskStatus::Done, Some("r"));
        y.updated_at = "2026-05-16T15:00:00+08:00".to_string();
        y.tags = vec!["yesterday-only-tag".to_string()];
        let s = format_tags_today_reply(&[t1, t2, y], today);
        // 今日 tag 命中
        assert!(s.contains("· #work ×2"), "work counted twice: {s}");
        assert!(s.contains("· #API ×1"), "API counted once: {s}");
        // 昨日 tag 不出现
        assert!(!s.contains("yesterday-only-tag"), "yesterday tag excluded: {s}");
        assert!(s.contains("2 个 tag"), "count: {s}");
    }

    #[test]
    fn tags_today_includes_untagged_count() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        // 今日含 tag + 今日无 tag 各一条
        let mut tagged = view("a", 3, None, TaskStatus::Pending, None);
        tagged.updated_at = "2026-05-17T09:00:00+08:00".to_string();
        tagged.tags = vec!["tag1".to_string()];
        let mut untagged = view("b", 3, None, TaskStatus::Pending, None);
        untagged.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        // tags 空
        let s = format_tags_today_reply(&[tagged, untagged], today);
        assert!(s.contains("· #tag1 ×1"), "{s}");
        assert!(s.contains("无 #tag 任务（今日）：1 条"), "untagged count: {s}");
    }

    #[test]
    fn tags_today_sorts_by_count_desc() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut t1 = view("a", 3, None, TaskStatus::Pending, None);
        t1.updated_at = "2026-05-17T09:00:00+08:00".to_string();
        t1.tags = vec!["rare".to_string()];
        let mut t2 = view("b", 3, None, TaskStatus::Pending, None);
        t2.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        t2.tags = vec!["common".to_string()];
        let mut t3 = view("c", 3, None, TaskStatus::Pending, None);
        t3.updated_at = "2026-05-17T11:00:00+08:00".to_string();
        t3.tags = vec!["common".to_string()];
        let mut t4 = view("d", 3, None, TaskStatus::Pending, None);
        t4.updated_at = "2026-05-17T12:00:00+08:00".to_string();
        t4.tags = vec!["common".to_string()];
        let s = format_tags_today_reply(&[t1, t2, t3, t4], today);
        let idx_common = s.find("#common").expect("common in output");
        let idx_rare = s.find("#rare").expect("rare in output");
        assert!(idx_common < idx_rare, "count desc: {s}");
        assert!(s.contains("#common ×3"), "{s}");
        assert!(s.contains("#rare ×1"), "{s}");
    }

    // -------- /peek_pinned parse + format --------

    #[test]
    fn peek_pinned_parser_no_arg() {
        assert_eq!(
            parse_tg_command("/peek_pinned"),
            Some(TgCommand::PeekPinned)
        );
        assert_eq!(
            parse_tg_command("/PEEK_PINNED"),
            Some(TgCommand::PeekPinned)
        );
        // 尾部 token 容忍（与其它无参 today/today_done 同协议）
        assert_eq!(
            parse_tg_command("/peek_pinned extra"),
            Some(TgCommand::PeekPinned)
        );
    }

    #[test]
    fn peek_pinned_empty_shows_teaching_hint() {
        let s = format_peek_pinned_reply(&[]);
        assert!(s.contains("暂无 pinned task"), "{s}");
        assert!(s.contains("/pin"), "should teach via /pin: {s}");
        assert!(s.contains("/peek_pinned"), "{s}");
    }

    #[test]
    fn peek_pinned_lists_each_pinned_in_peek_row_format() {
        // 注：format_peek_reply 的 schedule prefix 仅在 raw 起始位置识别
        // （`raw.starts_with('[')` + 首段是 every/once/deadline）。real
        // task 通常起 `[task pri=N]` header，schedule prefix 不会被本
        // formatter 捕获 — 这是既有 /peek 已知行为，本测试不断言 schedule
        // 渲染，仅验证 batch list 的 header + per-row peek format
        // wiring 正确。
        let v1 = crate::task_queue::TaskView {
            title: "整理 Downloads".to_string(),
            body: "".to_string(),
            raw_description: "[task pri=3] [pinned] 清桌面".to_string(),
            priority: 3,
            due: None,
            status: crate::task_queue::TaskStatus::Pending,
            error_message: None,
            tags: vec![],
            result: None,
            created_at: "2026-05-04T13:00:00+08:00".to_string(),
            updated_at: "2026-05-04T13:00:00+08:00".to_string(),
            detail_path: "".to_string(),
            blocked_by: vec![],
            snoozed_until: None,
            pinned: true,
        };
        let v2 = crate::task_queue::TaskView {
            title: "写周报".to_string(),
            raw_description: "[task pri=5] [pinned] [silent] body".to_string(),
            priority: 5,
            pinned: true,
            ..v1.clone()
        };
        let s = format_peek_pinned_reply(&[v1, v2]);
        // header
        assert!(s.contains("📌 2 条 pinned"), "header: {s}");
        // 每条 /peek 格式（emoji + 「title」 + markers + P）
        assert!(s.contains("「整理 Downloads」"), "{s}");
        assert!(s.contains("「写周报」"), "{s}");
        // markers / pri 信息（从 peek 单行渲染来）
        assert!(s.contains("📌"), "pinned marker emoji: {s}");
        assert!(s.contains("🔇"), "silent marker emoji from v2: {s}");
        assert!(s.contains("P3"), "priority 3 from v1: {s}");
        assert!(s.contains("P5"), "priority 5 from v2: {s}");
    }

    // -------- /alarms_thisweek parse + format --------

    #[test]
    fn alarms_thisweek_parser_no_arg() {
        assert_eq!(
            parse_tg_command("/alarms_thisweek"),
            Some(TgCommand::AlarmsThisweek),
        );
    }

    #[test]
    fn alarms_thisweek_empty_shows_week_specific_fallback() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 13)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let ws = chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap(); // Mon
        let s = format_alarms_thisweek_reply(&[], now, ws);
        assert!(s.contains("本周（2026-05-11 起）"), "{s}");
        assert!(s.contains("/alarms"), "alt /alarms: {s}");
        assert!(s.contains("/alarms_today"), "alt /alarms_today: {s}");
    }

    #[test]
    fn alarms_thisweek_filters_inclusive_week_range() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 13)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let ws = chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap();
        // 周内 Mon
        let mon_target = chrono::NaiveDate::from_ymd_opt(2026, 5, 11)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        // 周内 Sun
        let sun_target = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(20, 0, 0)
            .unwrap();
        // 上周
        let last_sun_target = chrono::NaiveDate::from_ymd_opt(2026, 5, 10)
            .unwrap()
            .and_hms_opt(20, 0, 0)
            .unwrap();
        // 下周
        let next_mon_target = chrono::NaiveDate::from_ymd_opt(2026, 5, 18)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        let rows = vec![
            (
                crate::proactive::ReminderTarget::Absolute(mon_target),
                "Mon".to_string(),
                "x1".to_string(),
            ),
            (
                crate::proactive::ReminderTarget::Absolute(sun_target),
                "Sun".to_string(),
                "x2".to_string(),
            ),
            (
                crate::proactive::ReminderTarget::Absolute(last_sun_target),
                "LastSun".to_string(),
                "x3".to_string(),
            ),
            (
                crate::proactive::ReminderTarget::Absolute(next_mon_target),
                "NextMon".to_string(),
                "x4".to_string(),
            ),
            (
                crate::proactive::ReminderTarget::TodayHour(20, 0),
                "TodayHour".to_string(),
                "x5".to_string(),
            ),
        ];
        let s = format_alarms_thisweek_reply(&rows, now, ws);
        assert!(s.contains("本周（2026-05-11 起）3 条 alarms"), "{s}");
        assert!(s.contains("Mon"), "Mon included: {s}");
        assert!(s.contains("Sun"), "Sun included: {s}");
        assert!(s.contains("TodayHour"), "TodayHour included: {s}");
        assert!(!s.contains("LastSun"), "last week excluded: {s}");
        assert!(!s.contains("NextMon"), "next week excluded: {s}");
    }

    // -------- /alarms_today parse + format --------

    #[test]
    fn alarms_today_parser_no_arg() {
        assert_eq!(
            parse_tg_command("/alarms_today"),
            Some(TgCommand::AlarmsToday)
        );
        assert_eq!(
            parse_tg_command("/ALARMS_TODAY"),
            Some(TgCommand::AlarmsToday)
        );
        // 尾部 token 容忍（与 /touched_today / /mute_today 同协议）
        assert_eq!(
            parse_tg_command("/alarms_today extra"),
            Some(TgCommand::AlarmsToday)
        );
    }

    #[test]
    fn alarms_today_empty_shows_friendly_fallback() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let s = format_alarms_today_reply(&[], now);
        assert!(s.contains("今日（2026-05-17）暂无 alarm"), "{s}");
        // 兜底教学指 /alarms 全量入口 — 不指向 own 命令（loop prevention）
        assert!(s.contains("/alarms"), "{s}");
    }

    #[test]
    fn alarms_today_filters_today_only() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        // 今日 Absolute target → 命中
        let today_target = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(18, 30, 0)
            .unwrap();
        // 明日 Absolute target → 不命中
        let tomorrow_target = chrono::NaiveDate::from_ymd_opt(2026, 5, 18)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        // TodayHour → 永远命中（按定义）
        let rows = vec![
            (
                crate::proactive::ReminderTarget::Absolute(today_target),
                "今日会议".to_string(),
                "t1".to_string(),
            ),
            (
                crate::proactive::ReminderTarget::Absolute(tomorrow_target),
                "明日 deploy".to_string(),
                "t2".to_string(),
            ),
            (
                crate::proactive::ReminderTarget::TodayHour(20, 0),
                "今晚 reminder".to_string(),
                "t3".to_string(),
            ),
        ];
        let s = format_alarms_today_reply(&rows, now);
        assert!(s.contains("今日（2026-05-17）2 条 alarms"), "count: {s}");
        assert!(s.contains("今日会议"), "today absolute included: {s}");
        assert!(s.contains("今晚 reminder"), "TodayHour included: {s}");
        assert!(!s.contains("明日 deploy"), "tomorrow excluded: {s}");
    }

    #[test]
    fn alarms_today_shows_hh_mm_only_in_header_and_lines() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let target = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(18, 45, 0)
            .unwrap();
        let rows = vec![(
            crate::proactive::ReminderTarget::Absolute(target),
            "准备会议".to_string(),
            "x".to_string(),
        )];
        let s = format_alarms_today_reply(&rows, now);
        // header 含 date；行 HH:MM only
        assert!(s.contains("今日（2026-05-17）"), "header date: {s}");
        assert!(s.contains("· 18:45 "), "line HH:MM: {s}");
        // 行内不重复 MM-DD（date 已在 header 不冗余）
        assert!(!s.contains("· 05-17 18:45"), "{s}");
    }

    #[test]
    fn alarms_today_shows_overdue_and_remaining_per_line() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(18, 30, 0)
            .unwrap();
        let future_target = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(20, 0, 0)
            .unwrap();
        let past_target = chrono::NaiveDate::from_ymd_opt(2026, 5, 17)
            .unwrap()
            .and_hms_opt(16, 0, 0)
            .unwrap();
        let rows = vec![
            (
                crate::proactive::ReminderTarget::Absolute(future_target),
                "晚 reminder".to_string(),
                "f".to_string(),
            ),
            (
                crate::proactive::ReminderTarget::Absolute(past_target),
                "下午 reminder".to_string(),
                "p".to_string(),
            ),
        ];
        let s = format_alarms_today_reply(&rows, now);
        assert!(s.contains("剩 1 小时"), "future shows 剩: {s}");
        assert!(s.contains("已逾期 2 小时"), "past shows 已逾期: {s}");
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

    // -------- /edit_title parse + format --------

    #[test]
    fn edit_title_parser_splits_on_double_colon() {
        assert_eq!(
            parse_tg_command("/edit_title 整理 Downloads :: 清理桌面"),
            Some(TgCommand::EditTitle {
                title: "整理 Downloads".to_string(),
                new_title: "清理桌面".to_string(),
            })
        );
        // 前后空白 trim
        assert_eq!(
            parse_tg_command("/edit_title   a   ::   b  "),
            Some(TgCommand::EditTitle {
                title: "a".to_string(),
                new_title: "b".to_string(),
            })
        );
    }

    #[test]
    fn edit_title_parser_missing_separator_yields_empty_new() {
        // 无 `::` → new_title 空，handler 走 missing-arg
        assert_eq!(
            parse_tg_command("/edit_title 整理 Downloads"),
            Some(TgCommand::EditTitle {
                title: "整理 Downloads".to_string(),
                new_title: String::new(),
            })
        );
    }

    #[test]
    fn edit_title_parser_empty_title_ok() {
        assert_eq!(
            parse_tg_command("/edit_title"),
            Some(TgCommand::EditTitle {
                title: String::new(),
                new_title: String::new(),
            })
        );
    }

    #[test]
    fn edit_title_parser_handles_double_colon_in_new_title() {
        // split_once 只切首个 `::` — 新 title 含 `::` 时一并入 new（用例
        // 不常见，但行为可预期）
        assert_eq!(
            parse_tg_command("/edit_title old :: a :: b"),
            Some(TgCommand::EditTitle {
                title: "old".to_string(),
                new_title: "a :: b".to_string(),
            })
        );
    }

    #[test]
    fn edit_title_reply_shows_old_and_new() {
        let s = format_edit_title_reply("整理 Downloads", "清理桌面");
        assert!(s.contains("✏️"), "{s}");
        assert!(s.contains("「整理 Downloads」"), "{s}");
        assert!(s.contains("「清理桌面」"), "{s}");
    }

    #[test]
    fn edit_title_reply_trims_both_sides() {
        let s = format_edit_title_reply("  a  ", "  b  ");
        assert!(s.contains("「a」"), "{s}");
        assert!(s.contains("「b」"), "{s}");
        assert!(!s.contains("  a  "), "{s}");
    }

    // -------- /search_today parse + format --------

    #[test]
    fn search_today_parser_takes_all_args_as_keyword() {
        assert_eq!(
            parse_tg_command("/search_today API"),
            Some(TgCommand::SearchToday {
                keyword: "API".to_string(),
            })
        );
        assert_eq!(
            parse_tg_command("/search_today 写 周报"),
            Some(TgCommand::SearchToday {
                keyword: "写 周报".to_string(),
            })
        );
    }

    #[test]
    fn search_today_parser_empty_keyword_ok() {
        // handler / formatter 走 usage hint
        assert_eq!(
            parse_tg_command("/search_today"),
            Some(TgCommand::SearchToday {
                keyword: String::new(),
            })
        );
    }

    #[test]
    fn search_today_empty_keyword_shows_usage_hint() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let s = format_search_today_reply(&[], today, "");
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/search_today"), "{s}");
        assert!(s.contains("/find"), "should mention alt /find: {s}");
        assert!(s.contains("/touched_today"), "should mention alt /touched_today: {s}");
    }

    #[test]
    fn search_today_no_hits_shows_friendly_fallback() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut t = view("nothing matches", 3, None, TaskStatus::Pending, None);
        t.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let s = format_search_today_reply(&[t], today, "missing-kw");
        assert!(s.contains("今日（2026-05-17）无任务命中"), "{s}");
        assert!(s.contains("「missing-kw」"), "{s}");
        assert!(s.contains("/find"), "fallback alt entry: {s}");
        assert!(s.contains("/touched_today"), "{s}");
    }

    #[test]
    fn search_today_filters_to_today_and_keyword_intersection() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        // 今日 + 命中
        let mut t_hit = view("API design", 3, None, TaskStatus::Pending, None);
        t_hit.updated_at = "2026-05-17T09:00:00+08:00".to_string();
        // 今日 + 不命中
        let mut t_miss = view("doc cleanup", 3, None, TaskStatus::Pending, None);
        t_miss.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        // 昨日 + 命中（应被日期 filter 排除）
        let mut y_hit = view("API yesterday", 3, None, TaskStatus::Done, Some("r"));
        y_hit.updated_at = "2026-05-16T20:00:00+08:00".to_string();
        let s = format_search_today_reply(&[t_hit, t_miss, y_hit], today, "API");
        assert!(s.contains("命中「API」1 条"), "exactly 1 hit: {s}");
        assert!(s.contains("API design"), "today hit included: {s}");
        assert!(!s.contains("doc cleanup"), "today non-hit excluded: {s}");
        assert!(!s.contains("API yesterday"), "yesterday hit excluded: {s}");
    }

    #[test]
    fn search_today_sorts_pending_before_done() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut d = view("foo done", 3, None, TaskStatus::Done, Some("r"));
        d.updated_at = "2026-05-17T08:00:00+08:00".to_string();
        let mut p = view("foo pending", 3, None, TaskStatus::Pending, None);
        p.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let s = format_search_today_reply(&[d, p], today, "foo");
        let idx_pending = s.find("foo pending").expect("pending in output");
        let idx_done = s.find("foo done").expect("done in output");
        assert!(idx_pending < idx_done, "pending浮顶: {s}");
    }

    #[test]
    fn search_today_is_case_insensitive() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut t = view("ReviewPR", 3, None, TaskStatus::Pending, None);
        t.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let s = format_search_today_reply(&[t], today, "reviewpr");
        assert!(s.contains("ReviewPR"), "case-insensitive match: {s}");
    }

    // -------- /touched_thisweek parse + format --------

    #[test]
    fn touched_thisweek_parser_no_arg() {
        assert_eq!(
            parse_tg_command("/touched_thisweek"),
            Some(TgCommand::TouchedThisweek)
        );
        assert_eq!(
            parse_tg_command("/TOUCHED_THISWEEK"),
            Some(TgCommand::TouchedThisweek)
        );
    }

    #[test]
    fn touched_thisweek_empty_shows_friendly_fallback() {
        let week_start = chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap(); // Monday
        let s = format_touched_thisweek_reply(&[], week_start);
        assert!(s.contains("本周（2026-05-11 起）暂无"), "{s}");
        // 三 alt 入口教学
        assert!(s.contains("/touched_today"), "{s}");
        assert!(s.contains("/yesterday"), "{s}");
        assert!(s.contains("/tasks"), "{s}");
    }

    #[test]
    fn touched_thisweek_filters_week_range_inclusive() {
        let week_start = chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap(); // Monday
        // 周内 (Mon ~ Sun) 各种 status
        let mut mon_pending = view("p", 3, None, TaskStatus::Pending, None);
        mon_pending.updated_at = "2026-05-11T09:00:00+08:00".to_string();
        let mut wed_done = view("d", 3, None, TaskStatus::Done, Some("ok"));
        wed_done.updated_at = "2026-05-13T15:00:00+08:00".to_string();
        let mut sun_pending = view("s", 3, None, TaskStatus::Pending, None);
        sun_pending.updated_at = "2026-05-17T22:00:00+08:00".to_string();
        // 周外 (上周日)
        let mut last_sun = view("last", 3, None, TaskStatus::Done, Some("r"));
        last_sun.updated_at = "2026-05-10T20:00:00+08:00".to_string();
        let s = format_touched_thisweek_reply(
            &[mon_pending, wed_done, sun_pending, last_sun],
            week_start,
        );
        assert!(s.contains("动过 3 条"), "this-week count: {s}");
        assert!(s.contains("\n· "), "{s}");
        // 周内 included
        assert!(s.contains(" p"), "Mon pending: {s}");
        assert!(s.contains(" d"), "Wed done: {s}");
        assert!(s.contains(" s"), "Sun pending: {s}");
        // 周外 excluded
        assert!(!s.contains("last"), "previous week excluded: {s}");
    }

    #[test]
    fn touched_thisweek_uses_mm_dd_hh_mm_per_line() {
        let week_start = chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap();
        let mut wed = view("写文档", 3, None, TaskStatus::Done, Some("提交 PR"));
        wed.updated_at = "2026-05-13T14:30:00+08:00".to_string();
        let s = format_touched_thisweek_reply(&[wed], week_start);
        // 跨日 scope — 行内必须含 MM-DD（不能省 date）
        assert!(s.contains("05-13 14:30"), "MM-DD HH:MM in line: {s}");
        // result preview
        assert!(s.contains("— 提交 PR"), "{s}");
    }

    #[test]
    fn touched_thisweek_snooze_emoji_for_pending_with_snooze_marker() {
        let week_start = chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap();
        let mut snoozed = view("延", 3, None, TaskStatus::Pending, None);
        snoozed.updated_at = "2026-05-13T10:00:00+08:00".to_string();
        snoozed.raw_description = "[task pri=3] [snooze: 2026-05-15 09:00] 延".to_string();
        let s = format_touched_thisweek_reply(&[snoozed], week_start);
        assert!(s.contains("💤"), "snoozed pending → 💤: {s}");
        assert!(!s.contains("⏳"), "non-snoozed emoji suppressed: {s}");
    }


    // -------- /cat_top parse + format --------

    #[test]
    fn cat_top_parser_default_n() {
        assert_eq!(
            parse_tg_command("/cat_top"),
            Some(TgCommand::CatTop { n: 5 }),
        );
    }

    #[test]
    fn cat_top_parser_clamp() {
        assert_eq!(
            parse_tg_command("/cat_top 12"),
            Some(TgCommand::CatTop { n: 12 }),
        );
        assert_eq!(
            parse_tg_command("/cat_top 999"),
            Some(TgCommand::CatTop { n: 20 }),
        );
        assert_eq!(
            parse_tg_command("/cat_top 0"),
            Some(TgCommand::CatTop { n: 1 }),
        );
    }

    #[test]
    fn format_cat_top_empty_shows_fallback() {
        let s = format_cat_top_reply(&[], 0);
        assert!(s.contains("无 cat"), "{s}");
    }

    #[test]
    fn format_cat_top_renders_rows() {
        let rows = vec![
            ("butler_tasks".to_string(), 156),
            ("decisions".to_string(), 89),
            ("general".to_string(), 42),
        ];
        let s = format_cat_top_reply(&rows, 8);
        assert!(s.contains("cat top 3"), "{s}");
        assert!(s.contains("共 8 cat in index"), "{s}");
        assert!(s.contains("· butler_tasks · 156 条"), "{s}");
        assert!(s.contains("· decisions · 89 条"), "{s}");
        assert!(s.contains("· general · 42 条"), "{s}");
        // 顺序：butler_tasks 在 general 前（count desc）
        let b_idx = s.find("butler_tasks").unwrap();
        let g_idx = s.find("general").unwrap();
        assert!(b_idx < g_idx, "butler_tasks should appear before general: {s}");
    }

    // -------- /audit_summary parse + format --------

    #[test]
    fn audit_summary_parser_no_args() {
        assert_eq!(
            parse_tg_command("/audit_summary"),
            Some(TgCommand::AuditSummary),
        );
        assert_eq!(
            parse_tg_command("/audit_summary extra"),
            Some(TgCommand::AuditSummary),
        );
    }

    #[test]
    fn format_audit_summary_renders_all_signals_with_deep_dive_links() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 18).unwrap();
        let s = format_audit_summary_reply(today, 5, 3, 12, 7, 2);
        // header 含日期
        assert!(s.contains("audit summary（2026-05-18）"), "{s}");
        // audit 信号每行
        assert!(s.contains("📌 pin streak: 5 天连续（当前 3 钉）"), "{s}");
        assert!(s.contains("💤 idle 7d+: 12 条 stale pending"), "{s}");
        assert!(s.contains("✅ 今日 touched: 7 条"), "{s}");
        assert!(s.contains("🏷 近 7d rename: 2 次"), "{s}");
        // deep dive 入口（剩余两条）
        assert!(s.contains("/idle_7d"), "{s}");
        assert!(s.contains("/touched_today"), "{s}");
    }

    #[test]
    fn format_audit_summary_zero_values_still_render_all_rows() {
        // 全 0 signals — 仍应每行渲染（避免「这条 audit 是不是没数据」歧义）
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 18).unwrap();
        let s = format_audit_summary_reply(today, 0, 0, 0, 0, 0);
        assert!(s.contains("pin streak: 0 天连续"), "{s}");
        assert!(s.contains("idle 7d+: 0 条"), "{s}");
        assert!(s.contains("今日 touched: 0 条"), "{s}");
        assert!(s.contains("近 7d rename: 0 次"), "{s}");
    }

    // -------- /help_table parse + format --------

    #[test]
    fn help_table_parser_no_args() {
        assert_eq!(
            parse_tg_command("/help_table"),
            Some(TgCommand::HelpTable { family: None }),
        );
    }

    #[test]
    fn help_table_parser_with_family() {
        assert_eq!(
            parse_tg_command("/help_table pin"),
            Some(TgCommand::HelpTable {
                family: Some("pin".to_string()),
            }),
        );
        // 含全角 / Chinese family name
        assert_eq!(
            parse_tg_command("/help_table 关注度"),
            Some(TgCommand::HelpTable {
                family: Some("关注度".to_string()),
            }),
        );
    }

    #[test]
    fn format_help_table_family_pin_shows_detail() {
        let s = format_help_table_family("pin");
        // header + hint
        assert!(s.contains("📌 pin 关注度 家族详细清单"), "{s}");
        assert!(s.contains("钉住关键 task"), "hint: {s}");
        // 每条命令 + 一行描述
        assert!(s.contains("/pin <title>"), "{s}");
        assert!(s.contains("/recent_pins"), "{s}");
        // 一行描述存在（spot check）
        assert!(s.contains("钉住任务"), "{s}");
    }

    #[test]
    fn format_help_table_family_alias_case_insensitive() {
        // 中文 alias
        let s_zh = format_help_table_family("关注度");
        assert!(s_zh.contains("📌 pin"), "{s_zh}");
        // 大写
        let s_upper = format_help_table_family("PIN");
        assert!(s_upper.contains("📌 pin"), "{s_upper}");
    }

    #[test]
    fn format_help_table_family_unknown_shows_available_list() {
        let s = format_help_table_family("xyz_unknown");
        assert!(s.contains("未知 family"), "{s}");
        assert!(s.contains("xyz_unknown"), "{s}");
        // 可用 family 列表
        for f in ["pin", "cat", "rename", "idle", "streak"] {
            assert!(s.contains(f), "missing {f}: {s}");
        }
        assert!(s.contains("/help_table"), "fallback hint: {s}");
    }

    #[test]
    fn format_help_table_full_no_family_shows_overview() {
        let s = format_help_table_reply_full(None);
        // 应走全表分支 — 必含分组速查表 header
        assert!(s.contains("命令分组速查表"), "{s}");
    }

    #[test]
    fn format_help_table_shows_all_audit_families() {
        let s = format_help_table_reply();
        // 必含 header
        assert!(s.contains("命令分组速查表"), "{s}");
        // 必含所有主要 family group
        for family in [
            "📌 pin 关注度",
            "📚 cat（memory category）",
            "🔁 rename 重命名",
            "💤 idle / stale",
            "🔥 streak 连续",
            "🔎 find / search",
            "🏷 tag",
            "🗣 pet speech",
            "⏰ alarm",
            "📋 task 增删改",
            "📊 status",
            "⚠️ batch / 危险",
            "⚙️ system",
        ] {
            assert!(s.contains(family), "missing family {family}: {s}");
        }
        // 必含若干代表性命令（spot check）
        for cmd in [
            "/pin",
            "/cat_top",
            "/idle_7d",
            "/streak",
            "/find",
            "/last_speech",
            "/alarms",
            "/task",
            "/tasks",
            "/cancel_all_error",
            "/help_table",
        ] {
            assert!(s.contains(cmd), "missing cmd {cmd}: {s}");
        }
        // 末尾相关教学
        assert!(s.contains("/help"), "{s}");
        assert!(s.contains("/help search"), "{s}");
    }

    // -------- /recent_pins parse + format --------

    #[test]
    fn recent_pins_parser_default_n() {
        assert_eq!(
            parse_tg_command("/recent_pins"),
            Some(TgCommand::RecentPins { n: 5 }),
        );
    }

    #[test]
    fn recent_pins_parser_explicit_n_clamps() {
        assert_eq!(
            parse_tg_command("/recent_pins 12"),
            Some(TgCommand::RecentPins { n: 12 }),
        );
        // upper clamp 20
        assert_eq!(
            parse_tg_command("/recent_pins 999"),
            Some(TgCommand::RecentPins { n: 20 }),
        );
        // lower clamp 1
        assert_eq!(
            parse_tg_command("/recent_pins 0"),
            Some(TgCommand::RecentPins { n: 1 }),
        );
    }

    #[test]
    fn format_recent_pins_empty_shows_fallback() {
        let s = format_recent_pins_reply(&[], 0);
        assert!(s.contains("无 [pinned] sighting"), "{s}");
        assert!(s.contains("/pin"), "{s}");
        assert!(s.contains("/pinned"), "{s}");
    }

    #[test]
    fn format_recent_pins_renders_rows() {
        let rows = vec![
            ("05-17 14:30".to_string(), "整理 Downloads".to_string()),
            ("05-16 09:15".to_string(), "写周报".to_string()),
        ];
        let s = format_recent_pins_reply(&rows, 2);
        assert!(s.contains("近 2 条 pin 决策（共 2 条 retention 内）"), "{s}");
        assert!(s.contains("· 05-17 14:30 · 「整理 Downloads」"), "{s}");
        assert!(s.contains("· 05-16 09:15 · 「写周报」"), "{s}");
    }

    #[test]
    fn format_recent_pins_header_shows_total_when_capped() {
        let rows = vec![
            ("05-17 14:30".to_string(), "X".to_string()),
        ];
        let s = format_recent_pins_reply(&rows, 15);
        assert!(s.contains("近 1 条 pin 决策（共 15 条 retention 内）"), "{s}");
    }

    // -------- compute_pin_streak --------

    #[test]
    fn compute_pin_streak_empty_returns_zero() {
        let dates = std::collections::HashSet::new();
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 18).unwrap();
        let (streak, earliest, max_s) = compute_pin_streak(&dates, false, today);
        assert_eq!(streak, 0);
        assert!(earliest.is_none());
        assert_eq!(max_s, 0);
    }

    #[test]
    fn compute_pin_streak_today_fallback_when_current_pinned() {
        // 当前有 pinned 但 history 无 sighting → 今日仍计 1 天
        let dates = std::collections::HashSet::new();
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 18).unwrap();
        let (streak, earliest, max_s) = compute_pin_streak(&dates, true, today);
        assert_eq!(streak, 1);
        assert_eq!(earliest.as_deref(), Some("2026-05-18"));
        assert_eq!(max_s, 1);
    }

    #[test]
    fn compute_pin_streak_consecutive_days() {
        // 连续 3 天 sighting + 今日 fallback
        let mut dates = std::collections::HashSet::new();
        dates.insert("2026-05-16".to_string());
        dates.insert("2026-05-17".to_string());
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 18).unwrap();
        let (streak, earliest, max_s) = compute_pin_streak(&dates, true, today);
        // today (fallback) + 17 + 16 = 3 天
        assert_eq!(streak, 3);
        assert_eq!(earliest.as_deref(), Some("2026-05-16"));
        assert_eq!(max_s, 3);
    }

    #[test]
    fn compute_pin_streak_break_on_gap() {
        // 5-16 / 5-17 / 5-18 yesterday (sighting), today 无 sighting + 无
        // current pinned → 今日不算，streak 0
        let mut dates = std::collections::HashSet::new();
        dates.insert("2026-05-16".to_string());
        dates.insert("2026-05-17".to_string());
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 18).unwrap();
        let (streak, earliest, max_s) = compute_pin_streak(&dates, false, today);
        assert_eq!(streak, 0, "today break → 0 current streak");
        assert!(earliest.is_none());
        // 但 max_streak 还是 5-16,5-17 = 2 day chain
        assert_eq!(max_s, 2);
    }

    #[test]
    fn compute_pin_streak_max_finds_longest_historical_chain() {
        // 5-10..5-13 (4 day chain) + 5-15 (1 day) + today fallback
        let mut dates = std::collections::HashSet::new();
        for d in ["2026-05-10", "2026-05-11", "2026-05-12", "2026-05-13"] {
            dates.insert(d.to_string());
        }
        dates.insert("2026-05-15".to_string());
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 18).unwrap();
        let (streak, _earliest, max_s) = compute_pin_streak(&dates, true, today);
        // current streak 仅今日 fallback（17 没 sighting → break）= 1
        assert_eq!(streak, 1);
        // max 历史最长链 5-10..5-13 = 4 天
        assert_eq!(max_s, 4);
    }

    // -------- /idle_7d parse + format --------

    #[test]
    fn idle_7d_parser_no_args() {
        assert_eq!(
            parse_tg_command("/idle_7d"),
            Some(TgCommand::Idle7d)
        );
        assert_eq!(
            parse_tg_command("/idle_7d extra"),
            Some(TgCommand::Idle7d)
        );
    }

    #[test]
    fn idle_7d_empty_shows_healthy_fallback() {
        let s = format_idle_7d_reply(&[]);
        assert!(s.contains("无 7d+ idle pending"), "{s}");
        assert!(s.contains("健康状态"), "{s}");
        assert!(s.contains("/touched_thisweek"), "{s}");
    }

    #[test]
    fn idle_7d_renders_rows_with_label() {
        let rows = vec![
            ("写周报".to_string(), 14, "2026-05-04".to_string()),
            ("整理 Downloads".to_string(), 9, "2026-05-09".to_string()),
        ];
        let s = format_idle_7d_reply(&rows);
        assert!(s.contains("stale backlog 2 条"), "{s}");
        assert!(
            s.contains("· 「写周报」 · idle 14 天（last update 2026-05-04）"),
            "{s}",
        );
        assert!(
            s.contains("· 「整理 Downloads」 · idle 9 天（last update 2026-05-09）"),
            "{s}",
        );
    }

    #[test]
    fn idle_7d_caps_at_12_with_tail_hint() {
        let mut rows = Vec::new();
        for i in 0..15 {
            rows.push((
                format!("task_{:02}", i),
                30 - i as i64, // strictly desc for stable order
                format!("2026-04-{:02}", (i % 30) + 1),
            ));
        }
        let s = format_idle_7d_reply(&rows);
        assert!(s.contains("stale backlog 15 条"), "{s}");
        // 第 12 条 (task_11) 应可见，第 13 条 (task_12) 应被截
        assert!(s.contains("· 「task_11」"), "12th visible: {s}");
        assert!(!s.contains("· 「task_12」"), "13th capped: {s}");
        assert!(s.contains("还有 3 条"), "tail hint: {s}");
    }

    // -------- /find_in_detail_yesterday parse + format --------

    #[test]
    fn find_in_detail_yesterday_parser_takes_all_args_as_keyword() {
        assert_eq!(
            parse_tg_command("/find_in_detail_yesterday rebase"),
            Some(TgCommand::FindInDetailYesterday {
                keyword: "rebase".to_string(),
            })
        );
        assert_eq!(
            parse_tg_command("/find_in_detail_yesterday API design"),
            Some(TgCommand::FindInDetailYesterday {
                keyword: "API design".to_string(),
            })
        );
    }

    #[test]
    fn find_in_detail_yesterday_empty_keyword_shows_usage_hint() {
        let y = chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let s = format_find_in_detail_yesterday_reply(&[], "", y);
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/find_in_detail_today"), "{s}");
        assert!(s.contains("/touched_yesterday"), "{s}");
    }

    #[test]
    fn find_in_detail_yesterday_no_hits_shows_yesterday_fallback() {
        let y = chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let s = format_find_in_detail_yesterday_reply(&[], "missing-kw", y);
        assert!(s.contains("昨日（2026-05-16）"), "{s}");
        // 兜底教学不指 today（不同 scope）/ 不指 self
        assert!(!s.contains("/find_in_detail_today"), "no today loop: {s}");
        assert!(s.contains("/find_in_detail"), "broader scope alt: {s}");
        assert!(s.contains("/touched_yesterday"), "{s}");
    }

    #[test]
    fn find_in_detail_yesterday_renders_hits_with_emoji_and_snippet() {
        let y = chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let hits = vec![FindInDetailHit {
            title: "API doc",
            status: crate::task_queue::TaskStatus::Done,
            snippet: "提到 rebase 的策略".to_string(),
        }];
        let s = format_find_in_detail_yesterday_reply(&hits, "rebase", y);
        assert!(s.contains("昨日（2026-05-16）命中「rebase」1 条"), "{s}");
        assert!(s.contains("✅ API doc"), "done emoji: {s}");
        assert!(s.contains("…提到 rebase 的策略…"), "snippet: {s}");
    }

    // -------- /find_in_detail_today parse + format --------

    #[test]
    fn find_in_detail_today_parser_takes_all_args_as_keyword() {
        assert_eq!(
            parse_tg_command("/find_in_detail_today rebase"),
            Some(TgCommand::FindInDetailToday {
                keyword: "rebase".to_string(),
            })
        );
        assert_eq!(
            parse_tg_command("/find_in_detail_today API design"),
            Some(TgCommand::FindInDetailToday {
                keyword: "API design".to_string(),
            })
        );
    }

    #[test]
    fn find_in_detail_today_empty_keyword_shows_usage_hint() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let s = format_find_in_detail_today_reply(&[], "", today);
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/find_in_detail"), "alt entry mentioned: {s}");
        assert!(s.contains("/search_today"), "{s}");
        assert!(s.contains("/touched_today"), "{s}");
    }

    #[test]
    fn find_in_detail_today_no_hits_shows_today_specific_fallback() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let s = format_find_in_detail_today_reply(&[], "missing-kw", today);
        assert!(s.contains("今日（2026-05-17）"), "{s}");
        assert!(s.contains("「missing-kw」"), "{s}");
        // 兜底教学指 /find_in_detail（更广）+ /touched_today（同 scope 全谱）
        assert!(s.contains("/find_in_detail"), "{s}");
        assert!(s.contains("/touched_today"), "{s}");
    }

    #[test]
    fn find_in_detail_today_renders_hits_with_emoji_and_snippet() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let hits = vec![
            FindInDetailHit {
                title: "API doc",
                status: crate::task_queue::TaskStatus::Pending,
                snippet: "提到 rebase 的策略".to_string(),
            },
            FindInDetailHit {
                title: "deploy notes",
                status: crate::task_queue::TaskStatus::Done,
                snippet: "rebase 之后 deploy 顺利".to_string(),
            },
        ];
        let s = format_find_in_detail_today_reply(&hits, "rebase", today);
        assert!(s.contains("今日（2026-05-17）命中「rebase」2 条"), "{s}");
        assert!(s.contains("🟢 API doc"), "pending emoji: {s}");
        assert!(s.contains("✅ deploy notes"), "done emoji: {s}");
        assert!(s.contains("…提到 rebase 的策略…"), "snippet preserved: {s}");
    }

    #[test]
    fn find_in_detail_today_caps_at_8_hits() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let titles: Vec<String> = (1..=12).map(|i| format!("task{}", i)).collect();
        let hits: Vec<FindInDetailHit> = titles
            .iter()
            .map(|t| FindInDetailHit {
                title: t.as_str(),
                status: crate::task_queue::TaskStatus::Pending,
                snippet: "x".to_string(),
            })
            .collect();
        let s = format_find_in_detail_today_reply(&hits, "x", today);
        assert!(s.contains("命中「x」12 条"), "total count: {s}");
        // 8 cap：仅前 8 个 task title 出现 + remainder hint
        assert!(s.contains("task1\n"), "{s}");
        assert!(s.contains("task8\n"), "{s}");
        assert!(!s.contains("task9"), "9th excluded by cap: {s}");
        assert!(s.contains("还有 4 条命中"), "remainder hint: {s}");
    }

    // -------- /search_thisweek parse + format --------

    #[test]
    fn search_thisweek_parser_takes_all_args_as_keyword() {
        assert_eq!(
            parse_tg_command("/search_thisweek API"),
            Some(TgCommand::SearchThisweek {
                keyword: "API".to_string(),
            })
        );
        assert_eq!(
            parse_tg_command("/search_thisweek 写 周报"),
            Some(TgCommand::SearchThisweek {
                keyword: "写 周报".to_string(),
            })
        );
    }

    #[test]
    fn search_thisweek_empty_keyword_shows_usage_hint() {
        let ws = chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap();
        let s = format_search_thisweek_reply(&[], ws, "");
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/search_today"), "alt /search_today: {s}");
        assert!(s.contains("/touched_thisweek"), "alt /touched_thisweek: {s}");
    }

    #[test]
    fn search_thisweek_no_hits_shows_week_specific_fallback() {
        let ws = chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap();
        let mut t = view("api unrelated thisweek", 3, None, TaskStatus::Done, Some("r"));
        t.updated_at = "2026-05-13T10:00:00+08:00".to_string();
        let s = format_search_thisweek_reply(&[t], ws, "missing-kw");
        assert!(s.contains("本周（2026-05-11 起）无任务命中"), "{s}");
        assert!(s.contains("/find"), "{s}");
        assert!(s.contains("/touched_thisweek"), "{s}");
        // 不指向 /search_today（不同 scope 让 owner 困惑）
        assert!(!s.contains("/search_today"), "loop prevention: {s}");
    }

    #[test]
    fn search_thisweek_filters_week_and_keyword() {
        let ws = chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap();
        // 周内 + 命中
        let mut wed_hit = view("API design", 3, None, TaskStatus::Pending, None);
        wed_hit.updated_at = "2026-05-13T09:00:00+08:00".to_string();
        // 周内 + 不命中
        let mut wed_miss = view("doc cleanup", 3, None, TaskStatus::Pending, None);
        wed_miss.updated_at = "2026-05-13T10:00:00+08:00".to_string();
        // 上周 + 命中（应被日期 filter 排除）
        let mut last_hit = view("API last week", 3, None, TaskStatus::Done, Some("r"));
        last_hit.updated_at = "2026-05-10T20:00:00+08:00".to_string();
        let s = format_search_thisweek_reply(&[wed_hit, wed_miss, last_hit], ws, "API");
        assert!(s.contains("命中「API」1 条"), "{s}");
        assert!(s.contains("API design"), "this-week hit included: {s}");
        assert!(!s.contains("doc cleanup"), "this-week non-hit excluded: {s}");
        assert!(!s.contains("API last week"), "last-week hit excluded: {s}");
    }

    // -------- /search_yesterday parse + format --------

    #[test]
    fn search_yesterday_parser_takes_all_args_as_keyword() {
        assert_eq!(
            parse_tg_command("/search_yesterday API"),
            Some(TgCommand::SearchYesterday {
                keyword: "API".to_string(),
            })
        );
        assert_eq!(
            parse_tg_command("/search_yesterday 写 周报"),
            Some(TgCommand::SearchYesterday {
                keyword: "写 周报".to_string(),
            })
        );
    }

    #[test]
    fn search_yesterday_empty_keyword_shows_usage_hint() {
        let y = chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let s = format_search_yesterday_reply(&[], y, "");
        assert!(s.contains("用法"), "{s}");
        assert!(s.contains("/search_today"), "should mention alt /search_today: {s}");
        assert!(s.contains("/touched_yesterday"), "{s}");
    }

    #[test]
    fn search_yesterday_no_hits_shows_yesterday_specific_fallback() {
        let y = chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut t = view("api unrelated yest", 3, None, TaskStatus::Done, Some("r"));
        t.updated_at = "2026-05-16T10:00:00+08:00".to_string();
        let s = format_search_yesterday_reply(&[t], y, "missing-kw");
        assert!(s.contains("昨日（2026-05-16）无任务命中"), "{s}");
        // 兜底教学不指 /search_yesterday（循环）— 指 /find / /touched_yesterday
        assert!(s.contains("/find"), "{s}");
        assert!(s.contains("/touched_yesterday"), "{s}");
        assert!(!s.contains("/touched_today"), "loop prevention: {s}");
    }

    #[test]
    fn search_yesterday_filters_yesterday_and_keyword() {
        let y = chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut y_hit = view("API yesterday", 3, None, TaskStatus::Pending, None);
        y_hit.updated_at = "2026-05-16T09:00:00+08:00".to_string();
        let mut y_miss = view("doc cleanup", 3, None, TaskStatus::Pending, None);
        y_miss.updated_at = "2026-05-16T10:00:00+08:00".to_string();
        let mut t_hit = view("API today", 3, None, TaskStatus::Done, Some("r"));
        t_hit.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let s = format_search_yesterday_reply(&[y_hit, y_miss, t_hit], y, "API");
        assert!(s.contains("命中「API」1 条"), "{s}");
        assert!(s.contains("API yesterday"), "yesterday hit included: {s}");
        assert!(!s.contains("doc cleanup"), "yesterday non-hit excluded: {s}");
        assert!(!s.contains("API today"), "today hit excluded: {s}");
    }

    // -------- /digest_thisweek parse + format --------

    #[test]
    fn digest_thisweek_parser_clamps_and_defaults() {
        assert_eq!(
            parse_tg_command("/digest_thisweek"),
            Some(TgCommand::DigestThisweek { n: 5 }),
        );
        assert_eq!(
            parse_tg_command("/digest_thisweek 10"),
            Some(TgCommand::DigestThisweek { n: 10 }),
        );
        assert_eq!(
            parse_tg_command("/digest_thisweek 21"),
            Some(TgCommand::DigestThisweek { n: 20 }),
        );
        assert_eq!(
            parse_tg_command("/digest_thisweek abc"),
            Some(TgCommand::DigestThisweek { n: 5 }),
        );
    }

    #[test]
    fn digest_thisweek_empty_shows_week_specific_fallback() {
        let ws = chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap(); // Mon
        let s = format_digest_thisweek_reply(&[], ws, 5);
        assert!(s.contains("本周（2026-05-11 起）暂无"), "{s}");
        // 三 alt 入口教学
        assert!(s.contains("/digest"), "{s}");
        assert!(s.contains("/touched_thisweek"), "{s}");
        assert!(s.contains("/yesterday"), "{s}");
    }

    #[test]
    fn digest_thisweek_filters_done_in_week() {
        let ws = chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap();
        // 周内 done — 命中
        let mut wed_done = view("写文档", 3, None, TaskStatus::Done, Some("提交 PR"));
        wed_done.updated_at = "2026-05-13T15:00:00+08:00".to_string();
        // 周内 pending — done filter 排除
        let mut wed_pending = view("review", 3, None, TaskStatus::Pending, None);
        wed_pending.updated_at = "2026-05-14T10:00:00+08:00".to_string();
        // 上周 done — 日期 filter 排除
        let mut last_done = view("old", 3, None, TaskStatus::Done, Some("r"));
        last_done.updated_at = "2026-05-10T20:00:00+08:00".to_string();
        let s = format_digest_thisweek_reply(&[wed_done, wed_pending, last_done], ws, 5);
        assert!(s.contains("完成 1 条"), "count: {s}");
        assert!(s.contains("写文档"), "this-week done included: {s}");
        assert!(!s.contains("review"), "pending excluded: {s}");
        assert!(!s.contains("old"), "last-week excluded: {s}");
    }

    #[test]
    fn digest_thisweek_uses_mm_dd_hh_mm_per_line() {
        let ws = chrono::NaiveDate::from_ymd_opt(2026, 5, 11).unwrap();
        let mut d = view("API", 3, None, TaskStatus::Done, Some("merged"));
        d.updated_at = "2026-05-13T14:30:00+08:00".to_string();
        let s = format_digest_thisweek_reply(&[d], ws, 5);
        assert!(s.contains("05-13 14:30"), "cross-day MM-DD HH:MM: {s}");
        assert!(s.contains("— merged"), "result preview: {s}");
    }

    // -------- /digest_yesterday parse + format --------

    #[test]
    fn digest_yesterday_parser_clamps_and_defaults() {
        assert_eq!(
            parse_tg_command("/digest_yesterday"),
            Some(TgCommand::DigestYesterday { n: 5 })
        );
        assert_eq!(
            parse_tg_command("/digest_yesterday 10"),
            Some(TgCommand::DigestYesterday { n: 10 })
        );
        // clamp 21 → 20
        assert_eq!(
            parse_tg_command("/digest_yesterday 21"),
            Some(TgCommand::DigestYesterday { n: 20 })
        );
        // 非数字 → default 5
        assert_eq!(
            parse_tg_command("/digest_yesterday abc"),
            Some(TgCommand::DigestYesterday { n: 5 })
        );
    }

    #[test]
    fn digest_yesterday_empty_shows_friendly_fallback() {
        let y = chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let s = format_digest_yesterday_reply(&[], y, 5);
        assert!(s.contains("昨日（2026-05-16）暂无完成"), "{s}");
        // 三条 alt 入口教学
        assert!(s.contains("/digest"), "{s}");
        assert!(s.contains("/yesterday"), "{s}");
        assert!(s.contains("/touched_yesterday"), "{s}");
    }

    #[test]
    fn digest_yesterday_filters_to_done_on_yesterday_only() {
        let y = chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut y_done = view("y_d", 3, None, TaskStatus::Done, Some("yesterday r"));
        y_done.updated_at = "2026-05-16T15:00:00+08:00".to_string();
        let mut t_done = view("t_d", 3, None, TaskStatus::Done, Some("today r"));
        t_done.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let mut y_pending = view("y_p", 3, None, TaskStatus::Pending, None);
        y_pending.updated_at = "2026-05-16T09:00:00+08:00".to_string();
        let s = format_digest_yesterday_reply(&[y_done, t_done, y_pending], y, 5);
        assert!(s.contains("y_d"), "yesterday done included: {s}");
        assert!(!s.contains("t_d"), "today excluded: {s}");
        assert!(!s.contains("y_p"), "yesterday pending excluded (done-only): {s}");
        assert!(s.contains("完成 1 条"), "count reflects filter: {s}");
    }

    #[test]
    fn digest_yesterday_shows_result_preview_with_hh_mm_only() {
        let y = chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut d = view("写文档", 3, None, TaskStatus::Done, Some("提交到 PR #42"));
        d.updated_at = "2026-05-16T16:00:00+08:00".to_string();
        let s = format_digest_yesterday_reply(&[d], y, 5);
        assert!(s.contains("· 16:00 · 写文档"), "HH:MM prefix (no MM-DD): {s}");
        assert!(s.contains("— 提交到 PR #42"), "result preview: {s}");
        // 不含 MM-DD（已在 header）— 避免冗余
        assert!(!s.contains("05-16 16:00"), "{s}");
    }

    #[test]
    fn digest_yesterday_truncates_long_result() {
        let y = chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let long = "x".repeat(120);
        let mut d = view("t", 3, None, TaskStatus::Done, Some(long.as_str()));
        d.updated_at = "2026-05-16T10:00:00+08:00".to_string();
        let s = format_digest_yesterday_reply(&[d], y, 5);
        assert!(s.contains("…"), "truncate at 80: {s}");
    }

    // -------- /mute_today parse + format --------

    #[test]
    fn mute_today_parser_no_arg() {
        assert_eq!(parse_tg_command("/mute_today"), Some(TgCommand::MuteToday));
        assert_eq!(parse_tg_command("/MUTE_TODAY"), Some(TgCommand::MuteToday));
        // 尾部多余 token 容忍（与 /today / /sleep / /yesterday 同协议）
        assert_eq!(
            parse_tg_command("/mute_today extra ignored"),
            Some(TgCommand::MuteToday),
        );
    }

    #[test]
    fn mute_today_reply_includes_hours_when_60plus() {
        let s = format_mute_today_reply(120);
        assert!(s.contains("🌙"), "{s}");
        assert!(s.contains("到本地午夜"), "{s}");
        assert!(s.contains("还 120 分钟"), "{s}");
        assert!(s.contains("2.0 小时"), "should include hour decimal: {s}");
    }

    #[test]
    fn mute_today_reply_omits_hours_when_under_60() {
        let s = format_mute_today_reply(45);
        assert!(s.contains("还 45 分钟"), "{s}");
        assert!(!s.contains("小时"), "should omit hours for <60: {s}");
    }

    #[test]
    fn mute_today_reply_handles_edge_1_minute() {
        // 极端：午夜前 1 分（临近 23:59）
        let s = format_mute_today_reply(1);
        assert!(s.contains("还 1 分钟"), "{s}");
        assert!(!s.contains("小时"), "{s}");
    }

    // -------- /cascade_rename parse + format --------

    #[test]
    fn cascade_rename_parser_splits_on_double_colon() {
        assert_eq!(
            parse_tg_command("/cascade_rename old :: new"),
            Some(TgCommand::CascadeRename {
                title: "old".to_string(),
                new_title: "new".to_string(),
            })
        );
        // 前后空白 trim
        assert_eq!(
            parse_tg_command("/cascade_rename  写周报  ::  写 W21 周报  "),
            Some(TgCommand::CascadeRename {
                title: "写周报".to_string(),
                new_title: "写 W21 周报".to_string(),
            })
        );
    }

    #[test]
    fn cascade_rename_parser_missing_separator_yields_empty_new() {
        assert_eq!(
            parse_tg_command("/cascade_rename 整理 Downloads"),
            Some(TgCommand::CascadeRename {
                title: "整理 Downloads".to_string(),
                new_title: String::new(),
            })
        );
    }

    #[test]
    fn cascade_rename_reply_shows_old_new_and_count() {
        let s = format_cascade_rename_reply("写周报", "写 W21 周报", 3);
        assert!(s.contains("🔁"), "{s}");
        assert!(s.contains("「写周报」"), "{s}");
        assert!(s.contains("「写 W21 周报」"), "{s}");
        assert!(s.contains("同步 3 份"), "count line: {s}");
    }

    #[test]
    fn cascade_rename_reply_zero_count_shows_friendly_note() {
        let s = format_cascade_rename_reply("a", "b", 0);
        assert!(s.contains("无 detail.md 需要更新"), "{s}");
        assert!(!s.contains("同步 0"), "shouldn't say '同步 0': {s}");
    }

    // -------- /touched_today parse + format --------

    #[test]
    fn touched_today_parser_no_arg() {
        assert_eq!(
            parse_tg_command("/touched_today"),
            Some(TgCommand::TouchedToday)
        );
        assert_eq!(
            parse_tg_command("/TOUCHED_TODAY"),
            Some(TgCommand::TouchedToday)
        );
        // 尾部多余 token 容忍（与 /today / /today_done 同协议）
        assert_eq!(
            parse_tg_command("/touched_today extra"),
            Some(TgCommand::TouchedToday)
        );
    }

    #[test]
    fn touched_today_empty_shows_friendly_fallback() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let s = format_touched_today_reply(&[], today);
        assert!(s.contains("今日（2026-05-17）暂无动过"), "{s}");
        assert!(s.contains("/today"), "{s}");
        assert!(s.contains("/today_done"), "{s}");
    }

    #[test]
    fn touched_today_filters_to_today_only_any_status() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut t_pending = view("p", 3, None, TaskStatus::Pending, None);
        t_pending.updated_at = "2026-05-17T09:00:00+08:00".to_string();
        let mut t_done = view("d", 3, None, TaskStatus::Done, Some("r"));
        t_done.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let mut t_err = view("e", 3, None, TaskStatus::Error, Some("oops"));
        t_err.updated_at = "2026-05-17T11:00:00+08:00".to_string();
        let mut t_cancel = view("c", 3, None, TaskStatus::Cancelled, Some("nope"));
        t_cancel.updated_at = "2026-05-17T12:00:00+08:00".to_string();
        let mut y_done = view("y", 3, None, TaskStatus::Done, Some("y"));
        y_done.updated_at = "2026-05-16T20:00:00+08:00".to_string();
        let s = format_touched_today_reply(
            &[t_pending, t_done, t_err, t_cancel, y_done],
            today,
        );
        // 不限 status — 4 条今日都在
        assert!(s.contains("动过 4 条"), "{s}");
        assert!(s.contains("p"), "pending included: {s}");
        assert!(s.contains("d"), "done included: {s}");
        assert!(s.contains("e"), "error included: {s}");
        assert!(s.contains("c"), "cancelled included: {s}");
        assert!(!s.contains("\ny"), "yesterday excluded: {s}");
    }

    #[test]
    fn touched_today_sorts_by_updated_at_desc() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut early = view("早", 3, None, TaskStatus::Pending, None);
        early.updated_at = "2026-05-17T08:00:00+08:00".to_string();
        let mut late = view("晚", 3, None, TaskStatus::Pending, None);
        late.updated_at = "2026-05-17T22:30:00+08:00".to_string();
        let mut mid = view("中", 3, None, TaskStatus::Pending, None);
        mid.updated_at = "2026-05-17T14:00:00+08:00".to_string();
        let s = format_touched_today_reply(&[early, mid, late], today);
        let idx_late = s.find("晚").expect("晚 in output");
        let idx_mid = s.find("中").expect("中 in output");
        let idx_early = s.find("早").expect("早 in output");
        assert!(idx_late < idx_mid, "{s}");
        assert!(idx_mid < idx_early, "{s}");
    }

    #[test]
    fn touched_today_status_emojis_per_state() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut p = view("p", 3, None, TaskStatus::Pending, None);
        p.updated_at = "2026-05-17T09:00:00+08:00".to_string();
        let mut d = view("d", 3, None, TaskStatus::Done, None);
        d.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let mut e = view("e", 3, None, TaskStatus::Error, None);
        e.updated_at = "2026-05-17T11:00:00+08:00".to_string();
        let mut c = view("c", 3, None, TaskStatus::Cancelled, None);
        c.updated_at = "2026-05-17T12:00:00+08:00".to_string();
        let s = format_touched_today_reply(&[p, d, e, c], today);
        assert!(s.contains("⏳"), "pending: {s}");
        assert!(s.contains("✅"), "done: {s}");
        assert!(s.contains("⚠️"), "error: {s}");
        assert!(s.contains("🚫"), "cancelled: {s}");
    }

    #[test]
    fn touched_today_snooze_emoji_for_pending_with_snooze_marker() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut snoozed = view("延", 3, None, TaskStatus::Pending, None);
        snoozed.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        snoozed.raw_description = "[task pri=3] [snooze: 2026-05-18 09:00] 延".to_string();
        let s = format_touched_today_reply(&[snoozed], today);
        assert!(s.contains("💤"), "snoozed pending → 💤: {s}");
        // ⏳ 不应同时显（避免双 emoji 视觉冗余）
        assert!(!s.contains("⏳"), "non-snoozed emoji should be suppressed: {s}");
    }

    #[test]
    fn touched_today_includes_hh_mm_prefix() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut v = view("t", 3, None, TaskStatus::Pending, None);
        v.updated_at = "2026-05-17T14:30:00+08:00".to_string();
        let s = format_touched_today_reply(&[v], today);
        assert!(s.contains("14:30"), "should include HH:MM time: {s}");
    }

    #[test]
    fn touched_today_done_includes_result_preview() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        let mut d = view("写文档", 3, None, TaskStatus::Done, Some("提交到 PR #42"));
        d.updated_at = "2026-05-17T16:00:00+08:00".to_string();
        let s = format_touched_today_reply(&[d], today);
        assert!(s.contains("— 提交到 PR #42"), "{s}");
    }

    // -------- /oldest_done parse + format --------

    #[test]
    fn oldest_done_parser_clamps_and_defaults() {
        assert_eq!(
            parse_tg_command("/oldest_done"),
            Some(TgCommand::OldestDone { n: 5 })
        );
        assert_eq!(
            parse_tg_command("/oldest_done 10"),
            Some(TgCommand::OldestDone { n: 10 })
        );
        // clamp 21 → 20
        assert_eq!(
            parse_tg_command("/oldest_done 21"),
            Some(TgCommand::OldestDone { n: 20 })
        );
        // 非数字 → default 5
        assert_eq!(
            parse_tg_command("/oldest_done abc"),
            Some(TgCommand::OldestDone { n: 5 })
        );
    }

    #[test]
    fn oldest_done_empty_shows_teaching_hint() {
        let s = format_oldest_done_reply(&[], 5);
        assert!(s.contains("暂无完成记录"), "{s}");
        assert!(s.contains("/done"), "should point to /done: {s}");
        assert!(s.contains("/oldest_done"), "{s}");
    }

    #[test]
    fn oldest_done_sorts_by_updated_at_asc() {
        let mut early = view("早", 3, None, TaskStatus::Done, Some("e"));
        early.updated_at = "2026-05-01T08:00:00+08:00".to_string();
        let mut mid = view("中", 3, None, TaskStatus::Done, Some("m"));
        mid.updated_at = "2026-05-10T12:00:00+08:00".to_string();
        let mut late = view("晚", 3, None, TaskStatus::Done, Some("l"));
        late.updated_at = "2026-05-17T22:30:00+08:00".to_string();
        // 输入乱序，formatter 应内部排序 — 最早在前
        let s = format_oldest_done_reply(&[late, mid, early], 5);
        let idx_early = s.find("早").expect("早 in output");
        let idx_mid = s.find("中").expect("中 in output");
        let idx_late = s.find("晚").expect("晚 in output");
        assert!(idx_early < idx_mid, "早 before 中: {s}");
        assert!(idx_mid < idx_late, "中 before 晚: {s}");
    }

    #[test]
    fn oldest_done_filters_to_done_only() {
        let mut d = view("d", 3, None, TaskStatus::Done, Some("r"));
        d.updated_at = "2026-05-01T08:00:00+08:00".to_string();
        let mut p = view("p", 3, None, TaskStatus::Pending, None);
        p.updated_at = "2026-05-02T09:00:00+08:00".to_string();
        let mut c = view("c", 3, None, TaskStatus::Cancelled, None);
        c.updated_at = "2026-05-03T10:00:00+08:00".to_string();
        let s = format_oldest_done_reply(&[d, p, c], 5);
        assert!(s.contains("\n· "), "{s}");
        assert!(s.contains("d"), "done included: {s}");
        assert!(!s.contains("\n· 2026-05-02 09:00 · p"), "pending excluded: {s}");
        assert!(!s.contains("\n· 2026-05-03 10:00 · c"), "cancelled excluded: {s}");
        assert!(s.contains("共 1"), "count reflects done filter: {s}");
    }

    #[test]
    fn oldest_done_caps_at_n_and_shows_remainder_hint() {
        let mut views = Vec::new();
        for i in 1..=10 {
            let mut v = view(&format!("t{}", i), 3, None, TaskStatus::Done, None);
            v.updated_at = format!("2026-05-{:02}T08:00:00+08:00", i);
            views.push(v);
        }
        let s = format_oldest_done_reply(&views, 3);
        assert!(s.contains("最早完成的 3 条"), "{s}");
        assert!(s.contains("共 10"), "{s}");
        assert!(s.contains("还有 7 条更晚完成"), "remainder hint: {s}");
        assert!(s.contains("/oldest_done 10"), "remainder cap hint: {s}");
    }

    // -------- /touched_yesterday parse + format --------

    #[test]
    fn touched_yesterday_parser_no_arg() {
        assert_eq!(
            parse_tg_command("/touched_yesterday"),
            Some(TgCommand::TouchedYesterday)
        );
        assert_eq!(
            parse_tg_command("/TOUCHED_YESTERDAY"),
            Some(TgCommand::TouchedYesterday)
        );
    }

    #[test]
    fn touched_yesterday_empty_shows_yesterday_specific_hint() {
        let y = chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let s = format_touched_yesterday_reply(&[], y);
        // 标题用「昨日」
        assert!(s.contains("昨日（2026-05-16）"), "{s}");
        // 空集兜底教学指向不同于 /touched_today（避免循环建议）
        assert!(s.contains("/touched_today"), "{s}");
        assert!(s.contains("/yesterday"), "{s}");
        assert!(!s.contains("/today_done"), "yesterday hint should not loop to today_done: {s}");
    }

    #[test]
    fn touched_yesterday_filters_yesterday_only() {
        let y = chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut y_pending = view("y_p", 3, None, TaskStatus::Pending, None);
        y_pending.updated_at = "2026-05-16T09:00:00+08:00".to_string();
        let mut t_done = view("t_d", 3, None, TaskStatus::Done, Some("r"));
        t_done.updated_at = "2026-05-17T10:00:00+08:00".to_string();
        let s = format_touched_yesterday_reply(&[y_pending, t_done], y);
        assert!(s.contains("y_p"), "yesterday included: {s}");
        assert!(!s.contains("t_d"), "today excluded: {s}");
    }

    #[test]
    fn touched_yesterday_reuses_emoji_and_snooze_logic() {
        let y = chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut snoozed = view("s", 3, None, TaskStatus::Pending, None);
        snoozed.updated_at = "2026-05-16T10:00:00+08:00".to_string();
        snoozed.raw_description = "[task pri=3] [snooze: 2026-05-17 09:00] s".to_string();
        let mut done = view("d", 3, None, TaskStatus::Done, Some("ok"));
        done.updated_at = "2026-05-16T11:00:00+08:00".to_string();
        let s = format_touched_yesterday_reply(&[snoozed, done], y);
        assert!(s.contains("💤"), "snoozed pending → 💤: {s}");
        assert!(s.contains("✅"), "done: {s}");
        assert!(s.contains("— ok"), "result preview: {s}");
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
    // -------- /random_pinned parse + format --------

    #[test]
    fn random_pinned_parser_no_arg() {
        assert_eq!(
            parse_tg_command("/random_pinned"),
            Some(TgCommand::RandomPinned),
        );
        assert_eq!(
            parse_tg_command("/RANDOM_PINNED"),
            Some(TgCommand::RandomPinned),
        );
        assert_eq!(
            parse_tg_command("/random_pinned extra"),
            Some(TgCommand::RandomPinned),
        );
    }

    #[test]
    fn random_pinned_empty_shows_friendly_fallback() {
        let s = format_random_pinned_reply(&[], 0);
        assert!(s.contains("无 pinned active task"), "{s}");
        assert!(s.contains("/pin"), "教学指 /pin 设置: {s}");
        assert!(s.contains("/random"), "教学指 /random fallback: {s}");
    }

    #[test]
    fn random_pinned_picks_pinned_active_only() {
        // pinned + pending → candidate
        let mut pinned_pending = view("PP", 3, None, TaskStatus::Pending, None);
        pinned_pending.pinned = true;
        // pinned + done → 不算 active
        let mut pinned_done = view("PD", 3, None, TaskStatus::Done, Some("r"));
        pinned_done.pinned = true;
        // 非 pinned + pending → 不在 pinned subset
        let unpinned_pending = view("UP", 3, None, TaskStatus::Pending, None);
        // pinned + error → candidate
        let mut pinned_error = view("PE", 3, None, TaskStatus::Error, Some("oops"));
        pinned_error.pinned = true;
        let s = format_random_pinned_reply(
            &[pinned_pending, pinned_done, unpinned_pending, pinned_error],
            0, // seed 0 取 candidates[0]
        );
        // header 应显「共 2 条 pinned active」（PP + PE）
        assert!(s.contains("共 2 条 pinned active"), "{s}");
        // seed 0 → candidates[0] = PP（pinned_pending 在过滤后首位）
        assert!(s.contains("「PP」"), "seed 0 picks first: {s}");
        // PD / UP 不是 candidate，但 verify 不在标题位置（出现在 reply 文本里是 OK
        // 的，因为 raw_description preview 可能含 "PP"… 这里只验 header line）
    }

    #[test]
    fn random_pinned_seed_cycles_candidates() {
        let mut a = view("A", 3, None, TaskStatus::Pending, None);
        a.pinned = true;
        let mut b = view("B", 3, None, TaskStatus::Pending, None);
        b.pinned = true;
        let mut c = view("C", 3, None, TaskStatus::Pending, None);
        c.pinned = true;
        let views = vec![a, b, c];
        // seed % 3 cycles through 0,1,2
        let s0 = format_random_pinned_reply(&views, 0);
        let s1 = format_random_pinned_reply(&views, 1);
        let s2 = format_random_pinned_reply(&views, 2);
        let s3 = format_random_pinned_reply(&views, 3);
        assert!(s0.contains("「A」"), "{s0}");
        assert!(s1.contains("「B」"), "{s1}");
        assert!(s2.contains("「C」"), "{s2}");
        // seed 3 wraps to candidates[0] = A
        assert!(s3.contains("「A」"), "wrap: {s3}");
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

    // -------- /show_speech parse + format --------

    #[test]
    fn show_speech_parses_default_5_no_arg() {
        assert_eq!(
            parse_tg_command("/show_speech"),
            Some(TgCommand::ShowSpeech { n: 5 })
        );
        assert_eq!(
            parse_tg_command("/show_speech   "),
            Some(TgCommand::ShowSpeech { n: 5 })
        );
    }

    #[test]
    fn show_speech_parses_explicit_n() {
        assert_eq!(
            parse_tg_command("/show_speech 10"),
            Some(TgCommand::ShowSpeech { n: 10 })
        );
        assert_eq!(
            parse_tg_command("/show_speech 1"),
            Some(TgCommand::ShowSpeech { n: 1 })
        );
    }

    #[test]
    fn show_speech_clamps_to_1_20_range() {
        assert_eq!(
            parse_tg_command("/show_speech 0"),
            Some(TgCommand::ShowSpeech { n: 1 })
        );
        assert_eq!(
            parse_tg_command("/show_speech 9999"),
            Some(TgCommand::ShowSpeech { n: 20 })
        );
    }

    #[test]
    fn show_speech_garbage_arg_falls_back_to_default() {
        assert_eq!(
            parse_tg_command("/show_speech abc"),
            Some(TgCommand::ShowSpeech { n: 5 })
        );
    }

    #[test]
    fn format_show_speech_empty_says_no_history() {
        let s = format_show_speech_reply(&[]);
        assert!(s.contains("🗣"), "{s}");
        assert!(s.contains("speech_history 空"), "{s}");
    }

    #[test]
    fn format_show_speech_reverses_to_newest_first() {
        use chrono::TimeZone;
        // oldest-first input（与 recent_speeches_with_meta 返回顺序同）
        let ts_old = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 10, 0, 0)
            .unwrap()
            .to_rfc3339();
        let ts_new = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 14, 0, 0)
            .unwrap()
            .to_rfc3339();
        let entries = vec![
            (ts_old.clone(), "早些说的".to_string()),
            (ts_new.clone(), "最新说的".to_string()),
        ];
        let s = format_show_speech_reply(&entries);
        // 最新应在前
        let pos_new = s.find("最新说的").expect("newest");
        let pos_old = s.find("早些说的").expect("oldest");
        assert!(pos_new < pos_old, "newest first: {s}");
        assert!(s.contains("最近 2 条主动开口"), "header: {s}");
    }

    #[test]
    fn format_show_speech_truncates_long_text_to_80_chars() {
        use chrono::TimeZone;
        let ts = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 14, 0, 0)
            .unwrap()
            .to_rfc3339();
        let long_text: String = "啊".repeat(100);
        let entries = vec![(ts, long_text)];
        let s = format_show_speech_reply(&entries);
        let counted = s.chars().filter(|&c| c == '啊').count();
        assert_eq!(counted, 80, "expected 80-char cap: {s}");
        assert!(s.contains("…"), "{s}");
    }

    #[test]
    fn format_show_speech_flattens_newlines_in_text() {
        use chrono::TimeZone;
        let ts = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 14, 0, 0)
            .unwrap()
            .to_rfc3339();
        let entries = vec![(
            ts,
            "line1\nline2\nline3".to_string(),
        )];
        let s = format_show_speech_reply(&entries);
        assert!(s.contains("line1 line2 line3"), "newlines flattened: {s}");
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

    // -------- /peek parse + format_peek_reply --------

    #[test]
    fn peek_parser_takes_all_args_as_title() {
        assert_eq!(
            parse_tg_command("/peek 整理 Downloads"),
            Some(TgCommand::Peek {
                title: "整理 Downloads".to_string()
            })
        );
    }

    #[test]
    fn peek_parser_empty_title_yields_empty_string() {
        // 空 title 留给 handler 走 missing-argument — 与 /show 同模板
        assert_eq!(
            parse_tg_command("/peek"),
            Some(TgCommand::Peek {
                title: String::new()
            })
        );
    }

    #[test]
    fn peek_reply_status_emoji_matches_state() {
        let s = format_peek_reply("写周报", "", TaskStatus::Pending);
        assert!(s.starts_with("⏳"), "pending → ⏳: {s}");
        let s = format_peek_reply("写周报", "", TaskStatus::Done);
        assert!(s.starts_with("✅"), "done → ✅: {s}");
        let s = format_peek_reply("写周报", "", TaskStatus::Error);
        assert!(s.starts_with("⚠️"), "error → ⚠️: {s}");
        let s = format_peek_reply("写周报", "", TaskStatus::Cancelled);
        assert!(s.starts_with("🚫"), "cancelled → 🚫: {s}");
    }

    #[test]
    fn peek_reply_bare_title_when_no_markers_or_schedule() {
        // 空 raw_description → 仅 emoji + 「title」，无后续段
        let s = format_peek_reply("写周报", "", TaskStatus::Pending);
        assert_eq!(s, "⏳ 「写周报」");
    }

    #[test]
    fn peek_reply_includes_every_schedule_prefix() {
        let s = format_peek_reply("整理 Downloads", "[every: 09:00] 清桌面", TaskStatus::Pending);
        assert!(s.contains("🕐"), "should have schedule clock emoji: {s}");
        assert!(s.contains("every: 09:00"), "should keep schedule body verbatim: {s}");
    }

    #[test]
    fn peek_reply_includes_once_and_deadline_schedule() {
        let s = format_peek_reply("t", "[once: 2026-05-20 14:00] meet client", TaskStatus::Pending);
        assert!(s.contains("once: 2026-05-20 14:00"), "{s}");
        let s = format_peek_reply("t", "[deadline: 2026-06-01] submit", TaskStatus::Pending);
        assert!(s.contains("deadline: 2026-06-01"), "{s}");
    }

    #[test]
    fn peek_reply_omits_schedule_when_no_prefix() {
        // raw 起始不是 [every|once|deadline:] → 无 🕐 段
        let s = format_peek_reply("t", "just a free-form description", TaskStatus::Pending);
        assert!(!s.contains("🕐"), "no schedule prefix → no clock: {s}");
    }

    #[test]
    fn peek_reply_omits_schedule_when_prefix_not_at_start() {
        // 中段出现 [every: ...] 不算 schedule（与 parse_butler_schedule_prefix 同语义）
        let s = format_peek_reply("t", "free-form [every: 09:00] mid-text", TaskStatus::Pending);
        assert!(!s.contains("🕐"), "mid-text prefix should not count: {s}");
    }

    #[test]
    fn peek_reply_shows_pinned_silent_snooze_blocked_markers() {
        let raw = "[task pri=3] [pinned] [silent] [snooze: 18:00] [blockedBy: foo] body";
        let s = format_peek_reply("t", raw, TaskStatus::Pending);
        assert!(s.contains("📌"), "pinned → 📌: {s}");
        assert!(s.contains("🔇"), "silent → 🔇: {s}");
        assert!(s.contains("💤"), "snooze → 💤: {s}");
        assert!(s.contains("🔒"), "blockedBy → 🔒: {s}");
    }

    #[test]
    fn peek_reply_omits_marker_section_when_none_present() {
        // 仅 priority + body，无 pinned/silent/snooze/blockedBy → markers 段省略
        let s = format_peek_reply("t", "[task pri=3] some body", TaskStatus::Pending);
        assert!(!s.contains("📌"), "{s}");
        assert!(!s.contains("🔇"), "{s}");
        assert!(!s.contains("💤"), "{s}");
        assert!(!s.contains("🔒"), "{s}");
    }

    #[test]
    fn peek_reply_does_not_show_state_change_markers_like_done_or_result() {
        // [done] / [result:] / [cancelled:] / [error:] 是状态变化 — 状态本身
        // 已在 emoji 表达，不应在 markers 段重复
        let raw = "[task pri=3] body [done] [result: ok]";
        let s = format_peek_reply("t", raw, TaskStatus::Done);
        assert!(!s.contains("✅ done"), "shouldn't echo done as marker: {s}");
        assert!(!s.contains("result"), "shouldn't echo [result:] verbatim: {s}");
    }

    #[test]
    fn peek_reply_priority_label_from_task_pri_marker() {
        let s = format_peek_reply("t", "[task pri=5] body", TaskStatus::Pending);
        assert!(s.contains("P5"), "should show priority label: {s}");
        let s = format_peek_reply("t", "[task pri=0] body", TaskStatus::Pending);
        assert!(s.contains("P0"), "P0 should still show: {s}");
    }

    #[test]
    fn peek_reply_priority_omitted_when_no_task_pri_marker() {
        // 无 [task pri=N] → 不显 P 段
        let s = format_peek_reply("t", "free-form body", TaskStatus::Pending);
        assert!(!s.contains(" · P"), "no pri marker → no P label: {s}");
    }

    // -------- /dup parse + format_dup_reply --------

    #[test]
    fn dup_parser_takes_all_args_as_title() {
        assert_eq!(
            parse_tg_command("/dup 整理 Downloads"),
            Some(TgCommand::Dup {
                title: "整理 Downloads".to_string()
            })
        );
    }

    #[test]
    fn dup_parser_empty_title_yields_empty_string() {
        assert_eq!(
            parse_tg_command("/dup"),
            Some(TgCommand::Dup {
                title: String::new()
            })
        );
    }

    #[test]
    fn dup_reply_shows_src_and_new_titles() {
        let s = format_dup_reply("整理 Downloads", "整理 Downloads (副本)");
        assert!(s.contains("📑"), "{s}");
        assert!(s.contains("「整理 Downloads」"), "src in 「」: {s}");
        assert!(s.contains("「整理 Downloads (副本)」"), "new in 「」: {s}");
    }

    #[test]
    fn dup_reply_notes_inheritance_and_strip_summary() {
        let s = format_dup_reply("a", "a (副本)");
        // 注脚说明两类 markers — 让 owner 看清楚副本继承了什么 / 丢了什么
        assert!(s.contains("继承"), "should note what's inherited: {s}");
        assert!(s.contains("剥"), "should note what's stripped: {s}");
    }

    #[test]
    fn dup_reply_handles_unique_filename_suffix() {
        // memory_edit 同 title 重复时返回 `<title>_1` / `_2`；formatter 透
        // 显新 title 不做特殊处理
        let s = format_dup_reply("整理", "整理 (副本)_1");
        assert!(s.contains("「整理 (副本)_1」"), "{s}");
    }

    // -------- /snippets parse_snippet_marker + format --------

    #[test]
    fn snippets_parser_no_arg() {
        assert_eq!(parse_tg_command("/snippets"), Some(TgCommand::Snippets));
        assert_eq!(parse_tg_command("/SNIPPETS"), Some(TgCommand::Snippets));
    }

    #[test]
    fn parse_snippet_marker_returns_none_when_absent() {
        assert_eq!(parse_snippet_marker("[task pri=3] 普通 task"), None);
        assert_eq!(parse_snippet_marker(""), None);
        // [snippetXY] 不该命中（防 token-boundary 碰撞）
        assert_eq!(parse_snippet_marker("[snippetXY]"), None);
    }

    #[test]
    fn parse_snippet_marker_returns_empty_label_for_bare_marker() {
        assert_eq!(
            parse_snippet_marker("[task pri=3] [snippet] 模板"),
            Some(String::new())
        );
        assert_eq!(
            parse_snippet_marker("[snippet:] 空 label"),
            Some(String::new())
        );
    }

    #[test]
    fn parse_snippet_marker_extracts_label() {
        assert_eq!(
            parse_snippet_marker("[snippet: PR template] body"),
            Some("PR template".to_string())
        );
        // 全角冒号
        assert_eq!(
            parse_snippet_marker("[snippet：模板A] body"),
            Some("模板A".to_string())
        );
        // 空格分隔（[snippet name] — 不带冒号但有空格）
        assert_eq!(
            parse_snippet_marker("[snippet 模板B] body"),
            Some("模板B".to_string())
        );
    }

    #[test]
    fn parse_snippet_marker_takes_first_occurrence_when_multiple() {
        assert_eq!(
            parse_snippet_marker("[snippet: A] body [snippet: B]"),
            Some("A".to_string())
        );
    }

    #[test]
    fn format_snippets_empty_shows_teaching_hint() {
        let s = format_snippets_reply(&[]);
        assert!(s.contains("还没标 snippet"), "{s}");
        assert!(s.contains("/edit"), "should teach via /edit example: {s}");
    }

    #[test]
    fn format_snippets_lists_titles_with_labels_and_preview() {
        let labeled = crate::task_queue::TaskView {
            title: "PR review template".to_string(),
            body: "".to_string(),
            raw_description: "[task pri=3] [snippet: PR template] 1. 看 diff 2. 跑测试 3. 提评论".to_string(),
            priority: 3,
            due: None,
            status: crate::task_queue::TaskStatus::Pending,
            error_message: None,
            tags: vec![],
            result: None,
            created_at: "2026-05-04T13:00:00+08:00".to_string(),
            updated_at: "2026-05-04T13:00:00+08:00".to_string(),
            detail_path: "".to_string(),
            blocked_by: vec![],
            snoozed_until: None,
            pinned: false,
        };
        let bare = crate::task_queue::TaskView {
            title: "决策日志开头".to_string(),
            raw_description: "[task pri=3] [snippet] 今天的关键决策".to_string(),
            ..labeled.clone()
        };
        let s = format_snippets_reply(&[labeled, bare]);
        assert!(s.contains("📎 snippets · 2 条"), "{s}");
        assert!(s.contains("PR review template"), "{s}");
        assert!(s.contains("[PR template]"), "label shown in brackets: {s}");
        assert!(s.contains("决策日志开头"), "{s}");
        // bare marker → no label brackets shown after title
        assert!(
            !s.contains("决策日志开头 ["),
            "bare snippet should not render empty label brackets: {s}"
        );
        // body preview present
        assert!(s.contains("看 diff"), "body preview: {s}");
        assert!(s.contains("今天的关键决策"), "{s}");
    }

    // -------- /recent_events parse + format --------

    #[test]
    fn recent_events_parser_uses_default_n_when_only_title() {
        // 单 token 数字也视作 title（与 /tasks 索引解析协同）
        assert_eq!(
            parse_tg_command("/recent_events 1"),
            Some(TgCommand::RecentEvents {
                title: "1".to_string(),
                n: 5,
            })
        );
        // 文字 title 同样 default N
        assert_eq!(
            parse_tg_command("/recent_events 整理 Downloads"),
            Some(TgCommand::RecentEvents {
                title: "整理 Downloads".to_string(),
                n: 5,
            })
        );
    }

    #[test]
    fn recent_events_parser_extracts_trailing_n() {
        assert_eq!(
            parse_tg_command("/recent_events 整理 Downloads 10"),
            Some(TgCommand::RecentEvents {
                title: "整理 Downloads".to_string(),
                n: 10,
            })
        );
        // 索引 + 显式 N（2 token，最后是数字）
        assert_eq!(
            parse_tg_command("/recent_events 1 10"),
            Some(TgCommand::RecentEvents {
                title: "1".to_string(),
                n: 10,
            })
        );
    }

    #[test]
    fn recent_events_parser_keeps_oversize_n_token_in_title() {
        // 末 token 是数字但 >20 → 不剥（不在 1..=20 范围），按 default + 包含数字的 title
        assert_eq!(
            parse_tg_command("/recent_events 题 99"),
            Some(TgCommand::RecentEvents {
                title: "题 99".to_string(),
                n: 5,
            })
        );
        // 0 也不剥（不在 1..=20）
        assert_eq!(
            parse_tg_command("/recent_events 题 0"),
            Some(TgCommand::RecentEvents {
                title: "题 0".to_string(),
                n: 5,
            })
        );
    }

    #[test]
    fn recent_events_parser_empty_title_ok() {
        // handler 走 missing-arg；parser 不抢话
        assert_eq!(
            parse_tg_command("/recent_events"),
            Some(TgCommand::RecentEvents {
                title: String::new(),
                n: 5,
            })
        );
    }

    #[test]
    fn format_recent_events_empty_history_shows_friendly_hint() {
        let s = format_recent_events_reply("整理", &[], 0, 5);
        assert!(s.contains("最近事件"), "{s}");
        assert!(s.contains("整理"), "{s}");
        assert!(s.contains("/show"), "should suggest /show fallback: {s}");
    }

    #[test]
    fn format_recent_events_takes_last_n_chronological() {
        // entries 是 chronological（旧→新）；recent = 末尾 N
        let entries = vec![
            TimelineEntry {
                timestamp: "2026-05-01 09:00:00".to_string(),
                action: "create".to_string(),
                markers: vec![],
                was: None,
            },
            TimelineEntry {
                timestamp: "2026-05-02 10:00:00".to_string(),
                action: "update".to_string(),
                markers: vec!["[pinned]".to_string()],
                was: None,
            },
            TimelineEntry {
                timestamp: "2026-05-03 11:00:00".to_string(),
                action: "update".to_string(),
                markers: vec!["[done]".to_string()],
                was: None,
            },
        ];
        // N=2 → 取最后 2 条（pinned + done）
        let s = format_recent_events_reply("t", &entries, 3, 2);
        assert!(s.contains("最近 2 个事件"), "{s}");
        assert!(s.contains("（共 3）"), "should show total: {s}");
        assert!(s.contains("[pinned]"), "should include pinned: {s}");
        assert!(s.contains("[done]"), "should include done: {s}");
        // 最早的「创建」事件应被切掉（取末尾 2 条不含它）
        assert!(!s.contains("· 创建"), "shouldn't include earliest create: {s}");
    }

    #[test]
    fn format_recent_events_caps_at_entries_len_when_n_exceeds() {
        let entries = vec![TimelineEntry {
            timestamp: "2026-05-01 09:00:00".to_string(),
            action: "create".to_string(),
            markers: vec![],
            was: None,
        }];
        // N=20 但仅 1 条 entry → 显 1 条
        let s = format_recent_events_reply("t", &entries, 1, 20);
        assert!(s.contains("最近 1 个事件"), "{s}");
        assert!(s.contains("（共 1）"), "{s}");
    }

    #[test]
    fn format_snippets_truncates_long_body_preview() {
        let long_body = "a".repeat(SNIPPET_BODY_PREVIEW_CHARS + 50);
        let raw = format!("[task pri=3] [snippet] {}", long_body);
        let v = crate::task_queue::TaskView {
            title: "long".to_string(),
            body: "".to_string(),
            raw_description: raw,
            priority: 3,
            due: None,
            status: crate::task_queue::TaskStatus::Pending,
            error_message: None,
            tags: vec![],
            result: None,
            created_at: "2026-05-04T13:00:00+08:00".to_string(),
            updated_at: "2026-05-04T13:00:00+08:00".to_string(),
            detail_path: "".to_string(),
            blocked_by: vec![],
            snoozed_until: None,
            pinned: false,
        };
        let s = format_snippets_reply(&[v]);
        assert!(s.contains("…"), "should truncate with ellipsis: {s}");
    }

    #[test]
    fn peek_reply_full_combo_layout() {
        // 全段都有：emoji · title · schedule · markers · priority
        let raw = "[every: 09:00] [task pri=3] [pinned] [silent] 早会";
        let s = format_peek_reply("早会", raw, TaskStatus::Pending);
        // 段间 separator
        let dots: Vec<&str> = s.split(" · ").collect();
        assert!(dots.len() >= 4, "should have ≥4 dot-separated segments: {s}");
        assert!(s.contains("⏳"), "{s}");
        assert!(s.contains("「早会」"), "{s}");
        assert!(s.contains("🕐 every: 09:00"), "{s}");
        assert!(s.contains("📌"), "{s}");
        assert!(s.contains("🔇"), "{s}");
        assert!(s.contains("P3"), "{s}");
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
                was: None,
            },
            TimelineEntry {
                timestamp: "2026-05-17T14:00:00+08:00".to_string(),
                action: "update".to_string(),
                markers: vec!["[done]".to_string(), "[result: 已发送]".to_string()],
                was: None,
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
    fn extract_was_from_snippet_basic() {
        assert_eq!(
            extract_was_from_snippet("[was: 整理 Downloads]"),
            Some("整理 Downloads".to_string()),
        );
        // 前后有其它文本（不应发生但 defensive）
        assert_eq!(
            extract_was_from_snippet("noise [was: A] tail"),
            Some("A".to_string()),
        );
        // 80 字截断把尾 `]` 砍掉 → 取到 snippet 末
        assert_eq!(
            extract_was_from_snippet("[was: very long old title cut by snippet limit…"),
            Some("very long old title cut by snippet limit".to_string()),
        );
        // 无 prefix → None
        assert_eq!(extract_was_from_snippet("just regular snippet"), None);
        // 空 prefix value → None
        assert_eq!(extract_was_from_snippet("[was: ]"), None);
    }

    #[test]
    fn timeline_reply_renders_rename_with_old_title() {
        let entries = vec![TimelineEntry {
            timestamp: "2026-05-17T15:00:00+08:00".to_string(),
            action: "rename".to_string(),
            markers: vec![],
            was: Some("写周报".to_string()),
        }];
        let s = format_timeline_reply("写 W21 周报", &entries, 1);
        assert!(
            s.contains("🔁 05-17 15:00 · 重命名 from 「写周报」"),
            "rename line: {s}",
        );
        // 不应 fallback 到「更新（无 marker 变化）」误判
        assert!(!s.contains("无 marker 变化"), "{s}");
    }

    #[test]
    fn timeline_reply_renders_rename_with_unknown_old_fallback() {
        // best-effort：snippet 截断导致 was=None 时仍能识别是 rename
        let entries = vec![TimelineEntry {
            timestamp: "2026-05-17T15:00:00+08:00".to_string(),
            action: "rename".to_string(),
            markers: vec![],
            was: None,
        }];
        let s = format_timeline_reply("X", &entries, 1);
        assert!(s.contains("🔁"), "rename emoji even without was: {s}");
        assert!(s.contains("重命名（old title 不可解）"), "{s}");
    }

    #[test]
    fn recent_events_reply_renders_rename() {
        let entries = vec![TimelineEntry {
            timestamp: "2026-05-17T15:00:00+08:00".to_string(),
            action: "rename".to_string(),
            markers: vec![],
            was: Some("整理 Downloads".to_string()),
        }];
        let s = format_recent_events_reply("清理桌面", &entries, 1, 5);
        assert!(
            s.contains("🔁 05-17 15:00 · 重命名 from 「整理 Downloads」"),
            "rename line in recent_events: {s}",
        );
    }

    #[test]
    fn timeline_reply_caps_at_30_entries_with_overflow_hint() {
        let entries: Vec<TimelineEntry> = (0..50)
            .map(|i| TimelineEntry {
                timestamp: format!("2026-05-17T{:02}:00:00+08:00", i % 24),
                action: "update".to_string(),
                markers: vec![format!("[result: r{}]", i)],
                was: None,
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
            was: None,
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
