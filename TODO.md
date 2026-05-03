# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
## 下一阶段（Iter 100 盘点后重排）

参 STATUS.md "未来路线"。从 A 路线（长期人格演化）切入，因为它把已有 infra 真正绑
在一起；其余路线作为辅助优先级。

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

### 路线 E：跨设备同步
- [ ] Iter Ex：把 ~/.config/pet/ 下子集（memory / speech_history / mood）支持 iCloud Drive
  路径，让两台 Mac 共享同一只宠物。settings 切换。

### 历史保留候选
- [ ] Iter 74：speech_daily.json 扩展 panel stats 卡为"今日 / 本周 / 累计"三列。视觉。
- [ ] Iter 7c (deferred)：macOS 系统通知 hook（NotificationCenter.db）。Full Disk Access、
  schema 不稳定、隐私风险高，长期 deferred。
