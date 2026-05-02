# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 54：把 quiet hours 临近边界（如距 quiet 开始 < 15 分钟）也作为 conditional rule 注入——这样宠物在快到睡眠时段时会主动调"晚安"语气，而不是被 quiet_hours gate 冷启动。需要在 evaluate_pre_input_idle 之外把 hour 信息也带进 PromptInputs。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
