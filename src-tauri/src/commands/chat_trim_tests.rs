    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        serde_json::from_value(serde_json::json!({
            "role": role,
            "content": content,
        }))
        .unwrap()
    }

    fn roles(msgs: &[ChatMessage]) -> Vec<&str> {
        msgs.iter().map(|m| m.role.as_str()).collect()
    }

    #[test]
    fn trim_zero_disables_gate() {
        let msgs = vec![
            msg("system", "soul"),
            msg("user", "hi"),
            msg("assistant", "hi"),
        ];
        let out = trim_to_context(msgs.clone(), 0);
        assert_eq!(out.len(), msgs.len(), "max=0 should leave input alone");
    }

    #[test]
    fn trim_below_cap_is_no_op() {
        let msgs = vec![
            msg("system", "soul"),
            msg("user", "hi"),
            msg("assistant", "hi"),
        ];
        let out = trim_to_context(msgs.clone(), 10);
        assert_eq!(out.len(), msgs.len());
    }

    #[test]
    fn trim_drops_oldest_history_keeps_system() {
        // 1 system + 6 user/assistant pairs = 13 total, history = 12. With max=4 we keep
        // system + the last 4 messages.
        let mut msgs = vec![msg("system", "soul")];
        for i in 0..6 {
            msgs.push(msg("user", &format!("u{}", i)));
            msgs.push(msg("assistant", &format!("a{}", i)));
        }
        let out = trim_to_context(msgs, 4);
        assert_eq!(out.len(), 5, "system + 4 history");
        assert_eq!(out[0].role, "system");
        // Last 4 should be u4, a4, u5, a5.
        assert_eq!(
            roles(&out[1..]),
            vec!["user", "assistant", "user", "assistant"]
        );
    }

    #[test]
    fn trim_preserves_multiple_leading_systems() {
        let msgs = vec![
            msg("system", "soul"),
            msg("system", "mood"),
            msg("user", "u1"),
            msg("assistant", "a1"),
            msg("user", "u2"),
            msg("assistant", "a2"),
        ];
        let out = trim_to_context(msgs, 2);
        assert_eq!(out.len(), 4, "2 systems + 2 history");
        assert_eq!(roles(&out), vec!["system", "system", "user", "assistant"]);
    }

    #[test]
    fn trim_with_no_system_messages() {
        let msgs = vec![
            msg("user", "u1"),
            msg("assistant", "a1"),
            msg("user", "u2"),
            msg("assistant", "a2"),
        ];
        let out = trim_to_context(msgs, 2);
        assert_eq!(out.len(), 2);
        assert_eq!(roles(&out), vec!["user", "assistant"]);
    }

    // -- Iter R5: refresh_leading_soul ---------------------------------------

    fn content_str(m: &ChatMessage) -> &str {
        m.content.as_str().unwrap_or("")
    }

    #[test]
    fn refresh_leading_soul_replaces_first_system_content() {
        let msgs = vec![msg("system", "OLD soul"), msg("user", "hi")];
        let out = refresh_leading_soul(msgs, "FRESH soul");
        assert_eq!(out.len(), 2, "no message count change");
        assert_eq!(content_str(&out[0]), "FRESH soul");
        assert_eq!(content_str(&out[1]), "hi", "user message untouched");
    }

    #[test]
    fn refresh_leading_soul_no_op_when_first_is_not_system() {
        // Histories with a leading user message exist when sessions were created
        // pre-Iter R5 or via paths that don't bake SOUL in. Don't synthesize one.
        let msgs = vec![msg("user", "u1"), msg("system", "embedded later")];
        let out = refresh_leading_soul(msgs, "FRESH soul");
        assert_eq!(content_str(&out[0]), "u1");
        assert_eq!(content_str(&out[1]), "embedded later");
    }

    #[test]
    fn refresh_leading_soul_only_touches_first_system_when_multiple() {
        // The chat pipeline injects mood / persona system messages AFTER the
        // session's leading SOUL slot. R5 must replace the SOUL slot only and
        // leave subsequent system messages alone — they carry transient prompt
        // context that doesn't come from SOUL.md.
        let msgs = vec![
            msg("system", "SOUL slot — stale"),
            msg("system", "mood note slot"),
            msg("user", "hi"),
        ];
        let out = refresh_leading_soul(msgs, "FRESH soul");
        assert_eq!(content_str(&out[0]), "FRESH soul");
        assert_eq!(content_str(&out[1]), "mood note slot");
        assert_eq!(content_str(&out[2]), "hi");
    }

    #[test]
    fn refresh_leading_soul_skips_when_current_is_blank() {
        // Empty / whitespace SOUL would zero out the system slot — better to
        // leave the prior content intact so the LLM still has *something*.
        let msgs = vec![msg("system", "PRIOR soul"), msg("user", "hi")];
        let out = refresh_leading_soul(msgs, "   \n  ");
        assert_eq!(content_str(&out[0]), "PRIOR soul");
    }

    #[test]
    fn refresh_leading_soul_empty_messages_passes_through() {
        let out: Vec<ChatMessage> = refresh_leading_soul(vec![], "FRESH");
        assert_eq!(out.len(), 0);
    }

    // -- Iter R9: format_recent_speech_layer ----------------------------------

    #[test]
    fn format_recent_speech_layer_returns_empty_for_no_lines() {
        assert_eq!(format_recent_speech_layer(&[]), "");
    }

    #[test]
    fn format_recent_speech_layer_skips_blank_lines() {
        // Empty / whitespace-only entries shouldn't render as ghost bullets.
        let lines = vec!["".to_string(), "   ".to_string(), "".to_string()];
        assert_eq!(format_recent_speech_layer(&lines), "");
    }

    #[test]
    fn format_recent_speech_layer_renders_bullets_in_order() {
        // recent_speeches returns oldest-first; preserve that ordering so the
        // bullet list reads chronologically and the latest utterance is
        // closest to the user's incoming message.
        let lines = vec![
            "2026-05-03T10:00:00+08:00 早上好".to_string(),
            "2026-05-03T11:30:00+08:00 看你还在工作".to_string(),
        ];
        let out = format_recent_speech_layer(&lines);
        assert!(out.starts_with("[最近主动开口]"));
        let m = out.find("早上好").unwrap();
        let n = out.find("看你还在工作").unwrap();
        assert!(m < n, "oldest first preserved");
        // Header signals the LLM how to use the section.
        assert!(
            out.contains("旧→新") && out.contains("接住话题"),
            "header should explain ordering + intent"
        );
    }

    #[test]
    fn format_recent_speech_layer_strips_timestamps_for_readability() {
        // Bullets shouldn't include the ISO timestamp prefix — the LLM
        // doesn't need it and it'd just spend tokens.
        let lines = vec!["2026-05-03T10:00:00+08:00 早上好".to_string()];
        let out = format_recent_speech_layer(&lines);
        assert!(!out.contains("2026-05-03T10:00:00"));
        assert!(out.contains("早上好"));
    }

    use chrono::NaiveDate;

    // -- Iter R79: format_deadline_chat_layer tests -------------------------

    #[test]
    fn deadline_chat_layer_returns_empty_when_all_distant_or_none() {
        let now = NaiveDate::from_ymd_opt(2026, 5, 10)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap();
        // Empty input.
        assert_eq!(format_deadline_chat_layer(&[], now), "");
        // All distant (≥6h away).
        let items = vec![(
            NaiveDate::from_ymd_opt(2026, 5, 12)
                .unwrap()
                .and_hms_opt(14, 0, 0)
                .unwrap(),
            "future".to_string(),
        )];
        assert_eq!(format_deadline_chat_layer(&items, now), "");
    }

    #[test]
    fn deadline_chat_layer_includes_approaching_unlike_proactive_filter() {
        // R79 distinction: chat layer surfaces Approaching too (user might ask),
        // even though R77's proactive format would also include it.
        let now = NaiveDate::from_ymd_opt(2026, 5, 10)
            .unwrap()
            .and_hms_opt(11, 0, 0)
            .unwrap();
        let items = vec![(
            NaiveDate::from_ymd_opt(2026, 5, 10)
                .unwrap()
                .and_hms_opt(14, 0, 0)
                .unwrap(),
            "review PR".to_string(),
        )];
        let out = format_deadline_chat_layer(&items, now);
        assert!(out.contains("[当前 deadline 概况]"));
        assert!(out.contains("review PR"));
        assert!(out.contains("约 3 小时后"));
        // Chat-specific tail wording differs from proactive.
        assert!(out.contains("不必主动列举"));
    }

    #[test]
    fn deadline_chat_layer_handles_overdue_minutes_then_hours() {
        let now = NaiveDate::from_ymd_opt(2026, 5, 10)
            .unwrap()
            .and_hms_opt(14, 30, 0)
            .unwrap();
        let items = vec![(
            NaiveDate::from_ymd_opt(2026, 5, 10)
                .unwrap()
                .and_hms_opt(14, 0, 0)
                .unwrap(),
            "missed".to_string(),
        )];
        let out = format_deadline_chat_layer(&items, now);
        assert!(out.contains("已过 30 分钟"));
        // > 1 hour formats as hours.
        let now2 = NaiveDate::from_ymd_opt(2026, 5, 10)
            .unwrap()
            .and_hms_opt(17, 0, 0)
            .unwrap();
        let out2 = format_deadline_chat_layer(&items, now2);
        assert!(out2.contains("已过 3 小时"));
    }

    #[test]
    fn format_persona_layer_includes_companionship_at_day_zero() {
        let body = format_persona_layer(0, "", "", "", "");
        assert!(body.starts_with("[宠物的长期人格画像]"));
        assert!(body.contains("第一天"));
        // Tail guidance always present so the LLM is told how to use the section.
        assert!(body.contains("自然渗进语气"));
    }

    #[test]
    fn format_persona_layer_includes_persona_when_set() {
        let body = format_persona_layer(30, "我倾向短句，话题偏当下场景。", "", "", "");
        assert!(body.contains("30 天"));
        assert!(body.contains("我倾向短句"));
        assert!(!body.contains("情绪谱"));
    }

    #[test]
    fn format_persona_layer_includes_trend_when_set() {
        let body = format_persona_layer(
            45,
            "",
            "你最近 30 次心情记录里：Tap × 12、Idle × 10。",
            "",
            "",
        );
        assert!(body.contains("45 天"));
        assert!(body.contains("Tap × 12"));
    }

    #[test]
    fn format_persona_layer_includes_all_three_when_present() {
        let body = format_persona_layer(
            120,
            "我倾向短句。",
            "你最近 50 次心情记录里：Tap × 30、Flick × 15。",
            "",
            "",
        );
        assert!(body.contains("120 天"));
        assert!(body.contains("我倾向短句"));
        assert!(body.contains("Tap × 30"));
        // Companionship comes before persona, persona before trend — matches the
        // section ordering chosen for the proactive prompt for visual consistency.
        let p_companionship = body.find("120 天").unwrap();
        let p_persona = body.find("我倾向短句").unwrap();
        let p_trend = body.find("Tap × 30").unwrap();
        assert!(p_companionship < p_persona && p_persona < p_trend);
    }

    #[test]
    fn format_persona_layer_blank_inputs_still_safe() {
        // Whitespace-only persona/trend should be treated as absent — no empty
        // sections injected into the system note.
        let body = format_persona_layer(7, "   \n  ", "\t", "", "");
        assert!(body.contains("7 天"));
        // Body should have header + invite-to-name line (GOAL 055，pet_name 空时常驻)
        // + companionship + tail = 4 sections joined by \n\n.
        let blocks: Vec<&str> = body.split("\n\n").collect();
        assert_eq!(blocks.len(), 4, "unexpected block count: {:#?}", blocks);
    }

    #[test]
    fn format_persona_layer_includes_user_name_when_set() {
        // Iter Cτ: user_name should prepend a "你的主人是「X」" line and sit before
        // the companionship line so the LLM reads "who I'm with" before "how long".
        let body = format_persona_layer(30, "", "", "moon", "");
        assert!(body.contains("你的主人是「moon」"));
        let p_user = body.find("你的主人是").unwrap();
        let p_companion = body.find("30 天").unwrap();
        assert!(p_user < p_companion);
    }

    // GOAL 055 tests
    #[test]
    fn format_persona_layer_includes_pet_name_when_set() {
        let body = format_persona_layer(10, "", "", "", "小冬");
        assert!(body.contains("你的名字是「小冬」"));
        // pet_name 应在 user_name 行前（"你是 X" 早于 "主人是 Y"）
        let p_pet = body.find("你的名字是").unwrap();
        let p_companion = body.find("10 天").unwrap();
        assert!(p_pet < p_companion);
    }

    #[test]
    fn format_persona_layer_invites_naming_when_pet_name_empty() {
        let body = format_persona_layer(5, "", "", "", "");
        // spec「user 还没给你取名时，被问到柔和邀请取名」对应——本 line 常驻空 pet_name 场景
        assert!(body.contains("你还没有名字"));
        assert!(body.contains("主人想叫我什么呀") || body.contains("邀请"));
        // 反指令：不要自己编一个
        assert!(body.contains("不要自己编"));
    }

    #[test]
    fn format_persona_layer_pet_name_and_user_name_both_set() {
        let body = format_persona_layer(20, "", "", "moon", "小冬");
        assert!(body.contains("你的名字是「小冬」"));
        assert!(body.contains("你的主人是「moon」"));
        // pet 名先于 user 名
        let p_pet = body.find("你的名字是").unwrap();
        let p_user = body.find("你的主人是").unwrap();
        assert!(p_pet < p_user);
    }

    #[test]
    fn format_persona_layer_pet_name_whitespace_treated_as_unset() {
        let body = format_persona_layer(0, "", "", "", "   ");
        // 全空白视作未设——走"邀请取名"分支而非注入空 name
        assert!(body.contains("你还没有名字"));
        assert!(!body.contains("你的名字是"));
    }

    #[test]
    fn format_persona_layer_omits_user_name_when_empty() {
        // Whitespace-only user_name treated as absent — no awkward "「  」" line.
        let body = format_persona_layer(30, "", "", "   ", "");
        assert!(!body.contains("你的主人是"));
    }

    #[test]
    fn format_persona_layer_trims_user_name_whitespace() {
        let body = format_persona_layer(30, "", "", "  moon  ", "");
        assert!(body.contains("你的主人是「moon」"));
    }

    #[test]
    fn tool_usage_prompt_teaches_butler_delegation() {
        // Iter Cι: pin the butler_tasks delegation guidance so a future refactor
        // can't silently drop it. Without this section the LLM falls back to
        // verbal-only acknowledgments and the user's "帮我每天 9 点 X" never lands
        // in butler_tasks.
        assert!(
            TOOL_USAGE_PROMPT.contains("butler_tasks"),
            "tool prompt must mention butler_tasks"
        );
        assert!(
            TOOL_USAGE_PROMPT.contains("[every:") && TOOL_USAGE_PROMPT.contains("[once:"),
            "tool prompt must teach the schedule prefixes by example"
        );
        assert!(
            TOOL_USAGE_PROMPT.contains("todo") && TOOL_USAGE_PROMPT.contains("提醒我"),
            "tool prompt must contrast butler_tasks with todo[remind:]"
        );
        // Iter R78: pin the [deadline:] prefix so the LLM creates the right
        // kind of butler entry when user describes their own due-date task.
        // Without this, "之前要..." phrasing collapses into [once:] which
        // implies pet auto-executes — wrong semantics for user-completion items.
        assert!(
            TOOL_USAGE_PROMPT.contains("[deadline:"),
            "tool prompt must teach the deadline prefix by example"
        );
        assert!(
            TOOL_USAGE_PROMPT.contains("user 必须在那之前自己完成")
                || TOOL_USAGE_PROMPT.contains("user 必须在那之前"),
            "tool prompt must contrast [once:] (pet executes) vs [deadline:] (user completes)"
        );
    }

    #[test]
    fn tool_usage_prompt_teaches_user_profile_capture() {
        // Iter Cσ: pin the user_profile capture guidance — symmetric to Cι's
        // butler delegation. Without this the LLM might absorb stable facts
        // verbally and forget them, defeating Iter Cα's user_profile_hint
        // injection (the prompt has nothing to inject if nothing was captured).
        assert!(
            TOOL_USAGE_PROMPT.contains("user_profile"),
            "tool prompt must mention user_profile capture"
        );
        // Test the contrast examples — stable facts vs ephemeral state.
        assert!(
            TOOL_USAGE_PROMPT.contains("不是临时心情") || TOOL_USAGE_PROMPT.contains("临时状态"),
            "tool prompt must contrast stable facts with ephemeral state"
        );
        // Test the dedup guidance — update existing rather than re-create.
        assert!(
            TOOL_USAGE_PROMPT.contains("update") && TOOL_USAGE_PROMPT.contains("相近"),
            "tool prompt must instruct dedup via update for similar entries"
        );
    }

    #[test]
    fn enforce_tool_round_limit_passes_under_max() {
        assert_eq!(enforce_tool_round_limit(0, 8), None);
        assert_eq!(enforce_tool_round_limit(7, 8), None);
    }

    #[test]
    fn enforce_tool_round_limit_aborts_at_or_over_max() {
        let at = enforce_tool_round_limit(8, 8).expect("must abort at limit");
        assert!(at.contains("8"));
        assert!(at.contains("max=8"));

        let over = enforce_tool_round_limit(99, 8).expect("must abort over limit");
        assert!(over.contains("99"));
    }

    #[test]
    fn tool_call_limit_message_is_user_meaningful() {
        // The error surfaces both to app.log and to the frontend stream — must explain
        // *why* the turn stopped, not just "error". Check the key signal words.
        let msg = tool_call_limit_message(8, 8);
        assert!(msg.contains("工具调用循环"), "must name the failure mode");
        assert!(
            msg.contains("已中止") || msg.contains("无限循环"),
            "must signal abort"
        );
        assert!(msg.contains("8"), "must include round count for debug");
    }

    // -- Iter TR1: tool-call purpose gate -----------------------------------------

    #[test]
    fn extract_tool_purpose_returns_some_for_valid_one_liner() {
        let args = r#"{"file_path":"~/.zshrc","purpose":"check shell config"}"#;
        assert_eq!(
            extract_tool_purpose(args),
            Some("check shell config".to_string())
        );
    }

    #[test]
    fn extract_tool_purpose_trims_surrounding_whitespace() {
        let args = r#"{"purpose":"  spaced reason  "}"#;
        assert_eq!(
            extract_tool_purpose(args),
            Some("spaced reason".to_string())
        );
    }

    #[test]
    fn extract_tool_purpose_returns_none_for_missing_field() {
        let args = r#"{"file_path":"foo"}"#;
        assert!(extract_tool_purpose(args).is_none());
    }

    #[test]
    fn extract_tool_purpose_returns_none_for_blank_string() {
        // Empty string and whitespace-only must both fail — accepting them would
        // defeat the protocol (LLMs would game the gate by passing "").
        assert!(extract_tool_purpose(r#"{"purpose":""}"#).is_none());
        assert!(extract_tool_purpose(r#"{"purpose":"   "}"#).is_none());
    }

    #[test]
    fn extract_tool_purpose_returns_none_for_non_string_value() {
        // Numbers, bools, nulls, objects must all fail rather than coerce — the
        // contract is "string sentence", anything else is malformed.
        assert!(extract_tool_purpose(r#"{"purpose":42}"#).is_none());
        assert!(extract_tool_purpose(r#"{"purpose":null}"#).is_none());
        assert!(extract_tool_purpose(r#"{"purpose":true}"#).is_none());
        assert!(extract_tool_purpose(r#"{"purpose":{"x":1}}"#).is_none());
    }

    #[test]
    fn extract_tool_purpose_returns_none_for_unparseable_json() {
        // Garbage args (rare but possible — proxy bug, model misformat) must not panic.
        assert!(extract_tool_purpose("not json").is_none());
        assert!(extract_tool_purpose("").is_none());
    }

    #[test]
    fn missing_purpose_error_result_carries_retry_hint() {
        let r = missing_purpose_error_result();
        // Must be parseable JSON so the LLM's tool-result handler can introspect it.
        let v: serde_json::Value = serde_json::from_str(&r).expect("must be valid JSON");
        assert!(v.get("error").is_some(), "must carry error field");
        let hint = v.get("hint").and_then(|h| h.as_str()).unwrap_or("");
        assert!(hint.contains("purpose"), "hint must name the missing field");
        assert!(hint.contains("重新调用"), "hint must instruct retry");
    }

    #[test]
    fn tool_usage_prompt_teaches_purpose_protocol() {
        // Iter TR1: pin the purpose-protocol guidance — without it the LLM's first
        // tool call after a fresh prompt will be rejected; the gate's recoverable
        // error gets the model to comply, but only if the prompt has set the
        // expectation up front.
        assert!(
            TOOL_USAGE_PROMPT.contains("purpose"),
            "tool prompt must teach purpose convention"
        );
        assert!(
            TOOL_USAGE_PROMPT.contains("强制") || TOOL_USAGE_PROMPT.contains("必须"),
            "tool prompt must signal that purpose is required, not optional"
        );
    }
