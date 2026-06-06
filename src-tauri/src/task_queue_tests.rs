    use super::*;
    use chrono::NaiveDate;

    fn dt(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, 0)
            .unwrap()
    }

    fn view(
        title: &str,
        priority: u8,
        due: Option<&str>,
        status: TaskStatus,
        created_at: &str,
    ) -> TaskView {
        TaskView {
            title: title.to_string(),
            body: String::new(),
            raw_description: String::new(),
            priority,
            due: due.map(String::from),
            status,
            error_message: None,
            tags: Vec::new(),
            result: None,
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
            detail_path: String::new(),
            blocked_by: Vec::new(),
            snoozed_until: None,
            pinned: false,
        }
    }

    // ---------------- parse_task_header ----------------

    #[test]
    fn parses_full_header() {
        let h = parse_task_header("[task pri=3 due=2026-05-05T18:00] 整理 Downloads")
            .expect("should parse");
        assert_eq!(h.priority, 3);
        assert_eq!(h.due, Some(dt(2026, 5, 5, 18, 0)));
        assert_eq!(h.body, "整理 Downloads");
    }

    #[test]
    fn parses_pri_only_header() {
        let h = parse_task_header("[task pri=1] 喝水").expect("should parse");
        assert_eq!(h.priority, 1);
        assert_eq!(h.due, None);
        assert_eq!(h.body, "喝水");
    }

    #[test]
    fn accepts_field_order_swap() {
        // due 在 pri 前面也接受 — 容忍写法
        let h = parse_task_header("[task due=2026-05-05T09:00 pri=2] 早会").expect("should parse");
        assert_eq!(h.priority, 2);
        assert_eq!(h.due, Some(dt(2026, 5, 5, 9, 0)));
    }

    #[test]
    fn returns_none_for_missing_brackets() {
        assert!(parse_task_header("task pri=1 没有方括号").is_none());
        assert!(parse_task_header("[task pri=1 没闭合").is_none());
    }

    #[test]
    fn strip_archive_markers_clears_terminal_state_but_keeps_schedule() {
        // 归档恢复场景：description 含 [archived:] [done] [result:] 标记，
        // 剥光后应保留 [task pri=] header / [every:] schedule prefix / #tag。
        let input =
            "[archived: 2026-04-01] [task pri=3] [every: 09:00] 写日报 #工作 [done] [result: 写了 5 段]";
        let out = strip_archive_markers(input);
        // 关键 marker 都被剥
        assert!(!out.contains("[archived"));
        assert!(!out.contains("[done"));
        assert!(!out.contains("[result"));
        // 任务核心保留
        assert!(out.contains("[task pri=3]"));
        assert!(out.contains("[every: 09:00]"));
        assert!(out.contains("写日报"));
        assert!(out.contains("#工作"));
    }

    #[test]
    fn strip_archive_markers_handles_cancelled_and_error() {
        let cancelled = strip_archive_markers(
            "[archived: 2026-04-01] [task pri=1] 拖延的事 [cancelled: 不做了]",
        );
        assert!(!cancelled.contains("[cancelled"));
        assert!(cancelled.contains("拖延的事"));

        let errored =
            strip_archive_markers("[archived: 2026-04-01] [task pri=2] 死循环 [error: 超时]");
        assert!(!errored.contains("[error"));
        assert!(errored.contains("死循环"));
    }

    #[test]
    fn returns_none_for_priority_out_of_range() {
        // 10 超出 0..=9 — 拒绝而不是 saturating
        assert!(parse_task_header("[task pri=10] x").is_none());
        assert!(parse_task_header("[task pri=255] x").is_none());
    }

    #[test]
    fn returns_none_for_invalid_due() {
        assert!(parse_task_header("[task pri=1 due=not-a-date] x").is_none());
        assert!(parse_task_header("[task pri=1 due=2026-13-99T25:99] x").is_none());
    }

    #[test]
    fn returns_none_for_unknown_token() {
        // 严格：未知字段视作格式错误，避免未来扩展时静默忽略
        assert!(parse_task_header("[task pri=1 status=done] x").is_none());
    }

    #[test]
    fn returns_none_for_duplicate_field() {
        assert!(parse_task_header("[task pri=1 pri=2] x").is_none());
        assert!(parse_task_header("[task due=2026-05-05T09:00 due=2026-05-05T10:00 pri=1] x").is_none());
    }

    #[test]
    fn returns_none_for_non_task_brackets() {
        // [once:...] / [every:...] / [done] 都不是 task header — 别误命中
        assert!(parse_task_header("[once: 2026-05-05T18:00] x").is_none());
        assert!(parse_task_header("[every: 09:00] x").is_none());
        assert!(parse_task_header("[done] x").is_none());
    }

    #[test]
    fn body_is_trimmed_and_can_be_empty() {
        let h = parse_task_header("[task pri=0]").expect("empty body still valid");
        assert_eq!(h.body, "");
        let h = parse_task_header("[task pri=0]    ").unwrap();
        assert_eq!(h.body, "");
    }

    // ---------------- format_task_description ----------------

    #[test]
    fn format_round_trips_with_parse() {
        let h = TaskHeader {
            priority: 5,
            due: Some(dt(2026, 6, 1, 10, 30)),
            body: "测试".to_string(),
        };
        let s = format_task_description(&h);
        let parsed = parse_task_header(&s).unwrap();
        assert_eq!(parsed, h);
    }

    #[test]
    fn format_omits_due_when_none() {
        let h = TaskHeader {
            priority: 0,
            due: None,
            body: "x".to_string(),
        };
        let s = format_task_description(&h);
        assert_eq!(s, "[task pri=0] x");
        assert!(!s.contains("due="));
    }

    // ---------------- classify_status ----------------

    #[test]
    fn classify_pending_when_no_markers() {
        let (s, m) = classify_status("[task pri=1] 整理文件");
        assert_eq!(s, TaskStatus::Pending);
        assert!(m.is_none());
    }

    #[test]
    fn classify_done_for_done_marker() {
        let (s, _) = classify_status("[task pri=1] 整理 [done]");
        assert_eq!(s, TaskStatus::Done);
    }

    #[test]
    fn classify_error_with_message() {
        let (s, m) = classify_status("[task pri=1] [error: 文件不存在] 复查");
        assert_eq!(s, TaskStatus::Error);
        assert_eq!(m.as_deref(), Some("文件不存在"));
    }

    #[test]
    fn classify_error_takes_precedence_over_done() {
        // 即使描述里也有 [done]，error 仍然优先 — 出错状态不该被 done 掩盖
        let (s, m) = classify_status("整理 [done] [error: 没权限]");
        assert_eq!(s, TaskStatus::Error);
        assert_eq!(m.as_deref(), Some("没权限"));
    }

    #[test]
    fn classify_cancelled_with_reason() {
        let (s, m) = classify_status("[task pri=1] x [cancelled: 不再需要]");
        assert_eq!(s, TaskStatus::Cancelled);
        assert_eq!(m.as_deref(), Some("不再需要"));
    }

    #[test]
    fn classify_cancelled_without_reason() {
        // [cancelled] 无副文案：仍判 Cancelled，但 reason = None
        let (s, m) = classify_status("[task pri=1] x [cancelled]");
        assert_eq!(s, TaskStatus::Cancelled);
        assert!(m.is_none());
    }

    #[test]
    fn classify_cancelled_takes_precedence_over_error() {
        // 用户的"取消"是终态，覆盖此前的失败状态
        let (s, m) = classify_status("整理 [error: 路径找不到] [cancelled: 不做了]");
        assert_eq!(s, TaskStatus::Cancelled);
        assert_eq!(m.as_deref(), Some("不做了"));
    }

    #[test]
    fn classify_cancelled_takes_precedence_over_done() {
        // 极少见：done + cancelled 共存。语义上"我说取消就取消"，覆盖 done
        let (s, _) = classify_status("整理 [done] [cancelled]");
        assert_eq!(s, TaskStatus::Cancelled);
    }

    #[test]
    fn classify_error_supports_chinese_colon() {
        let (s, m) = classify_status("[error：路径找不到]");
        assert_eq!(s, TaskStatus::Error);
        assert_eq!(m.as_deref(), Some("路径找不到"));
    }

    #[test]
    fn done_marker_must_be_token_not_substring() {
        // "我用 done 这个词描述任务" 不该被误判
        let (s, _) = classify_status("我用 done 形容这个任务");
        assert_eq!(s, TaskStatus::Pending);
    }

    // ---------------- strip_error_markers ----------------

    #[test]
    fn strip_clears_error_segment_and_keeps_header() {
        let cleaned = strip_error_markers("[task pri=2 due=2026-05-05T18:00] 整理 [error: 没权限] 复查");
        // task header 不动；error 段被剥；多余空白合并
        assert_eq!(
            cleaned,
            "[task pri=2 due=2026-05-05T18:00] 整理 复查"
        );
    }

    #[test]
    fn strip_clears_done_alongside_error() {
        // 重试时即便 LLM 误把它标了 done，也得复位
        let cleaned = strip_error_markers("[task pri=1] 整理 [done] [error: 文件不存在]");
        assert_eq!(cleaned, "[task pri=1] 整理");
    }

    #[test]
    fn strip_is_idempotent_on_clean_pending() {
        // 已是干净 pending 的 description 应保持不变（除空白合并）
        let cleaned = strip_error_markers("[task pri=1] 整理 Downloads");
        assert_eq!(cleaned, "[task pri=1] 整理 Downloads");
    }

    #[test]
    fn strip_handles_multiple_error_segments() {
        let cleaned = strip_error_markers("[error: 第一次失败] 进度 [error: 第二次失败]");
        assert_eq!(cleaned, "进度");
    }

    // ---------------- strip_for_clone ----------------

    #[test]
    fn strip_for_clone_removes_terminal_and_snooze_markers() {
        let cleaned = strip_for_clone(
            "[task pri=3] [every: 09:00] 整理 [done] [result: 50 files] [snooze: 2026-05-20 09:00] [archived: 2026-05-17]",
        );
        assert!(cleaned.contains("[task pri=3]"));
        assert!(cleaned.contains("[every: 09:00]"));
        assert!(cleaned.contains("整理"));
        assert!(!cleaned.contains("[done]"));
        assert!(!cleaned.contains("[result:"));
        assert!(!cleaned.contains("[snooze:"));
        assert!(!cleaned.contains("[archived:"));
    }

    #[test]
    fn strip_for_clone_keeps_owner_intent_markers() {
        let cleaned = strip_for_clone(
            "[task pri=3] [pinned] [silent] [blockedBy: A] [reminderMin: 5] #工作 做事 [done]",
        );
        assert!(cleaned.contains("[pinned]"));
        assert!(cleaned.contains("[silent]"));
        assert!(cleaned.contains("[blockedBy: A]"));
        assert!(cleaned.contains("[reminderMin: 5]"));
        assert!(cleaned.contains("#工作"));
        assert!(!cleaned.contains("[done]"));
    }

    #[test]
    fn strip_for_clone_idempotent_on_fresh_pending() {
        let cleaned = strip_for_clone("[task pri=2] [every: 工作日 09:00] 跑步");
        assert_eq!(cleaned, "[task pri=2] [every: 工作日 09:00] 跑步");
    }

    #[test]
    fn strip_for_clone_clears_error_for_cloned_task() {
        // 即便源 task 是 error 状态，clone 应该是 fresh — 剥 error reason
        let cleaned = strip_for_clone(
            "[task pri=3] 写报告 [error: API rate limit]",
        );
        assert_eq!(cleaned, "[task pri=3] 写报告");
    }

    // ---------------- strip_done_markers ----------------

    #[test]
    fn strip_done_clears_done_and_result() {
        let cleaned = strip_done_markers(
            "[task pri=1] 整理 Downloads [done] [result: 挪了 30 个文件]",
        );
        assert_eq!(cleaned, "[task pri=1] 整理 Downloads");
    }

    #[test]
    fn strip_done_keeps_owner_intent_markers() {
        // schedule / tag / pinned / silent / snooze / blockedBy 都保留
        let cleaned = strip_done_markers(
            "[task pri=3] [every: 09:00] [pinned] #工作 [snooze: 2026-05-20 09:00] [blockedBy: A] 整理 [done] [result: ok]",
        );
        // 仅 [done] + [result] 被剥；其它 owner-intent markers 全保留
        assert!(cleaned.contains("[task pri=3]"));
        assert!(cleaned.contains("[every: 09:00]"));
        assert!(cleaned.contains("[pinned]"));
        assert!(cleaned.contains("#工作"));
        assert!(cleaned.contains("[snooze: 2026-05-20 09:00]"));
        assert!(cleaned.contains("[blockedBy: A]"));
        assert!(cleaned.contains("整理"));
        assert!(!cleaned.contains("[done]"));
        assert!(!cleaned.contains("[result:"));
    }

    #[test]
    fn strip_done_is_idempotent_on_clean_pending() {
        let cleaned = strip_done_markers("[task pri=1] 整理 Downloads");
        assert_eq!(cleaned, "[task pri=1] 整理 Downloads");
    }

    #[test]
    fn strip_done_handles_multiple_result_segments() {
        // LLM 偶尔追加多次 result；都剥
        let cleaned = strip_done_markers(
            "整理 [result: 第一轮] 中间笔记 [done] [result: 第二轮 final]",
        );
        assert_eq!(cleaned, "整理 中间笔记");
    }

    // ---------------- strip_for_dup ----------------

    #[test]
    fn strip_for_dup_removes_terminal_state_markers() {
        let cleaned = strip_for_dup(
            "整理 Downloads [done] [result: 挪了 30 个文件]",
        );
        assert!(!cleaned.contains("[done]"), "should strip [done]: {cleaned}");
        assert!(!cleaned.contains("[result:"), "should strip [result:]: {cleaned}");
    }

    #[test]
    fn strip_for_dup_removes_snooze_and_origin() {
        let cleaned = strip_for_dup(
            "整理 [snooze: 2026-05-20 09:00] [origin:tg:12345] body",
        );
        assert!(!cleaned.contains("[snooze:"), "should strip snooze: {cleaned}");
        assert!(!cleaned.contains("[origin:tg"), "should strip tg origin: {cleaned}");
    }

    #[test]
    fn strip_for_dup_keeps_inheritable_markers() {
        let cleaned = strip_for_dup(
            "[every: 09:00] [reminderMin: 15] [pinned] [silent] [blockedBy: A] #工作 整理 [done]",
        );
        // 可继承的全保
        assert!(cleaned.contains("[every: 09:00]"), "{cleaned}");
        assert!(cleaned.contains("[reminderMin: 15]"), "{cleaned}");
        assert!(cleaned.contains("[pinned]"), "{cleaned}");
        assert!(cleaned.contains("[silent]"), "{cleaned}");
        assert!(cleaned.contains("[blockedBy: A]"), "{cleaned}");
        assert!(cleaned.contains("#工作"), "{cleaned}");
        assert!(cleaned.contains("整理"), "{cleaned}");
        // 终态 marker 剥
        assert!(!cleaned.contains("[done]"), "{cleaned}");
    }

    #[test]
    fn strip_for_dup_removes_error_and_cancelled_and_archived() {
        let cleaned = strip_for_dup(
            "整理 [error: timeout] [cancelled: 不需要了] [archived: 2026-04-01]",
        );
        assert!(!cleaned.contains("[error:"), "{cleaned}");
        assert!(!cleaned.contains("[cancelled:"), "{cleaned}");
        assert!(!cleaned.contains("[archived:"), "{cleaned}");
    }

    #[test]
    fn strip_for_dup_is_noop_on_clean_pending_body() {
        let raw = "[every: 09:00] [pinned] 写周报 #work";
        let cleaned = strip_for_dup(raw);
        assert_eq!(cleaned, raw);
    }

    // ---------------- append_cancelled_marker ----------------

    #[test]
    fn append_cancelled_with_reason_round_trips() {
        let appended = append_cancelled_marker("[task pri=1] 整理", "不需要了");
        assert_eq!(appended, "[task pri=1] 整理 [cancelled: 不需要了]");
        let (s, m) = classify_status(&appended);
        assert_eq!(s, TaskStatus::Cancelled);
        assert_eq!(m.as_deref(), Some("不需要了"));
    }

    #[test]
    fn append_cancelled_without_reason_uses_bare_marker() {
        let appended = append_cancelled_marker("[task pri=1] 整理", "  ");
        assert_eq!(appended, "[task pri=1] 整理 [cancelled]");
        let (s, m) = classify_status(&appended);
        assert_eq!(s, TaskStatus::Cancelled);
        assert!(m.is_none());
    }

    #[test]
    fn append_cancelled_to_empty_description() {
        let appended = append_cancelled_marker("", "x");
        assert_eq!(appended, "[cancelled: x]");
    }

    // ---------------- append_done_marker_with_result ----------------

    #[test]
    fn append_done_basic_appends_marker() {
        let appended = append_done_marker_with_result("[task pri=1] 整理", None);
        assert_eq!(appended, "[task pri=1] 整理 [done]");
        let (s, _) = classify_status(&appended);
        assert_eq!(s, TaskStatus::Done);
    }

    #[test]
    fn append_done_idempotent_when_already_done() {
        let original = "[task pri=1] 整理 [done]";
        assert_eq!(append_done_marker_with_result(original, None), original);
    }

    #[test]
    fn append_done_idempotent_with_result_marker() {
        // 含 [done] 即返回原串，不再追加；result 标记不影响判定。
        let original = "[task pri=1] 整理 [done] [result: 完成]";
        assert_eq!(append_done_marker_with_result(original, None), original);
    }

    #[test]
    fn append_done_to_empty_description() {
        assert_eq!(append_done_marker_with_result("", None), "[done]");
        assert_eq!(append_done_marker_with_result("   ", None), "[done]");
    }

    // ---------------- task origin ----------------

    #[test]
    fn parse_origin_extracts_telegram_chat_id() {
        let desc = "[task pri=1] 整理 [origin:tg:123456789]";
        assert_eq!(parse_task_origin(desc), Some(TaskOrigin::Tg(123456789)));
    }

    #[test]
    fn parse_origin_handles_negative_group_id() {
        // 群组 chat_id 在 teloxide 里是负数
        let desc = "[origin:tg:-1001234567890]";
        assert_eq!(parse_task_origin(desc), Some(TaskOrigin::Tg(-1001234567890)));
    }

    #[test]
    fn parse_origin_returns_none_when_absent() {
        assert_eq!(parse_task_origin("[task pri=1] 整理"), None);
    }

    #[test]
    fn parse_origin_returns_none_for_malformed_id() {
        assert_eq!(parse_task_origin("[origin:tg:not-a-number]"), None);
        assert_eq!(parse_task_origin("[origin:tg:]"), None);
    }

    #[test]
    fn append_origin_round_trips_with_parse() {
        let appended = append_origin_marker("[task pri=2] 跑步", &TaskOrigin::Tg(987654));
        assert_eq!(appended, "[task pri=2] 跑步 [origin:tg:987654]");
        assert_eq!(parse_task_origin(&appended), Some(TaskOrigin::Tg(987654)));
    }

    #[test]
    fn append_origin_idempotent_when_already_tagged() {
        // 反复 append 不会叠加多个 origin 段
        let once = append_origin_marker("[task pri=1] x", &TaskOrigin::Tg(42));
        let twice = append_origin_marker(&once, &TaskOrigin::Tg(42));
        let thrice = append_origin_marker(&twice, &TaskOrigin::Tg(42));
        assert_eq!(twice, once);
        assert_eq!(thrice, once);
    }

    #[test]
    fn append_origin_does_not_replace_existing_with_different_id() {
        // 已有 origin → 即便 id 不同也不替换 —— 防御性，避免后续误
        // 调用 swap origin（创建路径只调一次）
        let existing = "[task pri=1] x [origin:tg:1]";
        let attempted = append_origin_marker(existing, &TaskOrigin::Tg(2));
        assert_eq!(attempted, existing);
    }

    #[test]
    fn strip_origin_removes_marker_and_preserves_rest() {
        let desc = "[task pri=2] 整理 Downloads [origin:tg:999]";
        assert_eq!(strip_origin_marker(desc), "[task pri=2] 整理 Downloads");
    }

    #[test]
    fn strip_origin_is_noop_when_absent() {
        let desc = "[task pri=1] 整理 [error: 文件不存在]";
        assert_eq!(strip_origin_marker(desc), desc);
    }

    #[test]
    fn strip_origin_removes_multiple_markers() {
        // 防御性：理论上只该有一个，但脏数据 / 多次写入可能产生多个
        let desc = "x [origin:tg:1] y [origin:tg:2] z";
        assert_eq!(strip_origin_marker(desc), "x y z");
    }

    // ---------------- parse_task_tags ----------------

    #[test]
    fn parse_tags_extracts_ascii_and_chinese() {
        let tags = parse_task_tags("[task pri=2] 整理 Downloads #organize #文件整理 #weekly");
        assert_eq!(tags, vec!["organize", "文件整理", "weekly"]);
    }

    #[test]
    fn parse_tags_dedup_preserves_first_order() {
        let tags = parse_task_tags("#a #b #a #c #b");
        assert_eq!(tags, vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_tags_handles_underscore_and_dash() {
        let tags = parse_task_tags("#tech-debt #user_profile");
        assert_eq!(tags, vec!["tech-debt", "user_profile"]);
    }

    #[test]
    fn parse_tags_skips_lone_hash() {
        // 孤立的 # 或 # 后跟空白都不算 tag
        assert!(parse_task_tags("just # symbol").is_empty());
        assert!(parse_task_tags("hello # world").is_empty());
    }

    #[test]
    fn parse_tags_skips_hash_in_middle_of_word() {
        // "abc#def" 不视作 tag — # 紧贴标识符字符（如 PR 编号 #42 在英文
        // 句中）会引发误命中；要求 # 前不是 tag 字符
        assert_eq!(parse_task_tags("see PR#42 in #weekly notes"), vec!["weekly"]);
    }

    #[test]
    fn parse_tags_terminates_at_punctuation_and_brackets() {
        let tags = parse_task_tags("#a, #b. #c! #d ] #e");
        assert_eq!(tags, vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn parse_tags_returns_empty_for_no_tags() {
        assert!(parse_task_tags("[task pri=1] no tags here").is_empty());
        assert!(parse_task_tags("").is_empty());
    }

    // ---------------- parse_tag_ops ----------------

    #[test]
    fn parse_tag_ops_basic_add_remove() {
        let ops = parse_tag_ops("+a -b +工作").unwrap();
        assert_eq!(
            ops,
            vec![
                TagOp::Add("a".into()),
                TagOp::Remove("b".into()),
                TagOp::Add("工作".into()),
            ]
        );
    }

    #[test]
    fn parse_tag_ops_dedupes_repeated_op() {
        let ops = parse_tag_ops("+a +a -b -b").unwrap();
        assert_eq!(
            ops,
            vec![TagOp::Add("a".into()), TagOp::Remove("b".into())]
        );
    }

    #[test]
    fn parse_tag_ops_rejects_conflicting_signs() {
        assert!(parse_tag_ops("+a -a").is_err());
        assert!(parse_tag_ops("-x +x").is_err());
    }

    #[test]
    fn parse_tag_ops_rejects_missing_prefix_or_name() {
        assert!(parse_tag_ops("a").is_err()); // 缺前缀
        assert!(parse_tag_ops("+").is_err()); // 缺名
        assert!(parse_tag_ops("-").is_err());
    }

    #[test]
    fn parse_tag_ops_rejects_empty_input() {
        assert!(parse_tag_ops("").is_err());
        assert!(parse_tag_ops("   ").is_err());
    }

    #[test]
    fn parse_tag_ops_rejects_illegal_chars_in_name() {
        // 空格 / 标点等非 tag 字符
        assert!(parse_tag_ops("+a,b").is_err());
        assert!(parse_tag_ops("+a!b").is_err());
    }

    // ---------------- apply_tag_ops ----------------

    #[test]
    fn apply_tag_ops_add_appends_when_absent() {
        let out = apply_tag_ops("[task pri=2] 整理", &[TagOp::Add("organize".into())]);
        assert_eq!(out, "[task pri=2] 整理 #organize");
    }

    #[test]
    fn apply_tag_ops_add_noop_when_already_present() {
        let out = apply_tag_ops(
            "[task pri=2] 整理 #organize",
            &[TagOp::Add("organize".into())],
        );
        assert_eq!(out, "[task pri=2] 整理 #organize");
    }

    #[test]
    fn apply_tag_ops_remove_strips_token_and_leading_space() {
        let out = apply_tag_ops(
            "[task pri=1] 跑步 #weekly #fitness",
            &[TagOp::Remove("weekly".into())],
        );
        // 不该出现双空格
        assert!(!out.contains("  "));
        assert!(!parse_task_tags(&out).iter().any(|t| t == "weekly"));
        assert!(parse_task_tags(&out).iter().any(|t| t == "fitness"));
    }

    #[test]
    fn apply_tag_ops_remove_nonexistent_is_noop() {
        let out = apply_tag_ops("[task pri=1] x #a", &[TagOp::Remove("nonexistent".into())]);
        assert_eq!(parse_task_tags(&out), vec!["a"]);
    }

    #[test]
    fn apply_tag_ops_does_not_strip_substring_match() {
        // remove "tag" 不该误删 #tagged
        let out = apply_tag_ops(
            "[task pri=1] x #tag #tagged",
            &[TagOp::Remove("tag".into())],
        );
        assert_eq!(parse_task_tags(&out), vec!["tagged"]);
    }

    #[test]
    fn apply_tag_ops_chains_multiple_ops() {
        let out = apply_tag_ops(
            "[task pri=1] x #a #b",
            &[
                TagOp::Remove("a".into()),
                TagOp::Add("c".into()),
                TagOp::Add("b".into()), // 已存在 → noop
            ],
        );
        assert_eq!(parse_task_tags(&out), vec!["b", "c"]);
    }

    // ---------------- parse_task_result ----------------

    #[test]
    fn parse_result_extracts_text_after_colon() {
        let r = parse_task_result("[task pri=1] 整理 [done] [result: 把 38 个文件归档到 ~/Archive/]");
        assert_eq!(r.as_deref(), Some("把 38 个文件归档到 ~/Archive/"));
    }

    #[test]
    fn parse_result_supports_chinese_colon() {
        let r = parse_task_result("[result：完成]");
        assert_eq!(r.as_deref(), Some("完成"));
    }

    #[test]
    fn parse_result_returns_none_when_absent() {
        assert!(parse_task_result("[task pri=1] 整理 [done]").is_none());
    }

    #[test]
    fn parse_result_returns_none_when_empty() {
        // [result:] 空内容视作无产物 — 给 LLM 留容错空间
        assert!(parse_task_result("[result:]").is_none());
        assert!(parse_task_result("[result: ]").is_none());
        assert!(parse_task_result("[result:    ]").is_none());
    }

    #[test]
    fn parse_result_takes_first_when_multiple() {
        // 脏数据兜底：取首个，不合并
        let r = parse_task_result("[result: 一] [result: 二]");
        assert_eq!(r.as_deref(), Some("一"));
    }

    // ---------------- strip_result_marker ----------------

    #[test]
    fn strip_result_removes_marker_and_keeps_rest() {
        let desc = "[task pri=2] 整理 [done] [result: 完成]";
        assert_eq!(strip_result_marker(desc), "[task pri=2] 整理 [done]");
    }

    #[test]
    fn strip_result_is_noop_when_absent() {
        let desc = "[task pri=1] 整理 #organize";
        assert_eq!(strip_result_marker(desc), desc);
    }

    // ---------------- compare_for_queue ----------------

    #[test]
    fn cancelled_sorts_after_done() {
        // 结束段内 done 优于 cancelled — 用户开「显示已结束」时希望先看到完成的
        let now = dt(2026, 5, 4, 12, 0);
        let done = view("d", 9, None, TaskStatus::Done, "2026-05-01T00:00");
        let cancelled = view("c", 9, None, TaskStatus::Cancelled, "2026-05-01T00:00");
        assert_eq!(compare_for_queue(&done, &cancelled, now), Ordering::Less);
    }

    #[test]
    fn cancelled_sorts_after_pending_even_with_overdue() {
        // cancelled 是终态，永远不与活动段争位 —— 即便它"过期"也排到最后
        let now = dt(2026, 5, 4, 12, 0);
        let cancelled_overdue = view(
            "c",
            9,
            Some("2026-05-03T08:00"),
            TaskStatus::Cancelled,
            "2026-05-01T00:00",
        );
        let pending_no_due = view("p", 0, None, TaskStatus::Pending, "2026-05-01T00:00");
        assert_eq!(
            compare_for_queue(&pending_no_due, &cancelled_overdue, now),
            Ordering::Less
        );
    }

    #[test]
    fn error_outranks_pending_outranks_done() {
        let now = dt(2026, 5, 4, 12, 0);
        let err = view("e", 0, None, TaskStatus::Error, "2026-05-01T00:00");
        let pen = view("p", 9, None, TaskStatus::Pending, "2026-05-01T00:00");
        let done = view("d", 9, None, TaskStatus::Done, "2026-05-01T00:00");
        assert_eq!(compare_for_queue(&err, &pen, now), Ordering::Less);
        assert_eq!(compare_for_queue(&pen, &done, now), Ordering::Less);
        // 哪怕 pri 与 due 都更友好，done 也不能挤到 pending 前
        let done_high = view(
            "d",
            9,
            Some("2026-05-04T11:00"),
            TaskStatus::Done,
            "2026-05-01T00:00",
        );
        assert_eq!(compare_for_queue(&pen, &done_high, now), Ordering::Less);
    }

    #[test]
    fn overdue_pending_outranks_future_pending_even_with_lower_priority() {
        let now = dt(2026, 5, 4, 12, 0);
        let overdue_low = view(
            "overdue-low",
            1,
            Some("2026-05-04T08:00"),
            TaskStatus::Pending,
            "2026-05-01T00:00",
        );
        let future_high = view(
            "future-hi",
            9,
            Some("2026-05-05T08:00"),
            TaskStatus::Pending,
            "2026-05-01T00:00",
        );
        assert_eq!(
            compare_for_queue(&overdue_low, &future_high, now),
            Ordering::Less
        );
    }

    #[test]
    fn among_overdue_earlier_due_first() {
        let now = dt(2026, 5, 4, 12, 0);
        let a = view(
            "a",
            0,
            Some("2026-05-03T10:00"),
            TaskStatus::Pending,
            "2026-05-02T00:00",
        );
        let b = view(
            "b",
            0,
            Some("2026-05-04T10:00"),
            TaskStatus::Pending,
            "2026-05-02T00:00",
        );
        // a 过期更久 → 排前
        assert_eq!(compare_for_queue(&a, &b, now), Ordering::Less);
    }

    #[test]
    fn higher_priority_wins_among_non_overdue_pending() {
        let now = dt(2026, 5, 4, 12, 0);
        let a = view("a", 5, None, TaskStatus::Pending, "2026-05-01T00:00");
        let b = view("b", 1, None, TaskStatus::Pending, "2026-05-01T00:00");
        assert_eq!(compare_for_queue(&a, &b, now), Ordering::Less);
    }

    #[test]
    fn earlier_due_wins_when_priority_tied_and_not_overdue() {
        let now = dt(2026, 5, 4, 12, 0);
        let a = view(
            "a",
            3,
            Some("2026-05-05T08:00"),
            TaskStatus::Pending,
            "2026-05-01T00:00",
        );
        let b = view(
            "b",
            3,
            Some("2026-05-06T08:00"),
            TaskStatus::Pending,
            "2026-05-01T00:00",
        );
        assert_eq!(compare_for_queue(&a, &b, now), Ordering::Less);
    }

    #[test]
    fn task_with_due_outranks_dueless_at_same_priority() {
        let now = dt(2026, 5, 4, 12, 0);
        let a = view(
            "a",
            2,
            Some("2026-05-10T08:00"),
            TaskStatus::Pending,
            "2026-05-01T00:00",
        );
        let b = view("b", 2, None, TaskStatus::Pending, "2026-05-01T00:00");
        assert_eq!(compare_for_queue(&a, &b, now), Ordering::Less);
    }

    #[test]
    fn created_at_breaks_remaining_ties() {
        let now = dt(2026, 5, 4, 12, 0);
        let older = view("old", 3, None, TaskStatus::Pending, "2026-05-01T00:00");
        let newer = view("new", 3, None, TaskStatus::Pending, "2026-05-03T00:00");
        // 同 pri + 同 due（None） + 同 status → 老任务优先（避免饿死）
        assert_eq!(compare_for_queue(&older, &newer, now), Ordering::Less);
    }

    // ---------------- parse_blocked_by ----------------

    #[test]
    fn parse_blocked_by_basic() {
        let v = parse_blocked_by("[blockedBy: 整理 Downloads, 写日报] 真正要做的事");
        assert_eq!(v, vec!["整理 Downloads", "写日报"]);
    }

    #[test]
    fn parse_blocked_by_no_marker() {
        assert!(parse_blocked_by("普通任务").is_empty());
        assert!(parse_blocked_by("[task pri=3] 普通任务").is_empty());
    }

    #[test]
    fn parse_blocked_by_dedup_and_order() {
        let v = parse_blocked_by("[blockedBy: A, B] 主体 [blockedBy: B, C, A]");
        // 首次出现顺序 + 去重
        assert_eq!(v, vec!["A", "B", "C"]);
    }

    #[test]
    fn parse_blocked_by_trims_pieces() {
        let v = parse_blocked_by("[blockedBy:  A ,  ,  B   ]");
        assert_eq!(v, vec!["A", "B"]);
    }

    #[test]
    fn parse_blocked_by_ignores_unclosed_marker() {
        // 没闭合 `]` —— 当前实现遇到这种情况停止扫描，整段不返。
        let v = parse_blocked_by("[blockedBy: A, B 没闭合");
        assert!(v.is_empty());
    }

    #[test]
    fn parse_blocked_by_case_sensitive_key() {
        // 不接受小写 `blockedby` —— 与其它 marker 大小写敏感一致避免误匹配。
        let v = parse_blocked_by("[blockedby: A]");
        assert!(v.is_empty());
    }

    // ---------------- unresolved_blockers ----------------

    #[test]
    fn unresolved_blockers_filters_done_and_cancelled() {
        let items = vec![
            ("done-task".to_string(), "[task pri=3] 做完了 [done]".to_string()),
            (
                "cancel-task".to_string(),
                "[task pri=3] 已取消 [cancelled: 改主意]".to_string(),
            ),
            (
                "active-blocker".to_string(),
                "[task pri=3] 还没做".to_string(),
            ),
            (
                "blocked-task".to_string(),
                "[blockedBy: done-task, cancel-task, active-blocker] 主任务".to_string(),
            ),
        ];
        let map = unresolved_blockers(&items);
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get("blocked-task"),
            Some(&vec!["active-blocker".to_string()]),
            "done/cancelled blockers 已解决；active 仍卡住"
        );
    }

    #[test]
    fn unresolved_blockers_typo_blocker_treated_as_resolved() {
        // blocker 引用了不存在的 title（typo / 被删 / 被改名）→ 视作已解决。
        let items = vec![
            (
                "real".to_string(),
                "[blockedBy: 不存在的任务] 主任务".to_string(),
            ),
            ("real-2".to_string(), "[task pri=3] 不相关".to_string()),
        ];
        let map = unresolved_blockers(&items);
        assert!(map.is_empty(), "不存在的 blocker 不应卡住任务");
    }

    #[test]
    fn unresolved_blockers_no_marker_no_entry() {
        let items = vec![("a".to_string(), "[task pri=3] 没依赖".to_string())];
        let map = unresolved_blockers(&items);
        assert!(map.is_empty());
    }

    #[test]
    fn unresolved_blockers_error_state_is_still_active() {
        // error 状态的 blocker 仍算 active（用户没决定重试 / 取消，悬而未决）。
        let items = vec![
            (
                "err".to_string(),
                "[task pri=3] [error: 没网] 出错了".to_string(),
            ),
            (
                "blocked".to_string(),
                "[blockedBy: err] 主任务".to_string(),
            ),
        ];
        let map = unresolved_blockers(&items);
        assert_eq!(
            map.get("blocked"),
            Some(&vec!["err".to_string()]),
            "error 状态阻塞仍有效"
        );
    }

    // ---------------- parse_snooze ----------------

    fn ndt(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
    }

    #[test]
    fn parse_snooze_basic() {
        let v = parse_snooze("[snooze: 2026-05-20 09:00] 主任务");
        assert_eq!(v, Some(ndt(2026, 5, 20, 9, 0)));
    }

    #[test]
    fn parse_snooze_no_marker() {
        assert!(parse_snooze("普通任务").is_none());
        assert!(parse_snooze("[task pri=3] 普通任务").is_none());
    }

    #[test]
    fn parse_snooze_takes_latest_when_multiple() {
        // 用户重新 snooze 时可以 append 一个新 marker，不必删旧；解析器
        // 取最后一个有效值。
        let v = parse_snooze(
            "[snooze: 2026-05-15 09:00] 主任务 [snooze: 2026-05-20 09:00] 又延后",
        );
        assert_eq!(v, Some(ndt(2026, 5, 20, 9, 0)));
    }

    #[test]
    fn parse_snooze_invalid_format_ignored() {
        // chrono parse_from_str 对 "%Y-%m-%d %H:%M" 字段值非法 / 完全乱码
        // 都返 Err。注意：chrono 对零填充是宽松的（`2026-5-1 9:00` 也 accept），
        // 所以这里只测字段值越界 + 完全非时间串。
        assert!(parse_snooze("[snooze: 2026-13-99 25:99]").is_none());
        assert!(parse_snooze("[snooze: not-a-time]").is_none());
        assert!(parse_snooze("[snooze: ]").is_none());
        assert!(parse_snooze("[snooze:]").is_none());
    }

    #[test]
    fn parse_snooze_invalid_then_valid_keeps_valid() {
        // 第一个 marker 烂，第二个 marker 好 → 取后者；不应被烂条卡死。
        let v = parse_snooze(
            "[snooze: bad] [snooze: 2026-05-20 09:00]",
        );
        assert_eq!(v, Some(ndt(2026, 5, 20, 9, 0)));
    }

    #[test]
    fn parse_snooze_case_sensitive_key() {
        // 不接受 [SNOOZE: ...] / [Snooze: ...] —— 大小写敏感对齐其它 marker。
        assert!(parse_snooze("[SNOOZE: 2026-05-20 09:00]").is_none());
        assert!(parse_snooze("[Snooze: 2026-05-20 09:00]").is_none());
    }

    #[test]
    fn parse_snooze_unclosed_marker() {
        let v = parse_snooze("[snooze: 2026-05-20 09:00 没闭合");
        assert!(v.is_none());
    }

    // ---------------- snoozed_until_map ----------------

    #[test]
    fn snoozed_until_map_filters_past_snooze() {
        let now = ndt(2026, 5, 14, 12, 0);
        let items = vec![
            (
                "future".to_string(),
                "[snooze: 2026-05-20 09:00] 还没醒".to_string(),
            ),
            (
                "past".to_string(),
                "[snooze: 2026-05-10 09:00] 已经醒过".to_string(),
            ),
            ("none".to_string(), "[task pri=3] 无 snooze".to_string()),
        ];
        let m = snoozed_until_map(&items, now);
        assert_eq!(m.len(), 1, "{:?}", m);
        assert_eq!(m.get("future"), Some(&ndt(2026, 5, 20, 9, 0)));
        assert!(!m.contains_key("past"), "past snooze 已 expired");
        assert!(!m.contains_key("none"));
    }

    // ---------------- strip_snooze_markers ----------------

    #[test]
    fn strip_snooze_markers_basic() {
        let s = strip_snooze_markers("主任务 [snooze: 2026-05-20 09:00] 末尾");
        assert_eq!(s, "主任务 末尾", "{}", s);
    }

    #[test]
    fn strip_snooze_markers_multiple() {
        let s = strip_snooze_markers(
            "[snooze: 2026-05-10 09:00] 头部 [snooze: 2026-05-20 09:00] 尾部",
        );
        assert_eq!(s, "头部 尾部", "{}", s);
    }

    #[test]
    fn strip_snooze_markers_no_marker_noop() {
        let s = strip_snooze_markers("[task pri=3] 普通任务 #tag");
        assert_eq!(s, "[task pri=3] 普通任务 #tag");
    }

    #[test]
    fn strip_snooze_markers_unclosed_marker_preserved() {
        // 未闭合：保留原样，避免静默删除合法但 typo 的字面量
        let s = strip_snooze_markers("[snooze: 2026-05-20 09:00 没闭合");
        assert_eq!(s, "[snooze: 2026-05-20 09:00 没闭合");
    }

    #[test]
    fn strip_snooze_markers_normalizes_whitespace() {
        // marker 两侧多个空格 → 合并到单空格
        let s = strip_snooze_markers("a   [snooze: 2026-05-20 09:00]   b");
        assert_eq!(s, "a b", "{}", s);
    }

    #[test]
    fn strip_snooze_markers_leading_marker() {
        // 行首 marker：剥后无前缀空白
        let s = strip_snooze_markers("[snooze: 2026-05-20 09:00] 主体");
        assert_eq!(s, "主体");
    }

    #[test]
    fn strip_snooze_markers_trailing_marker() {
        // 行尾 marker：剥后无尾空白
        let s = strip_snooze_markers("主体 [snooze: 2026-05-20 09:00]");
        assert_eq!(s, "主体");
    }

    #[test]
    fn snoozed_until_map_boundary_now_equals_wake_is_awake() {
        // now == snooze 时刻：用户该被唤醒。`>` 严格 future 才算 snoozed。
        let now = ndt(2026, 5, 20, 9, 0);
        let items = vec![(
            "boundary".to_string(),
            "[snooze: 2026-05-20 09:00] 边界".to_string(),
        )];
        let m = snoozed_until_map(&items, now);
        assert!(m.is_empty(), "now == wake 不再算 snooze");
    }

    // ---------------- parse_pinned / strip_pinned_markers ----------------

    #[test]
    fn parse_pinned_matches_strict_form() {
        assert!(parse_pinned("[task pri=3] 主任务 [pinned]"));
        assert!(parse_pinned("[pinned] 行首也算"));
        assert!(parse_pinned("中间 [pinned] 也算"));
    }

    #[test]
    fn parse_pinned_rejects_variants() {
        // 严格匹配：大写 / 加载荷 / 拼写错 都不算。owner 写 `[pinned]` 才生效。
        assert!(!parse_pinned("[Pinned]"));
        assert!(!parse_pinned("[PINNED]"));
        assert!(!parse_pinned("[pinned: foo]"));
        assert!(!parse_pinned("[pin]"));
        assert!(!parse_pinned("普通任务"));
    }

    #[test]
    fn strip_pinned_markers_removes_and_normalizes() {
        // 单次：normal whitespace 合并；多次：全部剥；未匹配：noop。
        assert_eq!(strip_pinned_markers("主任务 [pinned]"), "主任务");
        assert_eq!(strip_pinned_markers("[pinned] 主任务"), "主任务");
        assert_eq!(strip_pinned_markers("a [pinned] b [pinned] c"), "a b c");
        assert_eq!(strip_pinned_markers("无 marker"), "无 marker");
    }

    #[test]
    fn strip_pinned_markers_preserves_other_markers() {
        // 关键回归：只剥 [pinned]，不动 [task pri=3] / [snooze:] / [origin:tg:]
        let s = strip_pinned_markers(
            "[task pri=3 due=2026-05-20T18:00] 主任务 [pinned] [snooze: 2026-05-20 09:00] [origin:tg:123]",
        );
        assert!(s.contains("[task pri=3 due=2026-05-20T18:00]"));
        assert!(s.contains("[snooze: 2026-05-20 09:00]"));
        assert!(s.contains("[origin:tg:123]"));
        assert!(!s.contains("[pinned]"));
    }

    // ---------------- parse_silent / strip_silent_markers ----------------

    #[test]
    fn parse_silent_strict_literal() {
        assert!(parse_silent("[silent]"));
        assert!(parse_silent("整理 Downloads [silent]"));
        assert!(parse_silent("[silent] 主任务"));
        assert!(!parse_silent("[Silent]"), "大小写敏感");
        assert!(!parse_silent("[silent: reason]"), "拒绝带 reason 变体");
        assert!(!parse_silent(""), "空 description false");
    }

    #[test]
    fn strip_silent_markers_removes_and_normalizes() {
        assert_eq!(strip_silent_markers("主任务 [silent]"), "主任务");
        assert_eq!(strip_silent_markers("[silent] 主任务"), "主任务");
        assert_eq!(strip_silent_markers("a [silent] b [silent] c"), "a b c");
        assert_eq!(strip_silent_markers("无 marker"), "无 marker");
    }

    #[test]
    fn strip_silent_markers_preserves_other_markers() {
        // 关键回归：只剥 [silent]，不动 [task pri=3] / [pinned] / [snooze:] / [origin:tg:]
        let s = strip_silent_markers(
            "[task pri=3 due=2026-05-20T18:00] 主任务 [silent] [pinned] [snooze: 2026-05-20 09:00] [origin:tg:123]",
        );
        assert!(s.contains("[task pri=3 due=2026-05-20T18:00]"));
        assert!(s.contains("[pinned]"));
        assert!(s.contains("[snooze: 2026-05-20 09:00]"));
        assert!(s.contains("[origin:tg:123]"));
        assert!(!s.contains("[silent]"));
    }
