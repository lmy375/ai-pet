# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，对照 debug 日志里的"missing [motion: X] prefix"出现率判断要不要再改 prompt。
- [ ] Iter 22：把当前活跃的 Focus 名字（"工作"、"专注"…）也读出来注入 proactive prompt——让宠物知道 "用户正在工作 Focus"，下次开口时话题更贴合（比如"等你忙完工作我们再聊"）。需要扩 focus_mode 模块返回 `Option<FocusInfo { active, name }>`。
- [ ] Iter 7c (deferred)：macOS 系统通知读取或 hook（NotificationCenter.db 或 user-script）。需 Full Disk Access、schema 不稳定、隐私风险高。
- [ ] PanelSettings.tsx：把新加的 Proactive / Consolidate 配置也接进 panel 形式视图。
