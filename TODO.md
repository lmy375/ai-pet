# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 59：reminder 解析支持 "today HH:MM"、"+N min" 等更灵活格式——目前只支持 `[remind: 23:00]`，无法表达"30 分钟后"或者"明天 9 点"这类相对时间。先扩展 parse_reminder_prefix 支持 `[remind: +30m]`、`[remind: tomorrow 09:00]` 等，再让 LLM 知道这些写法。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
