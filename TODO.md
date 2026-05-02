# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 65：让"立即开口"返回的 status 包含本次 LLM 真实回复内容（"开口说: '...'"）——目前只看到耗时数字。需要 trigger_proactive_turn 把 reply 一并返。proactive-message Tauri event 已经会推到 useChat 显示气泡，但 panel 单独看也想知道说啥。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
