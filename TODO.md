# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 43：让宠物在用户长时间空闲（如 2+ 小时）后说"早上好"/"下午好"等基于时间的问候——目前 proactive 开口的 mood prompt 只关心心情、不关心一天的节奏感。可以在 prompt 里注入"现在是 morning/afternoon/evening/night"，让模型话题更应景。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
