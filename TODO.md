# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 72：speech_count.txt 暴露成 Tauri command（如 `get_lifetime_speech_count`），让 panel"统计"页能展示更显眼的「累计开口数」大数字而非只在 chip 里。可以加按周分桶（last 7d / 30d / lifetime）但需要在 record_speech 时也写时间轴；先做 lifetime 单值，分桶后续。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
