# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 74：speech_daily.json 已经记了过去 90 天，扩展 panel stats 卡为"今日 / 本周 / 累计"三列，本周走 `recent_days_speech_count(7) -> sum`。或加一行 sparkline（7 天柱状）让用户看到趋势。
- [ ] Iter 89：写一个 `cargo test` 校验 PROMPT_RULE_DESCRIPTIONS 字典里的 keys 覆盖 backend 所有可能的 label（active_data_driven_rule_labels 和 active_environmental_rule_labels 全开后返回的全集）。当前如果 backend 加 label 但前端忘加翻译，只有靠 fallback 文案"暂无中文描述"在 UI 上视觉发现——加测试断 CI。但需要 frontend test runner 或同步 const 暴露给 Rust，先想清楚最低成本路径。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
