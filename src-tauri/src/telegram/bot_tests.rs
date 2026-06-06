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
