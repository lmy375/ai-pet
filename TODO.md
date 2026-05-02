# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 57：在 reactive chat 的 inject_mood_note 中也提示 LLM "如果用户说'X 时提醒我做 Y'，把它写成 todo 类别下 description 以 `[remind: HH:MM] Y` 开头的 memory item"——目前 LLM 知道有 memory_edit 工具但不知道这个格式约定。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
