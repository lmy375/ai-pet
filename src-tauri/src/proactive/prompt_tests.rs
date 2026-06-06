    use super::*;
    use crate::mood::{MOOD_CATEGORY, MOOD_TITLE};

    fn base_inputs<'a>() -> PromptInputs<'a> {
        PromptInputs {
            time: "2026-05-03 14:30",
            period: "下午",
            // 2026-05-03 is a Sunday — matches the 周末 register tests assert.
            day_of_week: "周日 · 周末",
            idle_minutes: 20,
            // 20 min idle = 用户离开了一小会儿 (16-60 band) — keeps existing tests stable.
            idle_register: "用户离开了一小会儿",
            input_hint: "用户键鼠空闲约 60 秒。",
            cadence_hint: "距上次你主动开口约 8 分钟（刚说过话，话题还热）。",
            mood_hint: "你上次记录的心情/状态：「平静」。",
            wake_hint: "",
            speech_hint: "",
            is_first_mood: false,
            pre_quiet_minutes: None,
            reminders_hint: "",
            plan_hint: "",
            // Default well past the icebreaker threshold so existing tests stay at the
            // base 6 rule count; icebreaker tests bump this down explicitly.
            proactive_history_count: 100,
            // Default 1 = above the first-of-day trigger (Iter Cξ requires == 0)
            // and still below the default chatty_day_threshold of 5 — keeps existing
            // tests neutral on both rules. Tests for either bump this explicitly.
            today_speech_count: 1,
            // Mirrors the production default (5) — keeps existing tests stable while
            // letting chatty-day tests assert behavior at exact threshold boundary.
            chatty_day_threshold: 5,
            // Default 0/0 = below ENV_AWARENESS_MIN_SAMPLES → rule won't fire. Tests that
            // exercise the corrective rule bump these explicitly.
            env_spoke_total: 0,
            env_spoke_with_any: 0,
            // Default Some(8) — well below LONG_IDLE_MINUTES, matches the cadence_hint
            // string ("约 8 分钟（刚说过话，话题还热）"). Tests for long-idle bump this
            // above 60 explicitly.
            since_last_proactive_minutes: Some(8),
            // Default 5 — past day-0 special framing, but not a milestone (30 / 100 /
            // 365 / etc.) so the Iter Cρ companionship-milestone rule stays silent
            // by default. Tests that need either the day-0 framing or a milestone
            // set this explicitly.
            companionship_days: 5,
            // Default empty — base inputs simulate the pre-Iter 102 state where no
            // persona summary has been written yet. Tests for the persona hint set this
            // to a non-empty string to assert injection.
            persona_hint: "",
            // Default empty — pre-Iter 103 state, no mood trend yet. Tests for the
            // trend hint set this explicitly.
            mood_trend_hint: "",
            // Default empty — pre-Iter Cα state, no user_profile entries surfaced.
            // Tests for the user-profile hint set this explicitly.
            user_profile_hint: "",
            // Default empty — pre-Iter Cγ state, no owner-assigned butler tasks yet.
            // Tests for the butler_tasks hint set this explicitly.
            butler_tasks_hint: "",
            // 默认空：心跳测试需要时显式覆盖。
            task_heartbeat_hint: "",
            // 默认空：报喜测试需要时显式覆盖。
            task_completion_hint: "",
            // 默认空：rolling 24h 完成清单测试需要时显式覆盖。
            recent_completion_hint: "",
            // Default empty — pre-Iter Cυ state, no owner name set in settings.
            // Tests for the user_name line set this explicitly.
            user_name: "",
            // GOAL 055: default empty — pet 还没取名。Tests for the pet_name
            // line / "invite to name" line set this explicitly.
            pet_name: "",
            // Default empty — pre-Iter R1 state, no feedback log yet. Tests for
            // the feedback hint set this explicitly.
            feedback_hint: "",
            feedback_aggregate_hint: "",
            consecutive_silent_hint: "",
            consecutive_negative_hint: "",
            transient_note_hint: "",
            // Default 14 (afternoon) — well outside the late-night-wellness window
            // (hours 0..LATE_NIGHT_END_HOUR=4) so existing tests stay neutral.
            // Tests for the late-night rule set this to 0/1/2/3 explicitly.
            hour: 14,
            // Default false — pre-Iter R8 gate state. Tests for the rate limit
            // bump this to true to verify suppression.
            recently_fired_wellness: false,
            // Default empty — pre-Iter R11 state, no detected redundancy.
            // Tests for the topic-repeat rule set this explicitly.
            repeated_topic_hint: "",
            // Default empty — pre-Iter R14 state, no cross-day hint. Tests
            // exercising the first-of-day continuity layer set this explicitly.
            cross_day_hint: "",
            yesterday_recap_hint: "",
            length_register_hint: "",
            // Default empty — pre-Iter R77 state, no deadline-prefixed butler tasks.
            deadline_hint: "",
        }
    }

    #[test]
    fn prompt_includes_required_sections() {
        let p = build_proactive_prompt(&base_inputs());
        assert!(p.starts_with("[系统提示·主动开口检查]"));
        assert!(p.contains("2026-05-03 14:30"));
        assert!(p.contains("下午"));
        assert!(p.contains("20 分钟"));
        assert!(p.contains("用户键鼠空闲约 60 秒"));
        assert!(p.contains("刚说过话"));
        assert!(p.contains("「平静」"));
        assert!(p.contains("约束："));
        assert!(p.contains("[motion: Tap]"));
    }

    #[test]
    fn empty_optional_hints_skip_their_lines() {
        let p = build_proactive_prompt(&base_inputs());
        // None of the conditional hint markers should appear when their inputs are blank.
        assert!(!p.contains("Focus 模式"));
        assert!(!p.contains("从休眠唤醒"));
        assert!(!p.contains("最近主动说过的几句话"));
        // And no leading/trailing/double blank from skipped sections — the focus point
        // is that join("\n") doesn't produce stray empty lines for skipped optionals.
        assert!(!p.contains("\n\n\n"));
    }

    #[test]
    fn wake_hint_renders_when_provided() {
        let mut inputs = base_inputs();
        inputs.wake_hint = "（用户的电脑在大约 60 秒前刚从休眠唤醒。）";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("从休眠唤醒"));
    }

    #[test]
    fn speech_hint_with_bullets_passes_through_verbatim() {
        let mut inputs = base_inputs();
        let bullets =
            "你最近主动说过的几句话（旧→新），开口前看一眼避免重复：\n· 早上好啊\n· 加油码代码";
        inputs.speech_hint = bullets;
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("早上好啊"));
        assert!(p.contains("加油码代码"));
    }

    #[test]
    fn mood_category_and_title_interpolated() {
        let p = build_proactive_prompt(&base_inputs());
        assert!(p.contains(MOOD_CATEGORY));
        assert!(p.contains(MOOD_TITLE));
    }

    // ---- proactive_rules ----

    #[test]
    fn rules_count_and_format() {
        let rules = proactive_rules(&base_inputs());
        assert_eq!(rules.len(), 6, "ladder change → update count + add a test");
        // Every rule is a bullet starting with "- ".
        for r in &rules {
            assert!(r.starts_with("- "), "rule must be a bullet: {:?}", r);
        }
    }

    #[test]
    fn rules_interpolate_constants() {
        let rules = proactive_rules(&base_inputs());
        let joined = rules.join("\n");
        assert!(joined.contains(SILENT_MARKER));
        assert!(joined.contains(MOOD_CATEGORY));
        assert!(joined.contains(MOOD_TITLE));
        // The motion-tag enumeration is part of the last rule and must remain there.
        for tag in ["Tap", "Flick", "Flick3", "Idle"] {
            assert!(joined.contains(tag), "missing motion tag: {}", tag);
        }
    }

    #[test]
    fn rules_appear_in_full_prompt() {
        let p = build_proactive_prompt(&base_inputs());
        let rules = proactive_rules(&base_inputs());
        for r in &rules {
            assert!(p.contains(r.as_str()), "rule missing from prompt: {:?}", r);
        }
    }

    // ---- context-driven rule additions ----

    #[test]
    fn wake_rule_appears_when_wake_hint_present() {
        let mut inputs = base_inputs();
        inputs.wake_hint = "（用户的电脑在大约 60 秒前刚从休眠唤醒。）";
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 7, "base 6 + 1 wake-context rule");
        assert!(rules.iter().any(|r| r.contains("用户刚从离开桌子回来")));
    }

    #[test]
    fn first_mood_rule_appears_when_flagged() {
        let mut inputs = base_inputs();
        inputs.is_first_mood = true;
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 7, "base 6 + 1 first-mood rule");
        assert!(rules.iter().any(|r| r.contains("memory_edit create")));
    }

    #[test]
    fn both_context_rules_can_coexist() {
        let mut inputs = base_inputs();
        inputs.wake_hint = "唤醒提示";
        inputs.is_first_mood = true;
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 8, "base 6 + 2 contextual rules");
    }

    #[test]
    fn no_context_rules_with_default_inputs() {
        let rules = proactive_rules(&base_inputs());
        assert_eq!(
            rules.len(),
            6,
            "no context-driven rules without their flags"
        );
    }

    #[test]
    fn reminders_rule_appears_when_hint_present() {
        let mut inputs = base_inputs();
        let bullet_text =
            "你有以下到期的用户提醒（请挑最相关的一条带进开口）：\n· 23:00 吃药（条目标题: meds）";
        inputs.reminders_hint = bullet_text;
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 7, "base 6 + 1 reminders rule");
        // v12: 提示已从 memory_edit delete 切到 todo_edit (action=delete)
        assert!(rules.iter().any(|r| r.contains("todo_edit")));
    }

    #[test]
    fn plan_rule_appears_when_hint_present() {
        let mut inputs = base_inputs();
        inputs.plan_hint = "你今天的小目标 / 计划：\n· 关心用户工作进展 [0/2]";
        let rules = proactive_rules(&inputs);
        assert!(rules.iter().any(|r| r.contains("你有今日计划在执行中")));
    }

    #[test]
    fn icebreaker_rule_appears_when_count_under_three() {
        let mut inputs = base_inputs();
        inputs.proactive_history_count = 0;
        let rules = proactive_rules(&inputs);
        assert!(rules.iter().any(|r| r.contains("你和用户还不熟")));
        // Must include the actual count so the LLM has a sense of where on the curve.
        assert!(rules.iter().any(|r| r.contains("0 次")));
    }

    #[test]
    fn icebreaker_rule_absent_at_threshold() {
        let mut inputs = base_inputs();
        inputs.proactive_history_count = 3;
        let rules = proactive_rules(&inputs);
        assert!(!rules.iter().any(|r| r.contains("你和用户还不熟")));
    }

    #[test]
    fn chatty_day_rule_appears_at_or_above_threshold() {
        let mut inputs = base_inputs();
        inputs.today_speech_count = inputs.chatty_day_threshold;
        let rules = proactive_rules(&inputs);
        assert!(rules.iter().any(|r| r.contains("今天已经聊了不少")));
        // The actual count must surface so the LLM knows where on the curve it sits.
        let count_str = format!("{} 次", inputs.chatty_day_threshold);
        assert!(rules.iter().any(|r| r.contains(&count_str)));
    }

    #[test]
    fn chatty_day_rule_absent_below_threshold() {
        let mut inputs = base_inputs();
        inputs.today_speech_count = inputs.chatty_day_threshold - 1;
        let rules = proactive_rules(&inputs);
        assert!(!rules.iter().any(|r| r.contains("今天已经聊了不少")));
    }

    #[test]
    fn chatty_day_rule_disabled_when_threshold_zero() {
        // threshold == 0 means user opted out of the chatty-day nudge entirely; the rule
        // must not fire even when today_speech_count is sky-high.
        let mut inputs = base_inputs();
        inputs.chatty_day_threshold = 0;
        inputs.today_speech_count = 9999;
        let rules = proactive_rules(&inputs);
        assert!(!rules.iter().any(|r| r.contains("今天已经聊了不少")));
    }

    #[test]
    fn env_awareness_low_below_min_samples_returns_false() {
        // Even 0/9 (which would be 0%) doesn't fire because we don't trust the ratio yet.
        assert!(!env_awareness_low(9, 0));
        assert!(!env_awareness_low(ENV_AWARENESS_MIN_SAMPLES - 1, 0));
    }

    #[test]
    fn env_awareness_low_at_threshold_strict_inequality() {
        // Exactly 30% (3/10) → not low; just under (2/10 = 20%) → low.
        assert!(!env_awareness_low(10, 3));
        assert!(env_awareness_low(10, 2));
    }

    #[test]
    fn env_awareness_low_at_full_coverage_returns_false() {
        // 100% coverage definitely shouldn't trigger the prod.
        assert!(!env_awareness_low(20, 20));
    }

    #[test]
    fn env_awareness_corrective_rule_appears_when_low() {
        let mut inputs = base_inputs();
        inputs.env_spoke_total = 12;
        inputs.env_spoke_with_any = 2; // ~16%
        let rules = proactive_rules(&inputs);
        assert!(rules
            .iter()
            .any(|r| r.contains("最近你开口前几乎都没看环境")));
        // Includes the actual numbers + threshold so the LLM has concrete signal.
        assert!(rules.iter().any(|r| r.contains("12 次")));
        assert!(rules.iter().any(|r| r.contains("get_active_window")));
    }

    #[test]
    fn active_data_driven_rule_labels_empty_in_neutral_state() {
        // Past icebreaker, below chatty threshold, no env-awareness data.
        assert!(active_data_driven_rule_labels(100, 0, 5, 0, 0, 0).is_empty());
    }

    #[test]
    fn active_data_driven_rule_labels_picks_up_each_rule_independently() {
        // Only icebreaker.
        assert_eq!(
            active_data_driven_rule_labels(0, 0, 5, 0, 0, 0),
            vec!["icebreaker"],
        );
        // Only chatty.
        assert_eq!(
            active_data_driven_rule_labels(100, 5, 5, 0, 0, 0),
            vec!["chatty"],
        );
        // Only env-awareness.
        assert_eq!(
            active_data_driven_rule_labels(100, 0, 5, 12, 2, 0),
            vec!["env-awareness"],
        );
    }

    #[test]
    fn active_data_driven_rule_labels_combine_in_firing_order() {
        // All three at once: should appear in the same order proactive_rules pushes them.
        let labels = active_data_driven_rule_labels(0, 6, 5, 12, 1, 0);
        assert_eq!(labels, vec!["icebreaker", "chatty", "env-awareness"]);
    }

    #[test]
    fn active_data_driven_rule_labels_zero_threshold_disables_chatty() {
        // chatty threshold == 0 means the user opted out — even today_count=99 shouldn't
        // surface the chatty label.
        assert!(active_data_driven_rule_labels(100, 99, 0, 0, 0, 0).is_empty());
    }

    #[test]
    fn active_environmental_rule_labels_empty_when_all_false() {
        assert!(
            active_environmental_rule_labels(false, false, false, false, false, false).is_empty()
        );
    }

    #[test]
    fn active_environmental_rule_labels_picks_each_independently() {
        assert_eq!(
            active_environmental_rule_labels(true, false, false, false, false, false),
            vec!["wake-back"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, true, false, false, false, false),
            vec!["first-mood"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, false, true, false, false, false),
            vec!["pre-quiet"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, false, false, true, false, false),
            vec!["reminders"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, false, false, false, true, false),
            vec!["plan"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, false, false, false, false, true),
            vec!["first-of-day"],
        );
    }

    #[test]
    fn companionship_milestone_fixed_thresholds() {
        assert_eq!(companionship_milestone(7), Some("刚好一周"));
        assert_eq!(companionship_milestone(30), Some("满一个月"));
        assert_eq!(companionship_milestone(100), Some("百日纪念"));
        assert_eq!(companionship_milestone(180), Some("满半年"));
        assert_eq!(companionship_milestone(365), Some("满一年"));
    }

    #[test]
    fn companionship_milestone_returns_none_for_non_milestones() {
        assert_eq!(companionship_milestone(0), None);
        assert_eq!(companionship_milestone(1), None);
        assert_eq!(companionship_milestone(6), None);
        assert_eq!(companionship_milestone(8), None);
        assert_eq!(companionship_milestone(29), None);
        assert_eq!(companionship_milestone(31), None);
        assert_eq!(companionship_milestone(99), None);
        assert_eq!(companionship_milestone(364), None);
        assert_eq!(companionship_milestone(366), None);
    }

    #[test]
    fn companionship_milestone_yearly_after_first_year() {
        assert_eq!(companionship_milestone(730), Some("又一个周年"));
        assert_eq!(companionship_milestone(1095), Some("又一个周年"));
        assert_eq!(companionship_milestone(1460), Some("又一个周年"));
        // Off-by-one days near anniversaries should not fire.
        assert_eq!(companionship_milestone(729), None);
        assert_eq!(companionship_milestone(731), None);
    }

    #[test]
    fn companionship_milestone_rule_fires_through_proactive_rules() {
        let mut inputs = base_inputs();
        inputs.companionship_days = 100;
        let rules = proactive_rules(&inputs);
        assert!(rules.iter().any(|r| r.contains("百日纪念")));
        assert!(rules.iter().any(|r| r.contains("今天是和用户相处的")));

        inputs.companionship_days = 5; // not a milestone
        let rules = proactive_rules(&inputs);
        assert!(!rules.iter().any(|r| r.contains("今天是和用户相处的")));
    }

    #[test]
    fn effective_awaiting_clears_when_raw_false() {
        // Raw flag false → always false regardless of timing.
        assert!(!effective_awaiting(false, Some(0)));
        assert!(!effective_awaiting(false, Some(99999)));
        assert!(!effective_awaiting(false, None));
    }

    #[test]
    fn effective_awaiting_honors_recent_raw() {
        // Raw flag true and pet spoke recently → gate fires.
        assert!(effective_awaiting(true, Some(0)));
        assert!(effective_awaiting(true, Some(60)));
        assert!(effective_awaiting(
            true,
            Some(AWAITING_AUTO_CLEAR_SECONDS - 1)
        ));
    }

    #[test]
    fn effective_awaiting_expires_after_threshold() {
        // Raw flag true but pet spoke long ago → gate falls off.
        assert!(!effective_awaiting(true, Some(AWAITING_AUTO_CLEAR_SECONDS)));
        assert!(!effective_awaiting(
            true,
            Some(AWAITING_AUTO_CLEAR_SECONDS + 1)
        ));
        assert!(!effective_awaiting(true, Some(99999)));
    }

    #[test]
    fn effective_awaiting_none_since_proactive_does_not_fire() {
        // Defensive: raw flag set but no recorded last_proactive — treat as
        // "shouldn't be awaiting" (mark_proactive_spoken sets both atomically,
        // so this state shouldn't happen, but we don't panic if it does).
        assert!(!effective_awaiting(true, None));
    }

    #[test]
    fn prompt_includes_user_name_line_when_set() {
        // Iter Cυ: when settings.user_name is non-empty, the proactive prompt
        // gets a "你的主人是「X」" line right after the companionship line so
        // the LLM can address the owner by name.
        let mut inputs = base_inputs();
        inputs.user_name = "moon";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("你的主人是「moon」"));
        // Ordering: companionship line precedes user_name line precedes the
        // optional persona/profile blocks (those are empty in base_inputs).
        let p_companion = p.find("一起走过").unwrap();
        let p_user = p.find("你的主人是").unwrap();
        assert!(p_companion < p_user, "companionship before user_name");
    }

    // GOAL 055 pet_name tests
    #[test]
    fn prompt_includes_pet_name_line_when_set() {
        let mut inputs = base_inputs();
        inputs.pet_name = "小冬";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("你的名字是「小冬」"));
        // 反指令：self-reference / 自我介绍 用此名
        assert!(p.contains("self-reference") || p.contains("自我介绍"));
    }

    #[test]
    fn prompt_invites_naming_when_pet_name_empty() {
        // pet_name = "" → 注入"邀请取名"提示
        let inputs = base_inputs();
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("你还没有名字"));
        assert!(p.contains("不要自己编"));
    }

    #[test]
    fn prompt_pet_name_whitespace_treated_as_unset() {
        let mut inputs = base_inputs();
        inputs.pet_name = "   ";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("你还没有名字"));
        assert!(!p.contains("你的名字是"));
    }

    #[test]
    fn prompt_omits_user_name_line_when_empty_or_whitespace() {
        let inputs = base_inputs(); // default user_name = ""
        let p = build_proactive_prompt(&inputs);
        assert!(!p.contains("你的主人是"));

        let mut inputs2 = base_inputs();
        inputs2.user_name = "   \t  ";
        let p2 = build_proactive_prompt(&inputs2);
        assert!(
            !p2.contains("你的主人是"),
            "whitespace-only must be skipped"
        );
    }

    #[test]
    fn prompt_trims_user_name_whitespace() {
        let mut inputs = base_inputs();
        inputs.user_name = "  moon  ";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("你的主人是「moon」"));
        assert!(!p.contains("「  moon"));
    }

    #[test]
    fn first_of_day_only_fires_when_today_count_is_zero() {
        // Iter Cξ: first-of-day is wired in proactive_rules from
        // `inputs.today_speech_count == 0`. Pin the integration here so a future
        // refactor (e.g., changing how the env_labels callsite reads the count)
        // doesn't silently break the trigger.
        let mut inputs = base_inputs();
        inputs.today_speech_count = 0;
        let rules_zero = proactive_rules(&inputs);
        assert!(rules_zero.iter().any(|r| r.contains("今天的第一次开口")));

        inputs.today_speech_count = 1;
        let rules_one = proactive_rules(&inputs);
        assert!(!rules_one.iter().any(|r| r.contains("今天的第一次开口")));
    }

    #[test]
    fn active_environmental_rule_labels_combine_in_firing_order() {
        let labels = active_environmental_rule_labels(true, true, true, true, true, true);
        // Iter Cξ: first-of-day slots between first-mood and pre-quiet so the
        // greeting register comes after mood bootstrap but before quiet-hours
        // wind-down.
        assert_eq!(
            labels,
            vec![
                "wake-back",
                "first-mood",
                "first-of-day",
                "pre-quiet",
                "reminders",
                "plan"
            ],
        );
    }

    #[test]
    fn format_companionship_line_day_zero_uses_first_day_framing() {
        let line = format_companionship_line(0);
        assert!(line.contains("第一天"));
        assert!(line.contains("初识"));
    }

    #[test]
    fn format_companionship_line_after_day_zero_states_count() {
        let line = format_companionship_line(42);
        assert!(line.contains("42 天"));
        // Mentions familiarity / shared time so the LLM is invited to use it.
        assert!(line.contains("相处时长"));
    }

    #[test]
    fn prompt_includes_companionship_line() {
        let mut inputs = base_inputs();
        inputs.companionship_days = 7;
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("7 天"));
    }

    #[test]
    fn prompt_includes_first_day_framing_at_day_zero() {
        let mut inputs = base_inputs();
        inputs.companionship_days = 0;
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("第一天"));
    }

    #[test]
    fn prompt_includes_persona_hint_when_set() {
        let mut inputs = base_inputs();
        inputs.persona_hint =
            "你最近一次自我反思的画像（来自 consolidate）：\n我倾向短句，话题偏当下场景。";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("自我反思的画像"));
        assert!(p.contains("我倾向短句"));
    }

    #[test]
    fn prompt_omits_persona_hint_when_empty() {
        let inputs = base_inputs(); // default persona_hint = ""
        let p = build_proactive_prompt(&inputs);
        assert!(!p.contains("自我反思的画像"));
    }

    #[test]
    fn prompt_includes_mood_trend_hint_when_set() {
        let mut inputs = base_inputs();
        inputs.mood_trend_hint =
            "你最近 30 次心情记录里：Tap × 12、Idle × 10、Flick × 5（按出现次数排序）。这是你长期的情绪谱——可以让 ta 渗进当下语气，但不必生硬带出。";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("长期的情绪谱"));
        assert!(p.contains("Tap × 12"));
    }

    #[test]
    fn prompt_omits_mood_trend_hint_when_empty() {
        let inputs = base_inputs(); // default mood_trend_hint = ""
        let p = build_proactive_prompt(&inputs);
        assert!(!p.contains("长期的情绪谱"));
    }

    #[test]
    fn prompt_includes_user_profile_hint_when_set() {
        let mut inputs = base_inputs();
        inputs.user_profile_hint =
            "你了解的用户习惯（来自 user_profile 记忆，最新 2 条）：\n- 起床时间：通常 8:30 起床\n- 喜欢：偏好 dark theme 编辑器";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("用户习惯"));
        assert!(p.contains("起床时间"));
        assert!(p.contains("dark theme"));
    }

    #[test]
    fn prompt_omits_user_profile_hint_when_empty() {
        let inputs = base_inputs(); // default user_profile_hint = ""
        let p = build_proactive_prompt(&inputs);
        assert!(!p.contains("用户习惯"));
    }

    #[test]
    fn format_user_profile_block_empty_returns_empty() {
        assert_eq!(
            format_user_profile_block(&[], 6, 80),
            String::new(),
            "no items → no block",
        );
    }

    #[test]
    fn format_user_profile_block_zero_max_returns_empty() {
        let items = vec![(
            "habit".into(),
            "desc".into(),
            "2026-05-03T10:00:00+08:00".into(),
        )];
        assert_eq!(format_user_profile_block(&items, 0, 80), String::new());
    }

    #[test]
    fn format_user_profile_block_sorts_by_updated_at_desc() {
        let items = vec![
            (
                "旧习惯".into(),
                "desc-a".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
            (
                "新习惯".into(),
                "desc-b".into(),
                "2026-05-03T10:00:00+08:00".into(),
            ),
            (
                "中习惯".into(),
                "desc-c".into(),
                "2026-04-20T10:00:00+08:00".into(),
            ),
        ];
        let out = format_user_profile_block(&items, 6, 80);
        let new_idx = out.find("新习惯").unwrap();
        let mid_idx = out.find("中习惯").unwrap();
        let old_idx = out.find("旧习惯").unwrap();
        assert!(new_idx < mid_idx, "newest should be first");
        assert!(mid_idx < old_idx, "middle should precede oldest");
    }

    #[test]
    fn format_user_profile_block_caps_item_count() {
        let items: Vec<(String, String, String)> = (0..10)
            .map(|i| {
                (
                    format!("habit-{i}"),
                    format!("desc-{i}"),
                    format!("2026-05-{:02}T10:00:00+08:00", i + 1),
                )
            })
            .collect();
        let out = format_user_profile_block(&items, 3, 80);
        assert!(out.contains("最新 3 条"));
        // Top-3 by date = day 10, 9, 8 (i = 9, 8, 7).
        assert!(out.contains("habit-9"));
        assert!(out.contains("habit-8"));
        assert!(out.contains("habit-7"));
        assert!(!out.contains("habit-6"), "4th-newest should be excluded");
    }

    #[test]
    fn format_user_profile_block_truncates_long_descriptions() {
        let long_desc: String = "字".repeat(120);
        let items = vec![(
            "habit".into(),
            long_desc,
            "2026-05-03T10:00:00+08:00".into(),
        )];
        let out = format_user_profile_block(&items, 6, 20);
        assert!(out.contains("…"), "should append ellipsis when truncated");
        // Body line should be 1 title + ：+ 20 chars + … = well under the 120.
        let body_chars = out
            .lines()
            .nth(1)
            .unwrap()
            .chars()
            .filter(|c| *c == '字')
            .count();
        assert_eq!(
            body_chars, 20,
            "should keep exactly 20 chars before ellipsis"
        );
    }

    #[test]
    fn prompt_includes_butler_tasks_hint_when_set() {
        let mut inputs = base_inputs();
        inputs.butler_tasks_hint =
            "用户委托给你的管家任务（共 1 条，按最早委托排在前）：\n- 早报：每天 8 点把日历发给我";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("管家任务"));
        assert!(p.contains("早报"));
        assert!(p.contains("把日历发给我"));
    }

    #[test]
    fn prompt_omits_butler_tasks_hint_when_empty() {
        let inputs = base_inputs();
        let p = build_proactive_prompt(&inputs);
        assert!(!p.contains("管家任务"));
    }

    #[test]
    fn prompt_includes_day_of_week_in_time_line() {
        let mut inputs = base_inputs();
        inputs.day_of_week = "周一 · 工作日";
        let p = build_proactive_prompt(&inputs);
        // Time line is the first non-header line; assert the new label is right
        // after the period.
        assert!(p.contains("（下午，周一 · 工作日）"));
    }

    // user_absence_tier tests moved to proactive/time_helpers.rs (Iter QG5c-prep).

    #[test]
    fn prompt_includes_idle_register_in_time_line() {
        let mut inputs = base_inputs();
        inputs.idle_register = "用户走开有一两小时了";
        inputs.idle_minutes = 90;
        let p = build_proactive_prompt(&inputs);
        // The register sits inside the parenthetical right after the minute count.
        assert!(p.contains("约 90 分钟（用户走开有一两小时了）"));
    }

    // weekday_zh / weekday_kind_zh / format_day_of_week_hint tests moved to
    // proactive/time_helpers.rs (Iter QG5c-prep).

    #[test]
    fn format_user_profile_block_no_truncation_when_under_cap() {
        let items = vec![(
            "habit".into(),
            "短描述".into(),
            "2026-05-03T10:00:00+08:00".into(),
        )];
        let out = format_user_profile_block(&items, 6, 80);
        assert!(out.contains("- habit：短描述"));
        assert!(!out.contains("…"));
    }

    #[test]
    fn active_composite_rule_labels_engagement_window_requires_both_signals() {
        // engagement-window: wake_back AND has_plan.
        // long-idle-no-restraint: long_idle (None == long-idle) AND under_chatty AND !pre_quiet.
        // Default for long-idle inputs: Some(8) (short idle), today=0, threshold=5, pre_quiet=false.
        // idle_min = 0 → long-absence-reunion silent throughout this test.
        // hour = 14 (afternoon) → late-night-wellness silent throughout.
        let short_idle = Some(8u64);
        assert!(
            active_composite_rule_labels(false, false, short_idle, 0, 5, false, 0, 14, false)
                .is_empty()
        );
        assert!(
            active_composite_rule_labels(true, false, short_idle, 0, 5, false, 0, 14, false)
                .is_empty()
        );
        assert!(
            active_composite_rule_labels(false, true, short_idle, 0, 5, false, 0, 14, false)
                .is_empty()
        );
        assert_eq!(
            active_composite_rule_labels(true, true, short_idle, 0, 5, false, 0, 14, false),
            vec!["engagement-window"],
        );
    }

    #[test]
    fn active_composite_rule_labels_long_idle_requires_three_signals() {
        // hour = 14 (afternoon) keeps late-night-wellness silent throughout.
        // long_idle yes, under_chatty yes, !pre_quiet yes → fires.
        assert_eq!(
            active_composite_rule_labels(
                false,
                false,
                Some(LONG_IDLE_MINUTES),
                0,
                5,
                false,
                0,
                14,
                false
            ),
            vec!["long-idle-no-restraint"],
        );
        // None (never spoken) is treated as long-idle.
        assert_eq!(
            active_composite_rule_labels(false, false, None, 0, 5, false, 0, 14, false),
            vec!["long-idle-no-restraint"],
        );
        // Short idle (< threshold) → no fire.
        assert!(active_composite_rule_labels(
            false,
            false,
            Some(LONG_IDLE_MINUTES - 1),
            0,
            5,
            false,
            0,
            14,
            false,
        )
        .is_empty());
        // chatty (today >= threshold) → no fire.
        assert!(
            active_composite_rule_labels(false, false, Some(120), 5, 5, false, 0, 14, false)
                .is_empty()
        );
        // pre_quiet active → no fire.
        assert!(
            active_composite_rule_labels(false, false, Some(120), 0, 5, true, 0, 14, false)
                .is_empty()
        );
        // Threshold == 0 disables chatty gate, so under_chatty is always true.
        assert_eq!(
            active_composite_rule_labels(false, false, Some(120), 9999, 0, false, 0, 14, false),
            vec!["long-idle-no-restraint"],
        );
    }

    #[test]
    fn active_composite_rule_labels_both_can_fire_together() {
        // wake_back + has_plan + long_idle + under_chatty + !pre_quiet → both labels.
        // hour = 14 keeps late-night-wellness silent.
        let labels = active_composite_rule_labels(
            true,
            true,
            Some(LONG_IDLE_MINUTES),
            0,
            5,
            false,
            0,
            14,
            false,
        );
        assert_eq!(labels, vec!["engagement-window", "long-idle-no-restraint"]);
    }

    #[test]
    fn active_composite_rule_labels_long_absence_reunion_gates() {
        // hour = 14 (afternoon) → late-night-wellness silent throughout. idle is
        // already large enough that the late-night idle<5 gate would never fire.
        // Threshold boundary — at threshold + under_chatty + !pre_quiet → fires.
        assert_eq!(
            active_composite_rule_labels(
                false,
                false,
                Some(8),
                0,
                5,
                false,
                LONG_ABSENCE_MINUTES,
                14,
                false,
            ),
            vec!["long-absence-reunion"],
        );
        // Just below threshold → silent.
        assert!(active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            false,
            LONG_ABSENCE_MINUTES - 1,
            14,
            false,
        )
        .is_empty());
        // chatty active → silent (don't pile reunion onto a chatty day).
        assert!(active_composite_rule_labels(
            false,
            false,
            Some(8),
            5,
            5,
            false,
            LONG_ABSENCE_MINUTES + 60,
            14,
            false,
        )
        .is_empty());
        // pre_quiet active → silent (don't open new register before quiet hours).
        assert!(active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            true,
            LONG_ABSENCE_MINUTES + 60,
            14,
            false,
        )
        .is_empty());
    }

    // -- Iter R83: extreme-absence-reunion mutual-exclusion -----------------

    #[test]
    fn active_composite_rule_labels_extreme_absence_reunion_at_24h_threshold() {
        // At EXTREME_ABSENCE_MINUTES exactly → extreme fires, long is suppressed.
        let labels = active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            false,
            EXTREME_ABSENCE_MINUTES,
            14,
            false,
        );
        assert_eq!(labels, vec!["extreme-absence-reunion"]);
        assert!(!labels.contains(&"long-absence-reunion"));
    }

    #[test]
    fn active_composite_rule_labels_extreme_absence_reunion_well_past_24h() {
        // Days-long absence still resolves to extreme, not duplicated.
        let labels = active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            false,
            EXTREME_ABSENCE_MINUTES * 3,
            14,
            false,
        );
        assert_eq!(labels, vec!["extreme-absence-reunion"]);
    }

    #[test]
    fn active_composite_rule_labels_just_below_24h_stays_long_absence() {
        // EXTREME_ABSENCE_MINUTES - 1 → long fires, not extreme. Boundary
        // pinned the same way Cν's threshold test pins LONG_ABSENCE_MINUTES.
        let labels = active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            false,
            EXTREME_ABSENCE_MINUTES - 1,
            14,
            false,
        );
        assert_eq!(labels, vec!["long-absence-reunion"]);
    }

    #[test]
    fn active_composite_rule_labels_extreme_absence_still_gated_by_chatty() {
        // Same gates as Cν: chatty active → both reunion variants silent.
        assert!(active_composite_rule_labels(
            false,
            false,
            Some(8),
            5,
            5,
            false,
            EXTREME_ABSENCE_MINUTES + 60,
            14,
            false,
        )
        .is_empty());
    }

    #[test]
    fn active_composite_rule_labels_late_night_wellness_gating() {
        // Iter R3: late-night-wellness fires iff hour < LATE_NIGHT_END_HOUR (4)
        // AND idle_minutes < LATE_NIGHT_ACTIVE_MAX_IDLE_MIN (5).
        // Pure name → no other signal interferes; pass neutral defaults.
        // Hours 0,1,2,3 with idle=0 should fire.
        for h in 0u8..LATE_NIGHT_END_HOUR {
            let labels =
                active_composite_rule_labels(false, false, Some(8), 0, 5, false, 0, h, false);
            assert!(
                labels.contains(&"late-night-wellness"),
                "hour={} idle=0 should trigger late-night-wellness",
                h,
            );
        }
        // Hour 4 (boundary off) → silent.
        let labels = active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            false,
            0,
            LATE_NIGHT_END_HOUR,
            false,
        );
        assert!(!labels.contains(&"late-night-wellness"));
        // hour=2 but idle=5 (boundary off) → silent.
        let labels = active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            false,
            LATE_NIGHT_ACTIVE_MAX_IDLE_MIN,
            2,
            false,
        );
        assert!(!labels.contains(&"late-night-wellness"));
        // hour=2 + idle=4 (in band) → fires regardless of chatty / pre_quiet.
        // This rule deliberately bypasses both gates because user health > cadence.
        let labels = active_composite_rule_labels(false, false, Some(8), 999, 5, true, 4, 2, false);
        assert!(labels.contains(&"late-night-wellness"));
    }

    #[test]
    fn late_night_wellness_recently_fired_at_gates_window() {
        // Iter R8: pure decider for the rate-limit window. None last → never recent;
        // last < gap → recent (suppress); last == gap → not recent (allow);
        // last > gap → not recent.
        let now = std::time::Instant::now();
        assert!(
            !late_night_wellness_recently_fired_at(None, now, 1800),
            "no prior fire = always allow"
        );
        let recent = now - std::time::Duration::from_secs(900); // 15 min ago
        assert!(
            late_night_wellness_recently_fired_at(Some(recent), now, 1800),
            "15 min < 30 min gap → suppress"
        );
        let exactly_at_gap = now - std::time::Duration::from_secs(1800);
        assert!(
            !late_night_wellness_recently_fired_at(Some(exactly_at_gap), now, 1800),
            "boundary off — exactly at gap is allowed (cooldown elapsed)"
        );
        let beyond_gap = now - std::time::Duration::from_secs(3600);
        assert!(
            !late_night_wellness_recently_fired_at(Some(beyond_gap), now, 1800),
            "1h > 30 min → cooldown elapsed"
        );
    }

    #[test]
    fn active_composite_rule_labels_late_night_wellness_suppressed_in_cooldown() {
        // Iter R8: when the rate limit is in effect, the rule must not fire even
        // though hour + idle satisfy the trigger.
        let labels = active_composite_rule_labels(false, false, Some(8), 0, 5, false, 0, 2, true);
        assert!(
            !labels.contains(&"late-night-wellness"),
            "recently_fired_wellness=true should suppress"
        );
    }

    #[test]
    fn active_composite_rule_labels_three_can_coexist() {
        // wake_back + has_plan + long_idle + under_chatty + !pre_quiet + long_absence
        // → all three labels emit in order: engagement-window, long-idle-no-restraint,
        //   long-absence-reunion.
        let labels = active_composite_rule_labels(
            true,
            true,
            Some(LONG_IDLE_MINUTES),
            0,
            5,
            false,
            LONG_ABSENCE_MINUTES + 60,
            14,
            false,
        );
        assert_eq!(
            labels,
            vec![
                "engagement-window",
                "long-idle-no-restraint",
                "long-absence-reunion"
            ],
        );
    }

    /// Read panelTypes.ts and return the kebab/snake keys defined in
    /// PROMPT_RULE_DESCRIPTIONS. Used by both alignment tests so the parsing logic
    /// only lives in one place. Plain string scanning — no regex dep — and tolerant
    /// of both quoted (`"wake-back": {`) and bare-identifier (`plan: {`) keys.
    ///
    /// Iter 98: relocated from PanelDebug.tsx to panelTypes.ts so PanelDebug owns only
    /// state + layout. If the dict moves again, update this path and the panic
    /// messages below in lockstep.
    fn parse_prompt_rule_dict_keys() -> Vec<String> {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR is set during cargo test runs");
        let panel_path = std::path::PathBuf::from(manifest_dir)
            .join("..")
            .join("src")
            .join("components")
            .join("panel")
            .join("panelTypes.ts");
        let panel_src = std::fs::read_to_string(&panel_path)
            .unwrap_or_else(|e| panic!("read panelTypes.ts: {}", e));
        let mut keys: Vec<String> = Vec::new();
        let mut in_dict = false;
        for line in panel_src.lines() {
            // Accept both `const ...` and `export const ...` forms — Iter 97 made the
            // dict an export so a sibling component can import it.
            if line.starts_with("const PROMPT_RULE_DESCRIPTIONS")
                || line.starts_with("export const PROMPT_RULE_DESCRIPTIONS")
            {
                in_dict = true;
                continue;
            }
            if !in_dict {
                continue;
            }
            let trimmed = line.trim();
            if trimmed == "};" {
                break;
            }
            // Each top-level entry starts with `<key>: {` (key value is itself a {…}
            // object literal). Inner lines use `title: "..."` / `summary: "..."` and
            // don't match `: {` so they're naturally skipped.
            if let Some(idx) = trimmed.find(": {") {
                let raw = trimmed[..idx].trim().trim_matches('"');
                if !raw.is_empty()
                    && raw
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                {
                    keys.push(raw.to_string());
                }
            }
        }
        keys
    }

    #[test]
    fn proactive_rules_has_match_arm_for_every_backend_label() {
        // Iter 91 / Iter 93 — guard the proactive_rules match against the helper label
        // set. If a label ships from a helper but proactive_rules has no arm for it,
        // the fallback `(规则文本待补)` slips into the prompt — visible in panel logs
        // but easy to miss in CI. This test fails immediately instead.
        //
        // Some labels are mutually exclusive (chatty vs long-idle-no-restraint share the
        // chatty threshold; pre-quiet excludes long-idle). We run two scenarios and
        // require each fingerprint to fire in at least one of them — combined coverage
        // still pins down the full match arm set.
        let fingerprints: &[(&str, &str)] = &[
            ("wake-back", "用户刚从离开桌子回来"),
            ("first-mood", "第一次开口"),
            ("first-of-day", "今天的第一次开口"),
            ("pre-quiet", "快进入安静时段"),
            ("reminders", "有到期的用户提醒"),
            ("plan", "你有今日计划在执行中"),
            ("icebreaker", "你和用户还不熟"),
            ("chatty", "今天已经聊了不少"),
            ("companionship-milestone", "今天是和用户相处的"),
            ("env-awareness", "最近你开口前几乎都没看环境"),
            ("engagement-window", "此刻是开新话题的好时机"),
            ("long-idle-no-restraint", "沉默已久但没在克制状态"),
            ("long-absence-reunion", "用户离开了不短的时间"),
            ("late-night-wellness", "深夜还在用电脑"),
        ];

        // Sanity: the fingerprint table must cover every label the helpers emit. Use
        // each helper's max-trigger inputs to enumerate the universe.
        // Two composite calls: one with high idle (long-absence-reunion fires),
        // one with low idle + hour=2 (late-night-wellness fires) — labels conflict
        // on idle_minutes / hour so we union the two outputs to get the universe.
        let backend_labels: std::collections::HashSet<&'static str> =
            active_environmental_rule_labels(true, true, true, true, true, true)
                .into_iter()
                .chain(active_data_driven_rule_labels(0, 999, 1, 999, 0, 100))
                .chain(active_composite_rule_labels(
                    true,
                    true,
                    Some(120),
                    0,
                    5,
                    false,
                    LONG_ABSENCE_MINUTES + 60,
                    14,
                    false,
                ))
                .chain(active_composite_rule_labels(
                    false, false, None, 0, 5, false, 0, 2, false,
                ))
                .collect();
        let fingerprint_labels: std::collections::HashSet<&str> =
            fingerprints.iter().map(|(l, _)| *l).collect();
        let untested: Vec<&&'static str> = backend_labels
            .iter()
            .filter(|l| !fingerprint_labels.contains(*l))
            .collect();
        assert!(
            untested.is_empty(),
            "fingerprint table missing entries for backend labels: {:?}. Add a row \
             with a substring unique to that label's rule text.",
            untested
        );

        // Scenario 1: short-idle + chatty active + pre-quiet active.
        // Covers: wake-back, first-mood, pre-quiet, reminders, plan, icebreaker, chatty,
        // env-awareness, engagement-window, companionship-milestone.
        let mut s1 = base_inputs();
        s1.wake_hint = "（用户的电脑在大约 60 秒前刚从休眠唤醒。）";
        s1.is_first_mood = true;
        s1.pre_quiet_minutes = Some(10);
        s1.reminders_hint = "你有以下到期的用户提醒：\n· something";
        s1.plan_hint = "你今天的小目标：\n· something";
        s1.proactive_history_count = 0;
        s1.today_speech_count = s1.chatty_day_threshold;
        s1.env_spoke_total = 12;
        s1.env_spoke_with_any = 1;
        s1.companionship_days = 100; // milestone — fires companionship-milestone
                                     // since_last_proactive_minutes stays at base_inputs default (Some(8)) — short.
        let rules1 = proactive_rules(&s1);

        // Scenario 2: long-idle + under-chatty + !pre-quiet + long-absence.
        // Covers: long-idle-no-restraint, long-absence-reunion, plus everything
        // except chatty / pre-quiet.
        let mut s2 = s1;
        s2.pre_quiet_minutes = None;
        s2.today_speech_count = 0;
        s2.since_last_proactive_minutes = Some(LONG_IDLE_MINUTES);
        s2.idle_minutes = LONG_ABSENCE_MINUTES + 60;
        s2.idle_register = "用户已经离开了大半天";
        let rules2 = proactive_rules(&s2);

        // Scenario 3: late-night-wellness — needs hour<4 AND idle_minutes<5.
        // Distinct from s1/s2 (both at hour=14, large idle) because the rule is
        // specifically designed to fire when the user is actively at the
        // keyboard past midnight.
        let mut s3 = base_inputs();
        s3.hour = 2;
        s3.idle_minutes = 1;
        s3.idle_register = "用户刚刚还在";
        let rules3 = proactive_rules(&s3);

        let combined: Vec<&String> = rules1
            .iter()
            .chain(rules2.iter())
            .chain(rules3.iter())
            .collect();
        assert!(
            !combined.iter().any(|r| r.contains("规则文本待补")),
            "proactive_rules emitted the unknown-label fallback. A helper added a \
             label without a matching arm in proactive_rules.\nRules1: {:#?}\nRules2: {:#?}\nRules3: {:#?}",
            rules1,
            rules2,
            rules3,
        );
        for (label, fp) in fingerprints {
            assert!(
                combined.iter().any(|r| r.contains(fp)),
                "label '{}' should produce a rule containing '{}' in at least one of \
                 the two scenarios, but neither matched. Either the arm is missing \
                 or its text changed.",
                label,
                fp
            );
        }
    }

    #[test]
    fn frontend_prompt_rule_descriptions_have_no_ghost_labels() {
        // Iter 90 — reverse of Iter 89's coverage check. A "ghost" entry is a key in
        // PROMPT_RULE_DESCRIPTIONS that no backend label helper ever returns; it
        // wastes dictionary space and quietly hides a UI string nobody can ever see.
        // Catches dead translations left behind after a backend rule was renamed or
        // removed.
        let frontend_keys = parse_prompt_rule_dict_keys();
        assert!(
            !frontend_keys.is_empty(),
            "PROMPT_RULE_DESCRIPTIONS dictionary appears empty — parser may be broken \
             or the dict was emptied. Iter 89's test should also be failing."
        );
        // All-true / max-trigger inputs surface every label all backend helpers can
        // ever produce. Same input recipe as Iter 89's test for consistency.
        // Two composite calls union: late-night needs hour<4 + idle<5 which
        // conflicts with long-absence (idle ≥ 240) so we collect both.
        let env = active_environmental_rule_labels(true, true, true, true, true, true);
        let data = active_data_driven_rule_labels(0, 999, 1, 999, 0, 100);
        let composite_a = active_composite_rule_labels(
            true,
            true,
            Some(120),
            0,
            5,
            false,
            LONG_ABSENCE_MINUTES + 60,
            14,
            false,
        );
        let composite_b =
            active_composite_rule_labels(false, false, None, 0, 5, false, 0, 2, false);
        let backend: std::collections::HashSet<&'static str> = env
            .iter()
            .chain(data.iter())
            .chain(composite_a.iter())
            .chain(composite_b.iter())
            .copied()
            .collect();
        let ghosts: Vec<&str> = frontend_keys
            .iter()
            .filter(|k| !backend.contains(k.as_str()))
            .map(|s| s.as_str())
            .collect();
        assert!(
            ghosts.is_empty(),
            "PROMPT_RULE_DESCRIPTIONS contains ghost keys with no backend producer: \
             {:?}. Either remove them, or add the corresponding backend label.",
            ghosts
        );
    }

    #[test]
    fn frontend_prompt_rule_descriptions_cover_every_backend_label() {
        // Iter 89 — guard the contract between the Rust label helpers and the frontend
        // PROMPT_RULE_DESCRIPTIONS dictionary in PanelDebug.tsx. If either side adds a
        // label without updating the other, the panel falls back to "(label X 暂无中文
        // 描述)" — visible but easy to miss in dev. This test fails CI instead.
        //
        // Iter 90 unified the parsing path: both this test and the ghost-keys test go
        // through `parse_prompt_rule_dict_keys`, so they validate against the same
        // canonical key set extracted from the dictionary block.
        let frontend_keys = parse_prompt_rule_dict_keys();
        assert!(
            !frontend_keys.is_empty(),
            "PROMPT_RULE_DESCRIPTIONS dictionary not found or empty in panelTypes.ts \
             — the frontend may have moved or renamed it. Update parse_prompt_rule_dict_keys."
        );
        let frontend_set: std::collections::HashSet<&str> =
            frontend_keys.iter().map(|s| s.as_str()).collect();
        // All-true inputs surface every possible label all helpers can ever return.
        // late-night-wellness needs hour<4 + idle<5 — conflicts with long-absence,
        // so emit it via a second composite call and chain.
        let env = active_environmental_rule_labels(true, true, true, true, true, true);
        let data = active_data_driven_rule_labels(0, 999, 1, 999, 0, 100);
        let composite_a = active_composite_rule_labels(
            true,
            true,
            Some(120),
            0,
            5,
            false,
            LONG_ABSENCE_MINUTES + 60,
            14,
            false,
        );
        let composite_b =
            active_composite_rule_labels(false, false, None, 0, 5, false, 0, 2, false);
        let missing: Vec<&'static str> = env
            .iter()
            .chain(data.iter())
            .chain(composite_a.iter())
            .chain(composite_b.iter())
            .filter(|l| !frontend_set.contains(*l))
            .copied()
            .collect();
        assert!(
            missing.is_empty(),
            "panelTypes.ts PROMPT_RULE_DESCRIPTIONS missing entries for backend \
             labels: {:?}. Add a {{title, summary, nature}} row for each.",
            missing
        );
    }

    #[test]
    fn proactive_rules_contextual_count_matches_label_count() {
        // The match-by-label refactor (Iter 87) means the number of contextual rules
        // pushed must equal env_labels.len() + data_labels.len(). If a future helper
        // returns a label proactive_rules doesn't recognize, the fallback "(规则文本待补)"
        // catches it but this test pins down the healthy path.
        let mut inputs = base_inputs();
        // Trip every contextual rule.
        inputs.wake_hint = "（用户的电脑在大约 60 秒前刚从休眠唤醒。）";
        inputs.is_first_mood = true;
        inputs.pre_quiet_minutes = Some(10);
        inputs.reminders_hint = "你有以下到期的用户提醒：\n· something";
        inputs.plan_hint = "你今天的小目标：\n· something";
        inputs.proactive_history_count = 0;
        inputs.today_speech_count = inputs.chatty_day_threshold;
        inputs.env_spoke_total = 12;
        inputs.env_spoke_with_any = 1;
        // pre-quiet on AND long-idle are mutually exclusive by design (long-idle
        // requires !pre_quiet). Likewise chatty (today_count >= threshold) and
        // long-idle exclude each other. This scenario: pre-quiet on, today=0
        // (no chatty AND first-of-day fires — Iter Cξ added that 6th env label),
        // long-idle won't fire — so we expect:
        // 6 base + 6 env (wake-back/first-mood/first-of-day/pre-quiet/reminders/plan)
        // + 2 data (icebreaker + env-awareness) + 1 composite (engagement-window)
        // = 15. The fingerprint test below covers long-idle / chatty / long-absence
        // in a separate scenario, so combined coverage is still complete.
        inputs.today_speech_count = 0;
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 6 + 6 + 2 + 1, "rules: {:#?}", rules);
        assert!(!rules.iter().any(|r| r.contains("规则文本待补")));
    }

    #[test]
    fn proactive_rules_baseline_only_pushes_always_on_rules() {
        // With a neutral base_inputs (no contextual triggers), only the 5 always-pushed
        // rules should appear — proves the contextual loop adds nothing when labels are empty.
        let rules = proactive_rules(&base_inputs());
        // Strip the 6 always-on rules: silent / speak / single-line / tools / cache / motion.
        // Wait — there are actually 6 always-on (counted directly from the code). Use that.
        assert_eq!(
            rules.len(),
            6,
            "expected exactly 6 always-on rules in neutral state"
        );
    }

    #[test]
    fn env_awareness_corrective_rule_absent_when_healthy() {
        let mut inputs = base_inputs();
        inputs.env_spoke_total = 12;
        inputs.env_spoke_with_any = 8; // ~67%
        let rules = proactive_rules(&inputs);
        assert!(!rules
            .iter()
            .any(|r| r.contains("最近你开口前几乎都没看环境")));
    }

    #[test]
    fn chatty_mode_tag_disabled_when_threshold_zero() {
        assert_eq!(chatty_mode_tag(0, 0), None);
        assert_eq!(chatty_mode_tag(99, 0), None);
    }

    #[test]
    fn chatty_mode_tag_below_threshold_is_none() {
        assert_eq!(chatty_mode_tag(0, 5), None);
        assert_eq!(chatty_mode_tag(4, 5), None);
    }

    #[test]
    fn chatty_mode_tag_at_or_above_threshold_formats() {
        assert_eq!(chatty_mode_tag(5, 5), Some("chatty=5/5".to_string()));
        assert_eq!(chatty_mode_tag(7, 5), Some("chatty=7/5".to_string()));
    }

    #[test]
    fn append_outcome_tag_handles_empty_and_dash_and_chained() {
        // Iter QG3: shared decision-log reason builder. Empty start, then dash sentinel,
        // then plain chaining — three behaviors the loop and manual-trigger paths both
        // depend on.
        let mut empty = String::new();
        append_outcome_tag(&mut empty, "source=loop");
        assert_eq!(empty, "source=loop");

        let mut dash = "-".to_string();
        append_outcome_tag(&mut dash, "source=manual");
        assert_eq!(dash, "source=manual", "dash placeholder must be replaced");

        let mut chained = "chatty=5/5".to_string();
        append_outcome_tag(&mut chained, "source=loop");
        append_outcome_tag(&mut chained, "rules=icebreaker");
        assert_eq!(chained, "chatty=5/5, source=loop, rules=icebreaker");
    }

    #[test]
    fn record_proactive_outcome_spoke_path_bumps_counters_and_logs_source() {
        use crate::commands::debug::ProcessCounters;
        use crate::decision_log::DecisionLog;
        let counters = ProcessCounters::default();
        let decisions = DecisionLog::new();
        let outcome: Result<ProactiveTurnOutcome, String> = Ok(ProactiveTurnOutcome {
            reply: Some("hello".to_string()),
            tools: vec!["get_active_window".to_string()],
        });
        record_proactive_outcome(&counters, &decisions, "manual", "-", None, &outcome);
        // llm_outcome.spoke bumped, env_tool counted as spoke_with_any.
        assert_eq!(
            counters
                .llm_outcome
                .spoke
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            counters
                .env_tool
                .spoke_total
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            counters
                .env_tool
                .spoke_with_any
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        let snap = decisions.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].kind, "Spoke");
        assert!(snap[0].reason.contains("source=manual"));
        assert!(snap[0].reason.contains("tools=get_active_window"));
    }

    #[test]
    fn record_proactive_outcome_silent_path_bumps_silent_and_tags_loop() {
        use crate::commands::debug::ProcessCounters;
        use crate::decision_log::DecisionLog;
        let counters = ProcessCounters::default();
        let decisions = DecisionLog::new();
        let outcome: Result<ProactiveTurnOutcome, String> = Ok(ProactiveTurnOutcome {
            reply: None,
            tools: vec![],
        });
        record_proactive_outcome(
            &counters,
            &decisions,
            "loop",
            "chatty=5/5",
            Some("rules=chatty"),
            &outcome,
        );
        assert_eq!(
            counters
                .llm_outcome
                .silent
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        // env_tool MUST stay zero on silent — it would skew the env-aware ratio otherwise.
        assert_eq!(
            counters
                .env_tool
                .spoke_total
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        let snap = decisions.snapshot();
        assert_eq!(snap[0].kind, "LlmSilent");
        assert!(snap[0].reason.contains("source=loop"));
        assert!(snap[0].reason.contains("rules=chatty"));
        assert!(snap[0].reason.contains("chatty=5/5"));
    }

    #[test]
    fn record_proactive_outcome_error_path_bumps_error_and_includes_message() {
        use crate::commands::debug::ProcessCounters;
        use crate::decision_log::DecisionLog;
        let counters = ProcessCounters::default();
        let decisions = DecisionLog::new();
        let outcome: Result<ProactiveTurnOutcome, String> = Err("network down".to_string());
        record_proactive_outcome(&counters, &decisions, "manual", "-", None, &outcome);
        assert_eq!(
            counters
                .llm_outcome
                .error
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        let snap = decisions.snapshot();
        assert_eq!(snap[0].kind, "LlmError");
        assert!(snap[0].reason.contains("network down"));
        assert!(snap[0].reason.contains("source=manual"));
    }

    fn passthrough(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn format_reminders_hint_empty_returns_empty_string() {
        let out = format_reminders_hint(&[], &passthrough);
        assert_eq!(out, "");
    }

    #[test]
    fn format_plan_hint_empty_or_whitespace_returns_empty() {
        assert_eq!(format_plan_hint("", &passthrough), "");
        assert_eq!(format_plan_hint("   \n  ", &passthrough), "");
    }

    #[test]
    fn format_proactive_mood_hint_empty_returns_first_time_message() {
        let out = format_proactive_mood_hint("", &passthrough);
        assert!(out.contains("还没有记录过"));
        assert!(out.contains("第一次"));
    }

    // -- task-completion hint --------------------------------------------------

    #[test]
    fn task_completion_first_tick_treats_all_done_as_new() {
        let prev = std::collections::HashSet::new();
        let items = vec![
            (
                "整理 downloads".to_string(),
                "[task pri=2] 把 30 天前的文件挪到 ~/Archive [done] [result: 38 个文件]".to_string(),
            ),
            (
                "提醒喝水".to_string(),
                "[task pri=1] [done]".to_string(),
            ),
        ];
        let (new, current) = compute_recent_task_completions(&items, &prev);
        assert_eq!(new.len(), 2, "首次见到的 done 都算新转 done");
        assert_eq!(current.len(), 2);
        assert!(current.contains("整理 downloads"));
    }

    #[test]
    fn task_completion_repeated_done_fires_only_once() {
        let mut prev = std::collections::HashSet::new();
        prev.insert("整理 downloads".to_string());
        let items = vec![(
            "整理 downloads".to_string(),
            "[task pri=2] [done] [result: 完成]".to_string(),
        )];
        let (new, current) = compute_recent_task_completions(&items, &prev);
        assert!(new.is_empty(), "已见过的 done 不再算新");
        assert!(current.contains("整理 downloads"));
    }

    #[test]
    fn task_completion_pending_excluded_even_if_in_prev() {
        // LLM 把 done 又改回 pending（罕见，但允许）。本轮不出现在 new；
        // current_done 也不应再含此条，下次它再转 done 时会被重新视作新。
        let mut prev = std::collections::HashSet::new();
        prev.insert("整理 downloads".to_string());
        let items = vec![(
            "整理 downloads".to_string(),
            "[task pri=2] 重新进行中".to_string(),
        )];
        let (new, current) = compute_recent_task_completions(&items, &prev);
        assert!(new.is_empty());
        assert!(!current.contains("整理 downloads"));
    }

    #[test]
    fn task_completion_format_skips_when_empty() {
        assert_eq!(format_task_completion_hint(&[]), "");
    }

    #[test]
    fn task_completion_format_includes_title_and_result() {
        let items = vec![CompletedTaskBrief {
            title: "整理 downloads".to_string(),
            result: Some("把 38 个文件归档到 ~/Archive/".to_string()),
        }];
        let out = format_task_completion_hint(&items);
        assert!(out.contains("[任务刚完成]"));
        assert!(out.contains("整理 downloads"));
        assert!(out.contains("产物：把 38 个文件归档"));
    }

    #[test]
    fn task_completion_format_marks_missing_result() {
        let items = vec![CompletedTaskBrief {
            title: "整理 downloads".to_string(),
            result: None,
        }];
        let out = format_task_completion_hint(&items);
        assert!(out.contains("无产物记录"));
    }

    #[test]
    fn task_completion_format_truncates_long_result() {
        let long: String = "X".repeat(TASK_COMPLETION_RESULT_CHARS + 30);
        let items = vec![CompletedTaskBrief {
            title: "t".to_string(),
            result: Some(long),
        }];
        let out = format_task_completion_hint(&items);
        assert!(out.ends_with("…）"));
    }

    #[test]
    fn task_completion_format_caps_list_with_overflow_line() {
        let items: Vec<CompletedTaskBrief> = (0..(TASK_COMPLETION_HINT_MAX_ITEMS + 3))
            .map(|i| CompletedTaskBrief {
                title: format!("t{}", i),
                result: None,
            })
            .collect();
        let out = format_task_completion_hint(&items);
        let bullets = out.matches("· ").count();
        // 5 listed bullets + 1 "…还有 N 条" overflow bullet。
        assert_eq!(bullets, TASK_COMPLETION_HINT_MAX_ITEMS + 1);
        assert!(out.contains("…还有 3 条"));
    }

    // -- recent (24h) completion hint ----------------------------------------

    fn ndt(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, 0)
            .unwrap()
    }

    #[test]
    fn recent_completion_empty_returns_empty_string() {
        let now = ndt(2026, 5, 16, 12, 0);
        assert_eq!(compute_recent_completions(&[], now), Vec::new());
        assert_eq!(format_recent_completion_hint(&[]), "");
    }

    #[test]
    fn recent_completion_filters_non_done_status() {
        // pending / error / cancelled 任务一律跳过 —— 本 hint 只展示已完成
        let now = ndt(2026, 5, 16, 12, 0);
        let items = vec![
            (
                "做完了".to_string(),
                "[task pri=3] xxx [done]".to_string(),
                "2026-05-16T10:00:00".to_string(),
            ),
            (
                "还没做".to_string(),
                "[task pri=3] yyy".to_string(),
                "2026-05-16T10:00:00".to_string(),
            ),
            (
                "失败了".to_string(),
                "[task pri=3] zzz [error: timeout]".to_string(),
                "2026-05-16T10:00:00".to_string(),
            ),
            (
                "取消了".to_string(),
                "[task pri=3] qqq [cancelled: no need]".to_string(),
                "2026-05-16T10:00:00".to_string(),
            ),
        ];
        let recent = compute_recent_completions(&items, now);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].title, "做完了");
    }

    #[test]
    fn recent_completion_24h_window_cutoff() {
        // updated 25h 前 → 不在窗口；< 24h → 在
        let now = ndt(2026, 5, 16, 12, 0);
        let items = vec![
            (
                "刚完成".to_string(),
                "[task pri=3] aaa [done]".to_string(),
                "2026-05-16T08:00:00".to_string(),  // 4h ago
            ),
            (
                "昨天完成".to_string(),
                "[task pri=3] bbb [done]".to_string(),
                "2026-05-15T13:00:00".to_string(),  // 23h ago
            ),
            (
                "前天完成".to_string(),
                "[task pri=3] ccc [done]".to_string(),
                "2026-05-15T11:00:00".to_string(),  // 25h ago → out
            ),
        ];
        let recent = compute_recent_completions(&items, now);
        assert_eq!(recent.len(), 2);
        let titles: Vec<&str> = recent.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"刚完成"));
        assert!(titles.contains(&"昨天完成"));
        assert!(!titles.contains(&"前天完成"));
    }

    #[test]
    fn recent_completion_sorts_by_recency() {
        // 最近完成的在前
        let now = ndt(2026, 5, 16, 12, 0);
        let items = vec![
            (
                "早 4h".to_string(),
                "[task pri=3] x [done]".to_string(),
                "2026-05-16T08:00:00".to_string(),
            ),
            (
                "早 1h".to_string(),
                "[task pri=3] x [done]".to_string(),
                "2026-05-16T11:00:00".to_string(),
            ),
            (
                "早 2h".to_string(),
                "[task pri=3] x [done]".to_string(),
                "2026-05-16T10:00:00".to_string(),
            ),
        ];
        let recent = compute_recent_completions(&items, now);
        assert_eq!(recent[0].title, "早 1h");
        assert_eq!(recent[1].title, "早 2h");
        assert_eq!(recent[2].title, "早 4h");
    }

    #[test]
    fn recent_completion_skips_unparseable_timestamps() {
        // 老 yaml 偶发 corrupt timestamp 不该让 hint 整段炸
        let now = ndt(2026, 5, 16, 12, 0);
        let items = vec![
            (
                "好数据".to_string(),
                "[task pri=3] ok [done]".to_string(),
                "2026-05-16T10:00:00".to_string(),
            ),
            (
                "坏数据".to_string(),
                "[task pri=3] bad [done]".to_string(),
                "not-a-timestamp".to_string(),
            ),
        ];
        let recent = compute_recent_completions(&items, now);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].title, "好数据");
    }

    #[test]
    fn recent_completion_includes_result_marker() {
        let now = ndt(2026, 5, 16, 12, 0);
        let items = vec![(
            "整理 downloads".to_string(),
            "[task pri=3] xxx [done] [result: 归档 38 个文件]".to_string(),
            "2026-05-16T10:00:00".to_string(),
        )];
        let recent = compute_recent_completions(&items, now);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].result.as_deref(), Some("归档 38 个文件"));
        let out = format_recent_completion_hint(&recent);
        assert!(out.contains("[最近 24h 完成]"));
        assert!(out.contains("整理 downloads"));
        assert!(out.contains("产物：归档 38 个文件"));
    }

    #[test]
    fn recent_completion_caps_list_with_overflow_line() {
        // 超过 N 条 cap，多余的转 "…还有 K 条"
        let now = ndt(2026, 5, 16, 12, 0);
        let items: Vec<(String, String, String)> = (0
            ..(RECENT_COMPLETION_HINT_MAX_ITEMS + 4))
            .map(|i| {
                (
                    format!("t{}", i),
                    "[task pri=3] x [done]".to_string(),
                    format!("2026-05-16T{:02}:00:00", 11 - (i as u32 % 12)),
                )
            })
            .collect();
        let recent = compute_recent_completions(&items, now);
        // 24h 窗口内的都收，不限制 cap（cap 在 format 层）
        assert!(recent.len() >= RECENT_COMPLETION_HINT_MAX_ITEMS + 1);
        let out = format_recent_completion_hint(&recent);
        assert!(out.contains(&format!("…还有 {} 条", recent.len() - RECENT_COMPLETION_HINT_MAX_ITEMS)));
    }

    #[test]
    fn recent_completion_future_timestamp_is_skipped() {
        // 数据 corrupt 防御：updated_at > now 视作无效
        let now = ndt(2026, 5, 16, 12, 0);
        let items = vec![(
            "未来时间戳".to_string(),
            "[task pri=3] x [done]".to_string(),
            "2026-05-16T13:00:00".to_string(),  // 1h after now
        )];
        let recent = compute_recent_completions(&items, now);
        assert!(recent.is_empty());
    }

    #[test]
    fn chatty_day_threshold_is_user_tunable() {
        // Custom threshold of 10 should hold its boundary regardless of the const.
        let mut inputs = base_inputs();
        inputs.chatty_day_threshold = 10;
        inputs.today_speech_count = 9;
        assert!(!proactive_rules(&inputs)
            .iter()
            .any(|r| r.contains("今天已经聊了不少")));
        inputs.today_speech_count = 10;
        assert!(proactive_rules(&inputs)
            .iter()
            .any(|r| r.contains("今天已经聊了不少")));
    }

    #[test]
    fn plan_hint_appears_in_full_prompt() {
        let mut inputs = base_inputs();
        inputs.plan_hint = "你今天的小目标 / 计划：\n· 关心用户工作进展 [0/2]";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("关心用户工作进展"));
    }

    #[test]
    fn pre_quiet_rule_appears_when_set() {
        let mut inputs = base_inputs();
        inputs.pre_quiet_minutes = Some(10);
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 7);
        assert!(rules
            .iter()
            .any(|r| r.contains("快进入安静时段") && r.contains("10 分钟")));
    }
