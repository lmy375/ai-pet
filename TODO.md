# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，对照 debug 日志里的"missing [motion: X] prefix"出现率判断要不要再改 prompt。
- [ ] Iter 36：把对话历史 trim 也搬到 settings——目前 telegram bot 写死 `MAX_CONTEXT_MESSAGES=50`，桌面 chat 没限制（依赖 frontend 提交的全部历史）。让用户能在 PanelSettings 调，或至少给桌面 chat 加默认上限避免长会话 token 爆炸。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
