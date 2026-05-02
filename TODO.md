# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 panel 里 Iter 40 加的 Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter 58：让 PanelDebug 也展示当前 todo 类别下所有"reminder 候选"——已设但未到时间的列表。用户调试"我刚让宠物提醒我，到底有没有记下来"时一目了然。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
