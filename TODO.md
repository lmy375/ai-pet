# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，对照 debug 日志里的"missing [motion: X] prefix"出现率判断要不要再改 prompt。
- [ ] Iter 28：把 `get_active_window` / `get_weather` / `get_upcoming_events` 这种"环境感知"工具的调用结果做一层缓存（同一 tick 内复用），避免 LLM 在主动开口检查中连续多次调用。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
