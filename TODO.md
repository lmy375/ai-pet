# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
## 下一阶段（Iter 100 盘点后重排，2026-05-03 用户加入"宠物管家"方向）

**当前主轴是路线 F（宠物管家），其他路线退为辅助优先级。** 跨设备同步已被用户明确移除。

### 路线 F：宠物管家（用户委托执行实际工作）— Iter Cγ 起步
- [x] Iter Cγ：butler_tasks 记忆类别 + 提示注入 + tools 描述 + panel 顺序（2026-05-03 完成）
- [x] Iter Cδ：panel 顶部 "+ 委托任务" 快捷入口 + 模态分类 placeholder（2026-05-03 完成）
- [ ] Iter Cε：butler_task 执行留痕——LLM 完成一项时自动 append 到 speech_history
  并发 panel 事件，让用户能看到"宠物刚为我做了什么"。
- [ ] Iter Cζ：scheduled butler_tasks——支持 description 前缀 `[every: 09:00]` /
  `[once: 2026-05-10 14:00]`，proactive 循环按时机选择性触发（参考 reminder parser
  做法，复用 ReminderTarget 但语义不同：reminder 是给用户的，schedule 是给宠物的）。
- [ ] Iter Cη：butler_task 执行结果摘要——consolidate 把今日完成的任务汇成一句
  "今天我帮你做了 X / Y / Z"塞进 speech_history 让用户回看。

### 路线 A：长期人格演化（Iter 101-107 全部完成，路线 A 真正收官）

### 路线 B：表情系统升级
- [ ] Iter 8b：扩展 mood 解析支持 expression 字段（如 [motion: Tap, expression: smile]），
  前端读到后切 Live2D expression 而不只是 motion group。

### 路线 C：隐私 filter
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 Iter 40 的
  Tag 统计观察实际命中率，决定是否要再加强 prompt。

### 路线 D：记忆 surface
- [ ] Iter Dx：panel 加 Memory tab（已有 PanelMemory.tsx，需要从 yaml 索引展开成可读 UI），
  让用户看到宠物"记住了什么"。增强信任也帮 debug。

### 历史保留候选
- [ ] Iter 74：speech_daily.json 扩展 panel stats 卡为"今日 / 本周 / 累计"三列。视觉。
