# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 68：让宠物的 first-time chat 互动有"破冰"专属规则——目前完全冷启动时 LLM 看到的 prompt 和长期老用户一样，缺少"我们刚认识，先了解你"的初印象。需要追踪 chat history 总长度 / interaction 总次数，作为新的 PromptInputs 字段。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
