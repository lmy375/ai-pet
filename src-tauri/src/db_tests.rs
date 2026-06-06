    use super::*;

    /// 启动 + migration 必须可重入。第一次建表，第二次 noop。
    #[test]
    fn migrations_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();
        apply_migrations(&conn).unwrap();
        // 验证 butler_tasks 表存在
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='butler_tasks'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "butler_tasks table should exist after migration");
        // todo table 也应建好（v2 migration）
        let todo_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='todo'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(todo_count, 1, "todo table should exist after migration");
        let archive_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='task_archive'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(archive_count, 1, "task_archive table should exist after migration");
        let kv_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='kv_state'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kv_count, 1, "kv_state table should exist after migration");
        // _migrations table 每个版本一行（v1 + v2 + v3 + v4 = 4；noop 第二次不重复）
        let mig_count: i32 = conn
            .query_row("SELECT COUNT(*) FROM _migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mig_count, 4, "four migrations applied, noop second call");
    }

    #[test]
    fn kv_state_upsert_and_delete() {
        // kv_get / kv_set / kv_delete 用全局 `with_db`，所以这里不能用
        // fresh_conn —— 直接走全局连接做 round-trip。pet.db 是用户配置
        // 目录下的实际文件；CI / 单机测试都能写。end 时清掉避免污染。
        let key = "_test_kv_state_upsert";
        crate::db::kv_set(key, "first");
        assert_eq!(crate::db::kv_get(key).as_deref(), Some("first"));
        crate::db::kv_set(key, "second");
        assert_eq!(crate::db::kv_get(key).as_deref(), Some("second"));
        crate::db::kv_delete(key);
        assert_eq!(crate::db::kv_get(key), None);
    }

    #[test]
    fn task_archive_crud_roundtrip() {
        let conn = fresh_conn();
        let row = TaskArchiveRow {
            title: "2026-04-01_整理 Downloads".to_string(),
            description: "[archived: 2026-04-01] [task pri=3] 整理 [done]".to_string(),
            status: "archived".to_string(),
            detail_path: Some("task_archive/2026_04_01_zheng_li.md".to_string()),
            tags: vec![],
            created_at: "".to_string(),
            updated_at: "".to_string(),
        };
        task_archive_create(&conn, &row).unwrap();
        let fetched = task_archive_get(&conn, "2026-04-01_整理 Downloads")
            .unwrap()
            .unwrap();
        assert_eq!(fetched.status, "archived");
        let deleted = task_archive_delete(&conn, "2026-04-01_整理 Downloads").unwrap();
        assert!(deleted);
    }

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn crud_roundtrip() {
        let conn = fresh_conn();
        let row = ButlerTaskRow {
            title: "整理 Downloads".to_string(),
            description: "[once: 2026-05-12] 把过期截图归类".to_string(),
            status: "pending".to_string(),
            detail_path: Some("butler_tasks/zheng_li_downloads.md".to_string()),
            tags: vec!["生活".to_string(), "整理".to_string()],
            created_at: "".to_string(),
            updated_at: "".to_string(),
        };
        butler_task_create(&conn, &row).unwrap();
        let fetched = butler_task_get(&conn, "整理 Downloads").unwrap().unwrap();
        assert_eq!(fetched.title, row.title);
        assert_eq!(fetched.description, row.description);
        assert_eq!(fetched.status, "pending");
        assert_eq!(fetched.tags, vec!["生活", "整理"]);
        assert!(!fetched.created_at.is_empty(), "created_at auto-filled");
        assert!(!fetched.updated_at.is_empty(), "updated_at auto-filled");

        let updated = butler_task_update(
            &conn,
            "整理 Downloads",
            "[once: 2026-05-12] [done] 完成",
            "done",
            None,
            &["生活".to_string()],
        )
        .unwrap();
        assert!(updated);
        let after = butler_task_get(&conn, "整理 Downloads").unwrap().unwrap();
        assert_eq!(after.status, "done");
        assert_eq!(after.detail_path, None);
        assert_eq!(after.tags, vec!["生活"]);

        let deleted = butler_task_delete(&conn, "整理 Downloads").unwrap();
        assert!(deleted);
        assert!(butler_task_get(&conn, "整理 Downloads").unwrap().is_none());
    }

    #[test]
    fn create_unique_title() {
        let conn = fresh_conn();
        let row = ButlerTaskRow {
            title: "唯一".to_string(),
            description: "first".to_string(),
            status: "pending".to_string(),
            detail_path: None,
            tags: vec![],
            created_at: "".to_string(),
            updated_at: "".to_string(),
        };
        butler_task_create(&conn, &row).unwrap();
        let err = butler_task_create(&conn, &row);
        assert!(err.is_err(), "duplicate title should be rejected by UNIQUE");
    }

    #[test]
    fn update_missing_title_returns_false() {
        let conn = fresh_conn();
        let updated =
            butler_task_update(&conn, "不存在", "desc", "pending", None, &[]).unwrap();
        assert!(!updated);
    }

    #[test]
    fn backfill_derives_status_and_tags() {
        use crate::commands::memory::MemoryItem;
        let conn = fresh_conn();
        let items = vec![
            MemoryItem {
                title: "做完的事".to_string(),
                description: "[task pri=3] 整理 #生活 [done] [result: 38 文件归档]".to_string(),
                detail_path: "butler_tasks/zuo_wan_de_shi.md".to_string(),
                created_at: "2026-05-01T00:00:00+08:00".to_string(),
                updated_at: "2026-05-02T00:00:00+08:00".to_string(),
            },
            MemoryItem {
                title: "失败的事".to_string(),
                description: "[task pri=5] 写日报 #工作 [error: 网络超时]".to_string(),
                detail_path: "".to_string(),
                created_at: "2026-05-01T00:00:00+08:00".to_string(),
                updated_at: "2026-05-03T00:00:00+08:00".to_string(),
            },
            MemoryItem {
                title: "待办的事".to_string(),
                description: "[task pri=2] 倒垃圾 #家务".to_string(),
                detail_path: "butler_tasks/dao_la_ji.md".to_string(),
                created_at: "2026-05-04T00:00:00+08:00".to_string(),
                updated_at: "2026-05-04T00:00:00+08:00".to_string(),
            },
        ];
        let n = backfill_butler_tasks(&conn, &items).unwrap();
        assert_eq!(n, 3);

        let done = butler_task_get(&conn, "做完的事").unwrap().unwrap();
        assert_eq!(done.status, "done");
        assert_eq!(done.tags, vec!["生活"]);
        assert_eq!(done.detail_path, Some("butler_tasks/zuo_wan_de_shi.md".to_string()));

        let err = butler_task_get(&conn, "失败的事").unwrap().unwrap();
        assert_eq!(err.status, "error");
        assert_eq!(err.tags, vec!["工作"]);
        assert_eq!(err.detail_path, None, "empty detail_path → None");

        let pending = butler_task_get(&conn, "待办的事").unwrap().unwrap();
        assert_eq!(pending.status, "pending");
        assert_eq!(pending.tags, vec!["家务"]);

        // 再跑一次 backfill：幂等，不重复插
        let n2 = backfill_butler_tasks(&conn, &items).unwrap();
        assert_eq!(n2, 0, "re-backfill must skip existing titles");
    }

    #[test]
    fn todo_crud_roundtrip() {
        let conn = fresh_conn();
        let row = TodoRow {
            title: "周三 14:00 视频会议".to_string(),
            description: "[remind: 2026-05-14 14:00] 客户 demo".to_string(),
            status: "active".to_string(),
            detail_path: None,
            tags: vec![],
            created_at: "".to_string(),
            updated_at: "".to_string(),
        };
        todo_create(&conn, &row).unwrap();
        let fetched = todo_get(&conn, "周三 14:00 视频会议").unwrap().unwrap();
        assert_eq!(fetched.status, "active");
        assert!(!fetched.created_at.is_empty());

        let updated = todo_update(
            &conn,
            "周三 14:00 视频会议",
            "[remind: 2026-05-14 14:30] 客户 demo（推迟 30 分钟）",
            "active",
            None,
            &[],
        )
        .unwrap();
        assert!(updated);
        let after = todo_get(&conn, "周三 14:00 视频会议").unwrap().unwrap();
        assert!(after.description.contains("14:30"));

        let deleted = todo_delete(&conn, "周三 14:00 视频会议").unwrap();
        assert!(deleted);
        assert!(todo_get(&conn, "周三 14:00 视频会议").unwrap().is_none());
    }

    #[test]
    fn todo_backfill_skips_existing() {
        use crate::commands::memory::MemoryItem;
        let conn = fresh_conn();
        let items = vec![MemoryItem {
            title: "买菜".to_string(),
            description: "晚上买番茄鸡蛋".to_string(),
            detail_path: "".to_string(),
            created_at: "2026-05-01T00:00:00+08:00".to_string(),
            updated_at: "2026-05-01T00:00:00+08:00".to_string(),
        }];
        let n = backfill_todos(&conn, &items).unwrap();
        assert_eq!(n, 1);
        let n2 = backfill_todos(&conn, &items).unwrap();
        assert_eq!(n2, 0, "re-backfill must skip existing titles");
    }

    #[test]
    fn list_order_by_updated_at_desc() {
        let conn = fresh_conn();
        butler_task_create(
            &conn,
            &ButlerTaskRow {
                title: "A".to_string(),
                description: "".to_string(),
                status: "pending".to_string(),
                detail_path: None,
                tags: vec![],
                created_at: "2026-05-01T00:00:00+08:00".to_string(),
                updated_at: "2026-05-01T00:00:00+08:00".to_string(),
            },
        )
        .unwrap();
        butler_task_create(
            &conn,
            &ButlerTaskRow {
                title: "B".to_string(),
                description: "".to_string(),
                status: "pending".to_string(),
                detail_path: None,
                tags: vec![],
                created_at: "2026-05-02T00:00:00+08:00".to_string(),
                updated_at: "2026-05-02T00:00:00+08:00".to_string(),
            },
        )
        .unwrap();
        let rows = butler_tasks_list(&conn).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "B", "newest updated_at first");
        assert_eq!(rows[1].title, "A");
    }

    // -------- task_stats --------

    fn insert_row(
        conn: &Connection,
        title: &str,
        desc: &str,
        status: &str,
        updated_at: &str,
    ) {
        butler_task_create(
            conn,
            &ButlerTaskRow {
                title: title.to_string(),
                description: desc.to_string(),
                status: status.to_string(),
                detail_path: None,
                tags: vec![],
                created_at: updated_at.to_string(),
                updated_at: updated_at.to_string(),
            },
        )
        .unwrap();
    }

    #[test]
    fn task_stats_all_zero_on_empty_table() {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 14)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let s = compute_task_stats(&conn, now).unwrap();
        assert_eq!(s.pending, 0);
        assert_eq!(s.overdue, 0);
        assert_eq!(s.done_today, 0);
        assert_eq!(s.error, 0);
        assert_eq!(s.cancelled_today, 0);
        assert_eq!(s.snoozed, 0);
    }

    #[test]
    fn task_stats_counts_each_status() {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();
        // pending（无 due → 不计逾期）
        insert_row(
            &conn,
            "未来的事",
            "[task pri=3] 未来的事",
            "pending",
            "2026-05-14T11:30:00+08:00",
        );
        // done 今天
        insert_row(
            &conn,
            "已完成今天",
            "[task pri=3] 已完成今天 [done]",
            "done",
            "2026-05-14T10:00:00+08:00",
        );
        // done 昨天 —— 不计今日
        insert_row(
            &conn,
            "已完成昨天",
            "[task pri=3] 已完成昨天 [done]",
            "done",
            "2026-05-13T10:00:00+08:00",
        );
        // error（任意时间，不限今日）
        insert_row(
            &conn,
            "出错的事",
            "[task pri=3] 出错的事 [error: 没网]",
            "error",
            "2026-05-10T10:00:00+08:00",
        );
        // cancelled 今天
        insert_row(
            &conn,
            "今天取消",
            "[task pri=3] 今天取消 [cancelled: 改主意]",
            "cancelled",
            "2026-05-14T11:00:00+08:00",
        );
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 14)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let s = compute_task_stats(&conn, now).unwrap();
        assert_eq!(s.pending, 1, "{s:?}");
        assert_eq!(s.overdue, 0, "{s:?}");
        assert_eq!(s.done_today, 1, "{s:?}");
        assert_eq!(s.error, 1, "{s:?}");
        assert_eq!(s.cancelled_today, 1, "{s:?}");
    }

    #[test]
    fn task_stats_overdue_picks_pending_with_past_due() {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();
        // due 在 now 之前 → 计入逾期
        insert_row(
            &conn,
            "过期了",
            "[task pri=3 due=2020-01-01T10:00] 过期了",
            "pending",
            "2026-05-14T11:30:00+08:00",
        );
        // due 在 now 之后 → 不计入
        insert_row(
            &conn,
            "未来到期",
            "[task pri=3 due=2030-01-01T10:00] 未来到期",
            "pending",
            "2026-05-14T11:30:00+08:00",
        );
        // done 状态有过期 due 也不计入（仅 pending 才数）
        insert_row(
            &conn,
            "已完成的过期",
            "[task pri=3 due=2020-01-01T10:00] 已完成的过期 [done]",
            "done",
            "2026-05-13T11:00:00+08:00",
        );
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 14)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let s = compute_task_stats(&conn, now).unwrap();
        assert_eq!(s.pending, 2);
        assert_eq!(s.overdue, 1, "only pending with past due counts");
    }

    #[test]
    fn task_stats_snoozed_counts_pending_with_future_snooze() {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();
        // pending + snooze 未来 → 计入 snoozed
        insert_row(
            &conn,
            "睡到下周",
            "[task pri=3] 睡到下周 [snooze: 2030-01-01 09:00]",
            "pending",
            "2026-05-14T11:30:00+08:00",
        );
        // pending + snooze 过去（已到期）→ 不计 snoozed（marker 自然失效）
        insert_row(
            &conn,
            "已醒",
            "[task pri=3] 已醒 [snooze: 2020-01-01 09:00]",
            "pending",
            "2026-05-14T11:30:00+08:00",
        );
        // pending 无 snooze marker → 不计 snoozed
        insert_row(
            &conn,
            "活跃任务",
            "[task pri=3] 活跃任务",
            "pending",
            "2026-05-14T11:30:00+08:00",
        );
        // done 状态即便有 future snooze 也不计（snooze 仅 pending 有意义）
        insert_row(
            &conn,
            "已完成的暂停",
            "[task pri=3] 已完成的暂停 [snooze: 2030-01-01 09:00] [done]",
            "done",
            "2026-05-13T11:00:00+08:00",
        );
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 14)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let s = compute_task_stats(&conn, now).unwrap();
        assert_eq!(s.pending, 3, "{s:?}");
        assert_eq!(s.snoozed, 1, "only pending + future snooze counts");
    }

    // -------- archive purge helper --------

    #[test]
    fn select_archive_titles_older_than_picks_only_old() {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();
        // 老归档 1：2025-12-01
        task_archive_create(
            &conn,
            &TaskArchiveRow {
                title: "old-1".to_string(),
                description: "old".to_string(),
                status: "archived".to_string(),
                detail_path: None,
                tags: vec![],
                created_at: "2025-12-01T10:00:00+08:00".to_string(),
                updated_at: "2025-12-01T10:00:00+08:00".to_string(),
            },
        )
        .unwrap();
        // 老归档 2：2025-12-15
        task_archive_create(
            &conn,
            &TaskArchiveRow {
                title: "old-2".to_string(),
                description: "old".to_string(),
                status: "archived".to_string(),
                detail_path: None,
                tags: vec![],
                created_at: "2025-12-15T10:00:00+08:00".to_string(),
                updated_at: "2025-12-15T10:00:00+08:00".to_string(),
            },
        )
        .unwrap();
        // 新归档：2026-05-13
        task_archive_create(
            &conn,
            &TaskArchiveRow {
                title: "new".to_string(),
                description: "new".to_string(),
                status: "archived".to_string(),
                detail_path: None,
                tags: vec![],
                created_at: "2026-05-13T10:00:00+08:00".to_string(),
                updated_at: "2026-05-13T10:00:00+08:00".to_string(),
            },
        )
        .unwrap();
        // cutoff = 2026-01-01 → 应仅命中 2 条 2025-12-* 归档
        let titles = select_archive_titles_older_than(&conn, "2026-01-01T00:00:00+08:00")
            .unwrap();
        assert_eq!(titles.len(), 2, "got: {titles:?}");
        assert!(titles.contains(&"old-1".to_string()));
        assert!(titles.contains(&"old-2".to_string()));
        assert!(!titles.contains(&"new".to_string()));
    }

    #[test]
    fn select_archive_titles_older_than_empty_when_nothing_old() {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();
        task_archive_create(
            &conn,
            &TaskArchiveRow {
                title: "fresh".to_string(),
                description: "x".to_string(),
                status: "archived".to_string(),
                detail_path: None,
                tags: vec![],
                created_at: "2026-05-13T10:00:00+08:00".to_string(),
                updated_at: "2026-05-13T10:00:00+08:00".to_string(),
            },
        )
        .unwrap();
        let titles = select_archive_titles_older_than(&conn, "2020-01-01T00:00:00+08:00")
            .unwrap();
        assert!(titles.is_empty(), "got: {titles:?}");
    }
