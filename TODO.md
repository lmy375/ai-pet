# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，对照 debug 日志里的"missing [motion: X] prefix"出现率判断要不要再改 prompt。
- [ ] Iter 39：把 LoopAction Silent 的常量 reason 也通过 `pub const` 暴露出来给前端用——目前 reason 字符串是硬编码 "disabled"/"quiet_hours"/"idle_below_threshold"，前端 UI 要显示中文"已禁用"/"安静时段"等需要重复一份映射。或者后端就出中文文案。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
