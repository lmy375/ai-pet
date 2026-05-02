# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 51：proactive prompt 已经积累了 7+ 个 hint 段（time/period、idle、cadence、mood、focus、wake、speech），string 拼接膨胀且各段的开关条件分散。重构成一个 prompt builder（按需 push 段，最后 join），让加新 hint 的代价从 "改 5 处" 降到 "调一次 builder.push_if(...)"。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
