# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，对照 debug 日志里的"missing [motion: X] prefix"出现率判断要不要再改 prompt。
- [ ] Iter 40：把 chat_done / proactive-message 事件里的"motion fallback 用了关键词"也记录到一个独立 ring buffer，让 UI 能展示"上次发话用了 Tap 但走的是 keyword fallback"——验证 LLM 实际是否在守 `[motion: X]` 格式（替代 Iter 12b 的需要交互的实测）。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
