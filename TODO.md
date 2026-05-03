# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一迭代候选（优先级从高到低）
## 下一阶段（Iter 100 盘点后重排，2026-05-03 用户加入"宠物管家"方向）

**当前主轴是路线 F（宠物管家），其他路线退为辅助优先级。** 跨设备同步已被用户明确移除。

### 路线 G：companion register 细化（小迭代）
- [x] Iter Cμ：proactive prompt 时间行加 user_absence_tier 语气线索（2026-05-03 完成）
- [x] Iter Cν：long-absence-reunion 复合规则（≥4h 用户离开 + under_chatty + !pre_quiet
  → 触发"重逢感"提示）（2026-05-03 完成）
- [x] Iter Cξ：first-of-day 环境规则（today_speech_count == 0 → 用当下时段问候打底）
  （2026-05-03 完成）
- [x] Iter Cο：PanelPersona 加"当下心情"区（motion emoji + 文字 + 空状态）
  （2026-05-03 完成）
- [x] Iter Cρ：companionship-milestone 数据驱动规则（满 7/30/100/180 天/年/周年→
  engagement 类提示）（2026-05-03 完成）
- [x] Iter Cσ：reactive chat 的 user_profile 捕捉引导 — 闭合 Iter Cα 注入 ↔ 捕捉对称
  （2026-05-03 完成）
- [x] Iter Cτ：settings.user_name 字段 + persona_layer 称呼注入（reactive chat / Telegram）
  （2026-05-03 完成）
- [x] Iter Cυ：把 user_name 也注入 proactive prompt — 让 bubble 主动开口偶尔用名字称呼
  （2026-05-03 完成）
- [x] Iter Cφ：PanelPersona "自我画像" 空态加"立即生成画像"按钮 — 空态内嵌 consolidate
  trigger，新装用户一键 unlock（2026-05-03 完成）
- [x] Iter Cχ：butler_tasks 一键"清除失败标记" ✕ 按钮 — 跟 ❌ chip 紧贴，单击 strip
  `[error: ...]` 保留其余 description（2026-05-03 完成）
- [x] Iter Cψ：PanelStatsCard 加 "上次开口 N 前" 列 — 复用 ToneSnapshot
  since_last_proactive_minutes，五列横排（今日/本周/累计/上次/陪伴）（2026-05-03 完成）

### 路线 F：宠物管家（用户委托执行实际工作）— Iter Cγ 起步
- [x] Iter Cγ：butler_tasks 记忆类别 + 提示注入 + tools 描述 + panel 顺序（2026-05-03 完成）
- [x] Iter Cδ：panel 顶部 "+ 委托任务" 快捷入口 + 模态分类 placeholder（2026-05-03 完成）
- [x] Iter Cε：butler_history.log + panel "最近执行" 时间线（2026-05-03 完成）
- [x] Iter Cζ：butler_tasks 调度前缀 `[every:]` / `[once:]` + 到期标注（2026-05-03 完成）
- [x] Iter Cη：每日 butler 小结写入 butler_daily.log + panel "每日小结" 区（2026-05-03 完成。
  注：用独立文件而非 speech_history，避免污染 chatty 计数）
- [x] Iter Cθ：panel butler_tasks 调度 chip + 实时 ⏰ 到期标记（2026-05-03 完成）
- [x] Iter Cι：reactive chat 的 butler 委托引导 — 扩展 TOOL_USAGE_PROMPT 让用户从聊天
  自然委托任务（2026-05-03 完成）
- [x] Iter Cκ：butler_tasks "等了 Nh" 过期指示 + 一键"立即处理"逃生口（2026-05-03 完成）
- [x] Iter Cλ：completed `[once]` butler_tasks 自动清理（48h grace）+ settings 配置项
  （2026-05-03 完成）
- [x] Iter Cπ：butler_tasks 执行失败回退 — `[error: 原因]` description 标记 + 红 chip
  （2026-05-03 完成）
- 路线 F 真闭环。后续看使用数据再加（任务依赖 / 监督模式 / batch 操作 etc）。

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
- [x] Iter 74：speech_daily.json 扩展 panel stats 卡为"今日 / 本周 / 累计"三列（2026-05-03 完成）
