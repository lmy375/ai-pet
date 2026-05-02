# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 52：把 proactive prompt 的"约束"段（5 条带 `-` 的规则）也抽成 `Vec<&str>` 常量，让加新规则的代价从"找到中间插一行"降到"在 const array 末尾 push 一行"——尤其便于未来动态启停某条规则（比如 disabled focus 时跳过相关说明）。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
