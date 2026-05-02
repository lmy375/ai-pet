# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 11：反应式 chat 在 prompt 中也注入 mood，并提示 LLM 可以更新 mood（含 [motion: X] 前缀）——目前 mood 只在 proactive 里更新。
- [ ] Iter 12：实测一次 proactive 端到端：实机跑 LLM 看 mood 写入是否带 `[motion: X]` 前缀；如果模型经常忘前缀，把 mood 解析失败率写到日志做个 fallback 监控。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
- [ ] PanelSettings.tsx：把新加的 Proactive / Consolidate 配置也接进 panel 形式视图。
