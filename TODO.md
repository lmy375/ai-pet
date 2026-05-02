# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
- [ ] Iter 3：键盘鼠标空闲时长检测（CGEventSourceSecondsSinceLastEventType），加入主动判断条件——idle 阈值改成"距上次互动 ≥ N 分钟 **且** 用户键鼠空闲 ≥ M 秒"，避免在用户正打字时打断。
- [ ] Iter 4：宠物当下情绪/状态写入 memory（如 `current_mood` 条目），主动 prompt 中作为 context；说话后 LLM 自行更新。
- [ ] Iter 5：主动发言节奏控制——cooldown / 用户未回应不再连续主动开口；上一条 proactive 没有用户回应前不再触发。
- [ ] Iter 6：定期记忆 consolidate（合并、去重、过期），后台每 N 小时跑一次。
- [ ] Iter 7：日历 / 天气 / 系统通知集成（MCP 或新工具）。
- [ ] Iter 8：根据情绪驱动 Live2D 表情和动作。
- [ ] 设置面板加上 Proactive 开关 + 间隔/阈值滑条（目前只能改 config.yaml）。
