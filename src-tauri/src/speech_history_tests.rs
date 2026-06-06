    use super::*;

    #[test]
    fn parse_recent_empty_content() {
        assert!(parse_recent("", 5).is_empty());
    }

    #[test]
    fn parse_recent_n_zero() {
        assert!(parse_recent("a\nb\nc\n", 0).is_empty());
    }

    #[test]
    fn parse_recent_fewer_than_n() {
        let v = parse_recent("a\nb\n", 5);
        assert_eq!(v, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parse_recent_exactly_n() {
        let v = parse_recent("a\nb\nc\n", 3);
        assert_eq!(v, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn parse_recent_more_than_n_keeps_tail() {
        let v = parse_recent("a\nb\nc\nd\ne\n", 3);
        assert_eq!(v, vec!["c".to_string(), "d".to_string(), "e".to_string()]);
    }

    #[test]
    fn parse_recent_skips_blank_lines() {
        let v = parse_recent("a\n\nb\n\nc\n", 5);
        assert_eq!(v, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn strip_timestamp_normal_line() {
        let line = "2026-05-03T12:34:56+08:00 早上好啊";
        assert_eq!(strip_timestamp(line), "早上好啊");
    }

    #[test]
    fn strip_timestamp_no_space_returns_whole_line() {
        assert_eq!(strip_timestamp("noprefix"), "noprefix");
    }

    #[test]
    fn parse_daily_empty_or_malformed() {
        assert!(parse_daily("").is_empty());
        assert!(parse_daily("not json").is_empty());
        assert!(parse_daily("[1, 2, 3]").is_empty());
    }

    #[test]
    fn parse_daily_valid_object() {
        let m = parse_daily(r#"{"2026-05-01": 3, "2026-05-02": 5}"#);
        assert_eq!(m.get("2026-05-01"), Some(&3));
        assert_eq!(m.get("2026-05-02"), Some(&5));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn prune_daily_drops_entries_before_cutoff() {
        let mut m = BTreeMap::new();
        m.insert("2026-01-01".to_string(), 10);
        m.insert("2026-04-01".to_string(), 20);
        m.insert("2026-05-01".to_string(), 30);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        let pruned = prune_daily(m, today, 30);
        // cutoff = 2026-04-03; 2026-01-01 (< cutoff) drops, 2026-04-01 (< cutoff) drops,
        // 2026-05-01 (>= cutoff) stays.
        assert_eq!(pruned.len(), 1);
        assert!(pruned.contains_key("2026-05-01"));
    }

    #[test]
    fn prune_daily_keeps_unparseable_keys() {
        let mut m = BTreeMap::new();
        m.insert("not-a-date".to_string(), 7);
        m.insert("2026-01-01".to_string(), 1);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        let pruned = prune_daily(m, today, 30);
        assert!(pruned.contains_key("not-a-date"));
        assert!(!pruned.contains_key("2026-01-01"));
    }

    #[test]
    fn sum_recent_days_basic() {
        let mut m = BTreeMap::new();
        m.insert("2026-05-01".to_string(), 3);
        m.insert("2026-05-02".to_string(), 5);
        m.insert("2026-05-03".to_string(), 7); // today
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        // 7-day window ending today = 2026-04-27..2026-05-03 inclusive. Only 3 days
        // present, others zero.
        assert_eq!(sum_recent_days(&m, today, 7), 3 + 5 + 7);
    }

    #[test]
    fn sum_recent_days_window_excludes_older() {
        let mut m = BTreeMap::new();
        m.insert("2026-04-26".to_string(), 100); // 7 days before today → excluded by 7-window
        m.insert("2026-05-03".to_string(), 4);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        // 7-day window: 2026-04-27..2026-05-03. 04-26 is outside.
        assert_eq!(sum_recent_days(&m, today, 7), 4);
    }

    #[test]
    fn sum_recent_days_zero_window_returns_zero() {
        let mut m = BTreeMap::new();
        m.insert("2026-05-03".to_string(), 99);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        assert_eq!(sum_recent_days(&m, today, 0), 0);
    }

    #[test]
    fn sum_recent_days_handles_empty_map() {
        let m = BTreeMap::new();
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        assert_eq!(sum_recent_days(&m, today, 7), 0);
    }

    #[test]
    fn prune_daily_zero_retain_drops_everything_dated() {
        let mut m = BTreeMap::new();
        m.insert("2026-05-03".to_string(), 1);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        let pruned = prune_daily(m, today, 0);
        // cutoff == today, "2026-05-03" >= "2026-05-03" → kept (today is always retained).
        assert!(pruned.contains_key("2026-05-03"));
    }

    fn fresh_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("pet-test-{}-{}", label, nanos));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Manual round-trip: write entries directly with the same trim semantics as
    /// record_speech_inner, then validate parse_recent reads back the tail. We don't go
    /// through record_speech_inner because that hard-codes the user's config_dir path;
    /// recreating the trim logic in tests keeps file IO opt-out.
    #[test]
    fn write_and_parse_round_trip_with_trim() {
        let dir = fresh_temp_dir("speech");
        let path = dir.join("speech_history.log");
        let mut entries: Vec<String> = (0..(SPEECH_HISTORY_CAP + 5))
            .map(|i| format!("2026-05-03T12:00:00+08:00 line {}", i))
            .collect();
        if entries.len() > SPEECH_HISTORY_CAP {
            let drop = entries.len() - SPEECH_HISTORY_CAP;
            entries.drain(0..drop);
        }
        std::fs::write(&path, entries.join("\n") + "\n").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let recent = parse_recent(&content, 3);
        assert_eq!(recent.len(), 3);
        // After trimming the first 5, lines 5..(50+5) remain; last 3 are 52, 53, 54.
        assert!(recent[2].ends_with("line 54"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -- Iter R11: detect_repeated_topic -------------------------------------

    fn line(text: &str) -> String {
        format!("2026-05-03T10:00:00+08:00 {}", text)
    }

    #[test]
    fn detect_repeated_topic_returns_none_for_empty_input() {
        assert!(detect_repeated_topic(&[], 4, 3).is_none());
    }

    #[test]
    fn detect_repeated_topic_returns_none_when_no_overlap() {
        let lines = vec![
            line("早上好今天怎么样"),
            line("最近天气不错适合散步"),
            line("中午吃了什么"),
        ];
        // No 4-char window appears in 3 distinct lines.
        assert!(detect_repeated_topic(&lines, 4, 3).is_none());
    }

    #[test]
    fn detect_repeated_topic_finds_chinese_topic_across_three_lines() {
        // "工作进展" appears in three lines → flagged.
        let lines = vec![
            line("看你在专心工作进展不错"),
            line("工作进展怎么样了"),
            line("聊聊你今天的工作进展吧"),
        ];
        let topic = detect_repeated_topic(&lines, 4, 3).expect("should detect");
        assert!(
            topic.contains("工作进展"),
            "expected to surface 工作进展, got '{}'",
            topic
        );
    }

    #[test]
    fn detect_repeated_topic_respects_min_distinct_lines() {
        // Only 2 lines share "周末出去" — below min_distinct_lines=3 → None.
        let lines = vec![
            line("周末出去走走吧"),
            line("周末出去吃饭怎么样"),
            line("今天天气不错"),
        ];
        assert!(detect_repeated_topic(&lines, 4, 3).is_none());
        // But min=2 → fires.
        assert!(detect_repeated_topic(&lines, 4, 2).is_some());
    }

    #[test]
    fn detect_repeated_topic_skips_whitespace_bearing_windows() {
        // "了 我" / " 我们" sliding across word boundary should not be flagged
        // even though it'd technically appear multiple times.
        let lines = vec![
            line("吃饭了 我们走"),
            line("回来了 我们一起"),
            line("睡觉了 我们再聊"),
        ];
        // Distinct words; only artifact "了 我" or " 我们" connects them across
        // whitespace — those are explicitly skipped.
        let topic = detect_repeated_topic(&lines, 4, 3);
        if let Some(t) = topic {
            assert!(
                !t.contains(' '),
                "topic should not contain whitespace, got '{}'",
                t
            );
        }
    }

    #[test]
    fn detect_repeated_topic_skips_uniform_char_windows() {
        // Test sentinel: "...." or "嗯嗯嗯嗯" are formatting/filler not topics.
        let lines = vec![
            line("嗯嗯嗯嗯继续吧"),
            line("好的嗯嗯嗯嗯"),
            line("嗯嗯嗯嗯让我想想"),
        ];
        let topic = detect_repeated_topic(&lines, 4, 3);
        // If anything fires it must NOT be the uniform-char window.
        if let Some(t) = topic {
            assert!(
                t != "嗯嗯嗯嗯",
                "uniform-char windows should be filtered, got '{}'",
                t
            );
        }
    }

    #[test]
    fn detect_repeated_topic_handles_short_lines() {
        // Lines shorter than ngram_size are silently skipped — no panic.
        let lines = vec![line("嗨"), line("好"), line("不错")];
        assert!(detect_repeated_topic(&lines, 4, 1).is_none());
    }

    // -- Iter R14: speeches_for_date -----------------------------------------

    fn ts_line(date: &str, time: &str, text: &str) -> String {
        // Format matches what record_speech writes: "YYYY-MM-DDTHH:MM:SS+TZ text".
        // Use a fixed offset (+08:00) so tests don't depend on the runner's tz.
        format!("{}T{}+08:00 {}", date, time, text)
    }

    fn nd(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn speeches_for_date_empty_content_returns_empty() {
        assert!(speeches_for_date("", nd(2026, 5, 3), 5).is_empty());
    }

    #[test]
    fn speeches_for_date_zero_max_returns_empty() {
        let content = ts_line("2026-05-03", "10:00:00", "早上好");
        assert!(speeches_for_date(&content, nd(2026, 5, 3), 0).is_empty());
    }

    #[test]
    fn speeches_for_date_filters_by_date() {
        let content = [
            ts_line("2026-05-02", "23:00:00", "晚安"),
            ts_line("2026-05-03", "08:00:00", "早安"),
            ts_line("2026-05-03", "12:00:00", "中午好"),
            ts_line("2026-05-04", "08:00:00", "新一天"),
        ]
        .join("\n");
        let out = speeches_for_date(&content, nd(2026, 5, 3), 5);
        assert_eq!(out.len(), 2);
        assert!(out[0].contains("早安"));
        assert!(out[1].contains("中午好"));
    }

    #[test]
    fn speeches_for_date_returns_last_max_when_more_match() {
        let content = [
            ts_line("2026-05-03", "08:00:00", "a"),
            ts_line("2026-05-03", "10:00:00", "b"),
            ts_line("2026-05-03", "12:00:00", "c"),
            ts_line("2026-05-03", "14:00:00", "d"),
        ]
        .join("\n");
        let out = speeches_for_date(&content, nd(2026, 5, 3), 2);
        // Last 2 in chronological order: c, d.
        assert_eq!(out.len(), 2);
        assert!(out[0].ends_with(" c"));
        assert!(out[1].ends_with(" d"));
    }

    #[test]
    fn speeches_for_date_skips_malformed_lines() {
        // Garbage line + line without timestamp + valid line — only the
        // valid one passes the filter.
        let content = [
            "garbage no space".to_string(),
            "not-a-timestamp line".to_string(),
            ts_line("2026-05-03", "10:00:00", "早上好"),
        ]
        .join("\n");
        let out = speeches_for_date(&content, nd(2026, 5, 3), 5);
        assert_eq!(out.len(), 1);
        assert!(out[0].contains("早上好"));
    }

    #[test]
    fn speeches_for_date_target_date_with_no_matches_returns_empty() {
        let content = ts_line("2026-05-03", "10:00:00", "今天的话");
        // Looking for yesterday — no match.
        assert!(speeches_for_date(&content, nd(2026, 5, 2), 5).is_empty());
    }

    fn ts(text: &str) -> String {
        format!("2026-05-04T12:00:00+08:00 {}", text)
    }

    #[test]
    fn length_hint_returns_empty_below_min_samples() {
        // R19: less than 3 samples = empty (not enough signal).
        assert_eq!(format_speech_length_hint(&[]), "");
        assert_eq!(
            format_speech_length_hint(&[ts("早上好啊好朋友今天怎么样")]),
            ""
        );
        assert_eq!(
            format_speech_length_hint(&[
                ts("早上好啊好朋友今天怎么样"),
                ts("中午吃了吗最近忙不忙啊"),
            ]),
            ""
        );
    }

    #[test]
    fn length_hint_fires_when_all_long() {
        // 3 lines all ≥ 25 chars → "偏长" hint.
        let lines = vec![
            ts("今天打算把那个超长的项目报告好好处理一下再放松休息吃饭"), // 27
            ts("最近这几天看起来真忙的样子要不要停下来多喝几口水休息会儿"), // 28
            ts("昨晚那本小说终于读完了我一直好奇结尾的反转给我讲讲嘛朋友"), // 28
        ];
        let hint = format_speech_length_hint(&lines);
        assert!(hint.contains("偏长"), "got {}", hint);
        assert!(hint.contains("更短"));
    }

    #[test]
    fn length_hint_fires_when_all_short() {
        // 3 lines all ≤ 8 chars → "偏短" hint.
        let lines = vec![ts("嘿"), ts("在吗？"), ts("吃了吗？")];
        let hint = format_speech_length_hint(&lines);
        assert!(hint.contains("偏短"), "got {}", hint);
        assert!(hint.contains("多花两句"));
    }

    #[test]
    fn length_hint_returns_empty_for_mixed_register() {
        // Mixed: 1 short + 2 long → already varying, no nudge.
        let lines = vec![
            ts("嘿"),
            ts("今天打算把那个超长的项目报告好好处理一下再放松休息吃饭"),
            ts("昨晚那本小说终于读完了我一直好奇结尾的反转给我讲讲嘛朋友"),
        ];
        assert_eq!(format_speech_length_hint(&lines), "");
    }

    #[test]
    fn length_hint_handles_chinese_correctly() {
        // 30 chars 中文 should register as 30 (chars().count()), not 90 (bytes).
        let line_30_chars = "一二三四五六七八九十十一十二十三十四十五十六十七十八十九二十二十一二十二二十三二十四二十五";
        let lines = vec![ts(line_30_chars), ts(line_30_chars), ts(line_30_chars)];
        let hint = format_speech_length_hint(&lines);
        assert!(hint.contains("偏长"));
    }

    #[test]
    fn length_hint_skips_empty_lines() {
        // Empty stripped lines shouldn't drag mean to 0 / register as "短".
        let lines = vec![
            ts(""),
            ts("今天打算把那个超长的项目报告好好处理一下再放松休息吃饭"),
            ts("昨晚那本小说终于读完了我一直好奇结尾的反转给我讲讲嘛朋友"),
            ts("最近这几天看起来真忙的样子要不要停下来多喝几口水休息会儿"),
        ];
        // 1 empty + 3 long → 3 nonzero ≥ min_samples, all long → 偏长
        let hint = format_speech_length_hint(&lines);
        assert!(hint.contains("偏长"), "got {}", hint);
    }

    #[test]
    fn length_hint_returns_empty_when_too_few_nonzero() {
        // 4 lines but 2 are empty → only 2 nonzero, below threshold.
        let lines = vec![
            ts(""),
            ts(""),
            ts("今天打算把那个超长的项目报告好好处理一下再放松休息吃饭"),
            ts("最近这几天看起来真忙的样子要不要停下来多喝几口水休息会儿"),
        ];
        assert_eq!(format_speech_length_hint(&lines), "");
    }

    #[test]
    fn length_hint_includes_sample_count_and_mean() {
        let lines = vec![ts("嘿"), ts("好的"), ts("好啊")];
        let hint = format_speech_length_hint(&lines);
        // mean = (1 + 2 + 2) / 3 = 1
        assert!(hint.contains("3 句"));
        assert!(hint.contains("平均"));
    }

    #[test]
    fn classify_register_returns_none_below_min_samples() {
        assert!(classify_speech_register(&[]).is_none());
        assert!(classify_speech_register(&[ts("一"), ts("二")]).is_none());
    }

    #[test]
    fn classify_register_returns_long_when_all_long() {
        let lines = vec![
            ts("今天打算把那个超长的项目报告好好处理一下再放松休息吃饭"),
            ts("最近这几天看起来真忙的样子要不要停下来多喝几口水休息会儿"),
            ts("昨晚那本小说终于读完了我一直好奇结尾的反转给我讲讲嘛朋友"),
        ];
        let summary = classify_speech_register(&lines).unwrap();
        assert_eq!(summary.kind, "long");
        assert_eq!(summary.samples, 3);
        assert!(summary.mean_chars >= 25);
    }

    #[test]
    fn classify_register_returns_short_when_all_short() {
        let lines = vec![ts("嘿"), ts("好的"), ts("吃了吗？")];
        let summary = classify_speech_register(&lines).unwrap();
        assert_eq!(summary.kind, "short");
        assert!(summary.mean_chars <= 8);
    }

    // -- Iter #389: speech meta sidecar -------------------------------------

    #[test]
    fn parse_meta_index_empty_content() {
        let m = parse_meta_index("");
        assert!(m.is_empty());
    }

    #[test]
    fn parse_meta_index_valid_lines() {
        let content = r#"{"ts":"2026-05-17T18:00:00+08:00","band":"mid","factor":1.0,"mode":"normal","deadline_factor":1.0}
{"ts":"2026-05-17T19:00:00+08:00","band":"low_negative","factor":0.7,"mode":"normal","deadline_factor":1.0}"#;
        let m = parse_meta_index(content);
        assert_eq!(m.len(), 2);
        let e1 = m.get("2026-05-17T18:00:00+08:00").unwrap();
        assert_eq!(e1.band, "mid");
        assert!((e1.factor - 1.0).abs() < 1e-9);
        let e2 = m.get("2026-05-17T19:00:00+08:00").unwrap();
        assert_eq!(e2.band, "low_negative");
        assert!((e2.factor - 0.7).abs() < 1e-9);
    }

    #[test]
    fn parse_meta_index_skips_malformed_lines() {
        // 混入 garbage 行不应阻塞 valid 行解析
        let content = r#"not valid json
{"ts":"2026-05-17T18:00:00+08:00","band":"mid","factor":1.0,"mode":"normal","deadline_factor":1.0}
also garbage
{"missing_required_fields": true}"#;
        let m = parse_meta_index(content);
        assert_eq!(m.len(), 1);
        assert!(m.contains_key("2026-05-17T18:00:00+08:00"));
    }

    #[test]
    fn parse_meta_index_dedup_by_ts() {
        // 同 ts 出现两次 — 后写覆盖前写（HashMap insert 语义；append-only
        // 文件中应不会发生，但 defensive 测试）
        let content = r#"{"ts":"T1","band":"mid","factor":1.0,"mode":"normal","deadline_factor":1.0}
{"ts":"T1","band":"high_negative","factor":2.0,"mode":"normal","deadline_factor":1.0}"#;
        let m = parse_meta_index(content);
        assert_eq!(m.len(), 1);
        let e = m.get("T1").unwrap();
        assert_eq!(e.band, "high_negative");
        assert!((e.factor - 2.0).abs() < 1e-9);
    }

    #[test]
    fn classify_register_returns_mixed_for_varied_register() {
        // R20: "mixed" is now an explicit return value (not collapsed to None).
        // Panel needs to render "📏 混合" chip even when LLM gets no nudge.
        let lines = vec![
            ts("嘿"),
            ts("今天打算把那个超长的项目报告好好处理一下再放松休息吃饭"),
            ts("昨晚那本小说终于读完了我一直好奇结尾的反转给我讲讲嘛朋友"),
        ];
        let summary = classify_speech_register(&lines).unwrap();
        assert_eq!(summary.kind, "mixed");
    }
