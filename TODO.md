# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 10：mood 关键词匹配从硬编码列表升级为 LLM 直接给出 motion group 标签（让模型自己挑"我现在该做什么动作"），减少前端枚举的脆弱性。
- [ ] Iter 11：反应式 chat 在 prompt 中也注入 mood，并提示 LLM 可以更新 mood——目前 mood 只在 proactive 里更新，长对话里 mood 一直停留在最后一次主动开口的状态。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
- [ ] PanelSettings.tsx：把新加的 Proactive / Consolidate 配置也接进 panel 形式视图。
