# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 73：speech_history 加按日分桶能力。在 `record_speech_inner` 写完后另写 `~/.config/pet/speech_daily.json`（map: YYYY-MM-DD -> count），读时合并 lifetime + 当日，让 stats 卡能展示"今天开口 X 次 / 总累计 Y 次"双值。带 90 天滚动 cap 防 unbounded。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
