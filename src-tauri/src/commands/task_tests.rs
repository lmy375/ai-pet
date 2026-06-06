    use super::*;
    use crate::commands::memory::MemoryItem;
    use crate::task_queue::TaskStatus;

    fn item(title: &str, desc: &str, created_at: &str) -> MemoryItem {
        MemoryItem {
            title: title.to_string(),
            description: desc.to_string(),
            detail_path: String::new(),
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
        }
    }

    #[test]
    fn build_view_extracts_header_fields() {
        let v = build_task_view(&item(
            "归档下载",
            "[task pri=3 due=2026-05-05T18:00] 把图片按月份归档",
            "2026-05-04T13:00:00+08:00",
        ));
        assert_eq!(v.priority, 3);
        assert_eq!(v.due.as_deref(), Some("2026-05-05T18:00"));
        assert_eq!(v.body, "把图片按月份归档");
        assert_eq!(v.status, TaskStatus::Pending);
    }

    #[test]
    fn build_view_falls_back_for_legacy_entries() {
        // 历史条目没有 task header，但带 [once:] 等老前缀 — 也能展示，
        // 只是 priority 默认 0、due None。这是兼容性保证，不能丢老数据。
        let v = build_task_view(&item(
            "晚上跑步",
            "[once: 2026-05-04T19:00] 跑 30 分钟",
            "2026-05-04T13:00:00+08:00",
        ));
        assert_eq!(v.priority, 0);
        assert_eq!(v.due, None);
        assert_eq!(v.body, "[once: 2026-05-04T19:00] 跑 30 分钟");
        assert_eq!(v.status, TaskStatus::Pending);
    }

    #[test]
    fn build_view_collects_tags_and_result() {
        let v = build_task_view(&item(
            "整理 Downloads",
            "[task pri=2] 把图片归档 #organize #weekly [done] [result: 归档 38 个文件到 ~/Archive/]",
            "2026-05-04T13:00:00+08:00",
        ));
        assert_eq!(v.tags, vec!["organize", "weekly"]);
        assert_eq!(v.result.as_deref(), Some("归档 38 个文件到 ~/Archive/"));
        assert_eq!(v.status, TaskStatus::Done);
        // body 不应该包含 [result:...] 段（已在 result 字段独立展示）
        assert!(!v.body.contains("[result"));
        // body 应保留 #tag 让用户在描述里也看到
        assert!(v.body.contains("#organize"));
    }

    #[test]
    fn build_view_handles_missing_tags_and_result() {
        let v = build_task_view(&item(
            "x",
            "[task pri=1] 普通任务",
            "2026-05-04T13:00:00+08:00",
        ));
        assert!(v.tags.is_empty());
        assert!(v.result.is_none());
    }

    #[test]
    fn build_view_hides_origin_marker_from_displayed_body() {
        // origin 标记是宠物侧 routing 协议，面板上的 body 不应该带它
        let v = build_task_view(&item(
            "整理 Downloads",
            "[task pri=2] 把图片归档 [origin:tg:12345]",
            "2026-05-04T13:00:00+08:00",
        ));
        assert_eq!(v.body, "把图片归档");
    }

    #[test]
    fn build_view_picks_up_error_status_with_message() {
        let v = build_task_view(&item(
            "整理 Downloads",
            "[task pri=2] [error: 路径找不到] 复查",
            "2026-05-04T13:00:00+08:00",
        ));
        assert_eq!(v.status, TaskStatus::Error);
        assert_eq!(v.error_message.as_deref(), Some("路径找不到"));
    }

    // ---------------- task_set_priority validation ----------------

    #[test]
    fn set_priority_rejects_empty_title() {
        // 校验路径不依赖 memory IO —— 只测早退出分支
        let r = task_set_priority(String::new(), 3);
        assert!(r.is_err());
        let r = task_set_priority("   ".to_string(), 3);
        assert!(r.is_err());
    }

    #[test]
    fn set_priority_rejects_out_of_range() {
        let r = task_set_priority("any".to_string(), 10);
        assert!(r.is_err());
        let err = r.unwrap_err();
        assert!(err.contains("priority must be 0..=9"));
    }

    // ---------------- task_set_due validation ----------------

    #[test]
    fn set_due_rejects_empty_title() {
        // 空 title 应在 IO 之前早退；不依赖 memory mock
        assert!(task_set_due(String::new(), Some("2026-05-05T18:00".to_string())).is_err());
        assert!(task_set_due("   ".to_string(), None).is_err());
    }

    #[test]
    fn set_due_rejects_invalid_format() {
        // 任意非空 title + 不符合 datetime-local 协议的 due 也应早退
        let r = task_set_due("any".to_string(), Some("2026/05/05 18:00".to_string()));
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("invalid due"));
    }

    // ---------------- task_save_detail validation ----------------

    #[test]
    fn save_detail_rejects_empty_title() {
        // 空 title 早退于 IO 之前，不需 memory mock
        assert!(task_save_detail(String::new(), "any content".to_string()).is_err());
        assert!(task_save_detail("   ".to_string(), String::new()).is_err());
    }

    // ---------------- count_overdue ----------------

    fn now() -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 5, 5)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
    }

    #[test]
    fn count_overdue_returns_zero_for_empty() {
        assert_eq!(count_overdue(&[], now()), 0);
    }

    #[test]
    fn count_overdue_includes_pending_with_passed_due() {
        let items = vec![item(
            "整理 Downloads",
            "[task pri=2 due=2026-05-04T18:00] 整理",
            "2026-05-04T13:00:00+08:00",
        )];
        assert_eq!(count_overdue(&items, now()), 1);
    }

    #[test]
    fn count_overdue_includes_error_with_passed_due() {
        let items = vec![item(
            "跑步",
            "[task pri=1 due=2026-05-04T18:00] [error: 下雨] 跑步",
            "2026-05-04T13:00:00+08:00",
        )];
        assert_eq!(count_overdue(&items, now()), 1);
    }

    #[test]
    fn count_overdue_excludes_done_and_cancelled() {
        // 终态任务即便 due 已过也不入计数 —— 用户已经 move on
        let items = vec![
            item(
                "已做完",
                "[task pri=2 due=2026-05-04T18:00] 整理 [done]",
                "2026-05-04T13:00:00+08:00",
            ),
            item(
                "已取消",
                "[task pri=2 due=2026-05-04T18:00] 整理 [cancelled: 不做了]",
                "2026-05-04T13:00:00+08:00",
            ),
        ];
        assert_eq!(count_overdue(&items, now()), 0);
    }

    #[test]
    fn count_overdue_excludes_future_due() {
        let items = vec![item(
            "未来任务",
            "[task pri=1 due=2026-05-06T18:00] 还没到",
            "2026-05-04T13:00:00+08:00",
        )];
        assert_eq!(count_overdue(&items, now()), 0);
    }

    #[test]
    fn count_overdue_excludes_no_due() {
        // 无 due 字段 — "过期"概念不适用
        let items = vec![item(
            "无截止",
            "[task pri=3] 闲时做",
            "2026-05-04T13:00:00+08:00",
        )];
        assert_eq!(count_overdue(&items, now()), 0);
    }

    #[test]
    fn count_overdue_excludes_legacy_no_header() {
        // 旧格式（无 [task pri=...] header）→ parse 失败 → 视作无 due → 不计入
        let items = vec![item(
            "旧任务",
            "[once: 2026-05-04T18:00] 跑步",
            "2026-05-04T13:00:00+08:00",
        )];
        assert_eq!(count_overdue(&items, now()), 0);
    }

    #[test]
    fn count_overdue_treats_due_eq_now_as_overdue() {
        // 边界一致性：`due == now` 与 compare_for_queue 都判 overdue
        let items = vec![item(
            "刚好到点",
            "[task pri=2 due=2026-05-05T12:00] 该开始了",
            "2026-05-04T13:00:00+08:00",
        )];
        assert_eq!(count_overdue(&items, now()), 1);
    }

    #[test]
    fn count_overdue_aggregates_multiple() {
        let items = vec![
            item("过期1", "[task pri=2 due=2026-05-04T10:00] a", "2026-05-04T09:00:00+08:00"),
            item("过期2", "[task pri=1 due=2026-05-05T11:30] [error: x] b", "2026-05-04T09:00:00+08:00"),
            item("未过期", "[task pri=3 due=2026-05-06T10:00] c", "2026-05-04T09:00:00+08:00"),
            item("已完成", "[task pri=1 due=2026-05-04T08:00] d [done]", "2026-05-04T09:00:00+08:00"),
            item("无 due", "[task pri=2] e", "2026-05-04T09:00:00+08:00"),
        ];
        assert_eq!(count_overdue(&items, now()), 2);
    }
