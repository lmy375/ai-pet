# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 8：根据情绪驱动 Live2D 表情和动作（读 ai_insights/current_mood，前端按关键词切换表情）。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script），用于"我看见你刚收到 XX 的消息"类话题——延后，因为需要 Full Disk Access、schema 不稳定、隐私风险高，优先级落后于 Iter 8。
- [ ] PanelSettings.tsx：把新加的 Proactive / Consolidate 配置也接进 panel 形式视图（目前只在小窗 SettingsPanel 里能改；panel 视图自动接收后端值，但缺乏专属 UI 表单）。
