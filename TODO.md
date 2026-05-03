# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
## 下一阶段（Iter 100 盘点后重排）

参 STATUS.md "未来路线"。从 A 路线（长期人格演化）切入，因为它把已有 infra 真正绑
在一起；其余路线作为辅助优先级。

### 路线 A：长期人格演化（首要）
- [ ] Iter 101：把"陪伴天数"作为 prompt input——首次启动写 ~/.config/pet/install_date.txt，
  每次 proactive prompt 加一句"你和用户一起走过 N 天"。最小步骤但是路线 A 的入口。
- [ ] Iter 102：speech_history 中的 50 行截取改为 LLM 自己生成"性格摘要"：每日 consolidate
  时让 LLM 读最近 50 句自己的话 + user_profile，写一段 ~100 字的"我观察到自己的语气
  / 与用户互动的特点"到 ai_insights/persona_summary.md。下次 proactive 把它注进 prompt。
  让人格不是静态 SOUL.md，而是"自我反思形成的画像"。
- [ ] Iter 103：mood 加趋势——记 mood_history.log（与 speech_history 类似），让 prompt 看
  到"我最近一周心情主要是 X，今天偏 Y"，引导 LLM 在不同长期态势下选择不同语气。

### 路线 B：表情系统升级
- [ ] Iter 8b：扩展 mood 解析支持 expression 字段（如 [motion: Tap, expression: smile]），
  前端读到后切 Live2D expression 而不只是 motion group。

### 路线 C：隐私 filter
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 Iter 40 的
  Tag 统计观察实际命中率，决定是否要再加强 prompt。
- [ ] Iter Cx：在 prompt 构造层加可配置的 redaction（如 active_window 标题里某些 app
  名 / calendar event 关键词被替换为 "(私人)"）。settings 暴露 redaction patterns。

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
