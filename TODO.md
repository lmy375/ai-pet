# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 45：让宠物主动消息也持久化到一个独立的 "speech_history" memory 项（最近 N 条，类似 mood），让下一次主动开口前 LLM 看到"我刚说了什么"避免话题重复。当前每次 proactive 都重新加载 session messages，但 session 可能很长被裁掉，对自己最近发言记忆不可靠。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
