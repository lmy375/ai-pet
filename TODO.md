# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 74：speech_daily.json 已经记了过去 90 天，扩展 panel stats 卡为"今日 / 本周 / 累计"三列，本周走 `recent_days_speech_count(7) -> sum`。或加一行 sparkline（7 天柱状）让用户看到趋势。
- [ ] Iter 77：当前"今日 N 次"已超过 `chatty_day_threshold` 时，panel stats 卡的「今日」数字变橙色 + 加 hover 文案"已进入克制模式"。让用户视觉上看到行为切换（现在只有 LLM 内部知道）。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
