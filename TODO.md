# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 42：把 cache_counters / mood_tag_counters 这种"全 process counters"合并成一个 `ProcessCounters` Tauri State——目前每加一组就要扩 ToolContext + 5 callsite 透传一遍（Iter 34 + 40 都做过），rule of three 已到。重构 + 不破坏现有 API。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
