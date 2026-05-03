# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 74：speech_daily.json 已经记了过去 90 天，扩展 panel stats 卡为"今日 / 本周 / 累计"三列，本周走 `recent_days_speech_count(7) -> sum`。或加一行 sparkline（7 天柱状）让用户看到趋势。
- [ ] Iter 92：当前 8 条 prompt 软规则的设计偏 "克制宠物"（icebreaker / chatty / pre-quiet 都让宠物更安静）+ "纠偏"（env-awareness）。考虑加一条积极规则——比如"if has_plan && wake_back && idle long" 这种"用户刚回桌子，宠物今天有计划，距离上次说话很久"的复合 trigger，鼓励主动开新话题。让 prompt 系统不只是单向限制宠物，也能根据复合信号主动提议。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
