# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，对照 debug 日志里的"missing [motion: X] prefix"出现率判断要不要再改 prompt。
- [ ] Iter 33：把 LogStore 当前的 unbounded `Vec<String>` 加上 size cap（保留最近 N 行，比如 10000），避免长时间运行 OOM。同时让 `get_cache_stats` 即便日志被裁剪也仍然准（也许把累计统计搬到独立 atomic 而非依赖日志解析）。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
