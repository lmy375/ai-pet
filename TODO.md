# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，对照 debug 日志里的"missing [motion: X] prefix"出现率判断要不要再改 prompt。
- [ ] Iter 19：给 `evaluate_loop_tick` 写表驱动测试——每个 gate 一个 case（构造伪 settings + 伪 snapshot），验证返回 LoopAction 正确。这样以后再加新 gate（focus mode、quiet hours 等）时回归测试已有逻辑。需要先抽 `Snapshot` provider 让函数可注入而不依赖真实 InteractionClockStore。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
- [ ] PanelSettings.tsx：把新加的 Proactive / Consolidate 配置也接进 panel 形式视图。
