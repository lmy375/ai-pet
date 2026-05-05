# 早安简报 — 开发计划

> 对应需求（来自 docs/TODO.md「已确认」）：
> 早安简报：每日固定时间将天气/日程/未读提醒/昨日小结合并成一段晨间播报。

## 目标

每天早晨在用户配置的时刻（默认 **08:30**），宠物以一次"主动发言"开口，把以下信息合成成一段自然语言播报：

- 当地天气（白天概览 + 是否需带伞 / 加衣）
- 今日日程（前几条）
- 已到期 / 即将到期的用户提醒
- 昨日的 daily_review 高亮（陪伴回看）

不是机械列条，而是宠物以自身人格 / 当前情绪重新组织成"早安你好，今天 X，记得 Y"的语气，符合 GOAL.md 的"实时陪伴 + 情绪价值"。

## 非目标

- 不做语音 TTS（保留文本 / 气泡形式即可）
- 不接 push 通知系统（依赖现有 proactive 气泡）
- 不替代每日回顾 daily_review（22:00 那条仍存在，互补不冲突）

## 设计

### 数据流

```
proactive tick (08:30+)
  └─ should_trigger_morning_briefing(now, last_date, settings)  ← 纯门控
       └─ true → trigger_morning_briefing(app)                   ← async, IO
            ├─ 拼装 intent 段（昨日 daily_review 摘要 + 用户名 + 当前情绪）
            ├─ 调用 chat pipeline（开放工具：weather / calendar / memory_list）
            ├─ LLM 自主补全天气与日程并生成播报
            ├─ 写入 speech_history（kind = morning_briefing）
            └─ 标记 LAST_MORNING_BRIEFING_DATE = today
```

### 模块划分（仿 daily_review）

| 文件 | 职责 | 是否纯函数 |
| --- | --- | --- |
| `src-tauri/src/proactive/morning_briefing.rs` | 门控 + intent 文本模板 + 测试 | 纯 |
| `src-tauri/src/proactive.rs`（增量） | async 触发器：调 chat、写 speech、置 last_date | IO |
| `src-tauri/src/commands/settings.rs`（增量） | `MorningBriefingConfig { enabled, hour, minute }` | 配置 |
| `src/components/panel/PanelSettings.tsx`（增量） | 启停开关 + 时间选择 | UI |

### 配置项（写入 settings.json）

```jsonc
"morning_briefing": {
  "enabled": true,
  "hour": 8,
  "minute": 30
}
```

默认开启 — 这是产品亮点，理想路径就是用户能感知到。用户可在面板关闭。

### 门控设计要点

- 与 daily_review 同结构：`now_hour/now_minute/today + last_briefing_date` → bool
- 进入触发时刻后窗口 N 分钟内若未触发就触发一次（避免 8:30 整点错过到 8:45 还是该开口）
- 进程内缓存 `LAST_MORNING_BRIEFING_DATE`，跨重启靠 speech_history 二次校验
- 与现有 mute / 专注模式 / 主动发言冷却的关系：尊重 mute 与专注模式（早安简报是低紧迫度），但**绕过普通主动发言冷却**（它有自己的"每日 1 次"语义）

## 阶段划分

| 阶段 | 范围 | 状态 |
| --- | --- | --- |
| **M1** | 本文档 + `morning_briefing.rs` 纯门控 + 单元测试 | ✅ 完成（14/14 测试，cargo check 通过） |
| **M2** | intent 文本模板 + async 触发 wrapper（接入 proactive 循环） | ✅ 完成 |
| **M3** | settings 字段 + 前端开关 / 时间选择 | ✅ 完成 |
| **M4** | 与 mute / focus / 冷却的交互测试 + 联调 | ✅ 完成（pure helper `morning_briefing_block_reason` + 4 条新单测） |
| **M5** | README 产品亮点更新 + 移出 TODO.md「已确认」、本文件移到 done/ | ✅ 完成 |

## 复用清单

- `proactive/daily_review.rs` — 直接借鉴门控结构与测试模式
- `proactive/time_helpers.rs` — 时间相关工具
- `tools/weather_tool.rs` / `tools/calendar_tool.rs` — LLM 在 turn 中自动调用
- `commands/memory.rs::memory_list` — LLM 取昨日 daily_review 用
- `speech_history` — 记录早安简报为新 kind
- `mood` — 当前情绪供 intent prompt 使用

## 待用户裁定的开放问题

1. **默认时间** 8:30 还是 9:00？（当前选 8:30，工作日上班族口径）
2. **节假日 / 周末**是否要不同时间？（当前一律 8:30，复杂度低）
3. **没装日历 / 天气**时是否仍触发？（建议：是，只播报已有部分 + 昨日回顾）

可在 M3 落地前由用户回写到本文件「答复」节。

## 进度日志

- 2026-05-04 11:50 — 创建本文档；准备进入 M1
- 2026-05-04 11:56 — M1 完成：`src-tauri/src/proactive/morning_briefing.rs` 提交，挂入 `proactive` 模块树。`cargo check` 通过（仅 dead-code warning，预期）；`cargo test --lib morning_briefing` 14 个测试全过。下轮进入 M2：写 `trigger_morning_briefing` async wrapper 并接到 `proactive::tick` 的合适分支。
- 2026-05-04 12:30 — M2-M5 一次性合到 main：
  - **M2**：`maybe_run_morning_briefing` async wrapper 落在 `proactive.rs`，与 `maybe_run_daily_review` 平行；spawn 循环每 tick 先跑早安再走 `evaluate_loop_tick`。LLM 调用走 `run_chat_pipeline` + `CollectingSink`，工具白名单由 chat 层注入（不在 intent 文本里硬编码）。跨进程幂等：`ai_insights/morning_briefing_YYYY-MM-DD` 标题写入 + `LAST_MORNING_BRIEFING_DATE` 静态。
  - **M3**：`MorningBriefingConfig { enabled, hour, minute }` 进 `AppSettings`；前端 `useSettings.ts`、`PanelSettings.tsx` 同步加类型 + UI 区块。默认 `enabled: true / 8:30`。
  - **M4**：抽出纯函数 `morning_briefing_block_reason(enabled, muted, focus_active, respect_focus)`，把"绕过 cooldown / 尊重 mute 与 focus"的优先级语义固化进单测。`cargo test --lib morning_briefing` 21/21 全过；全量 `cargo test --lib` 644/644 全过。
  - **M5**：README「主动聊天」段落加早安简报亮点；`docs/TODO.md` 移除条目；本文件移入 `docs/done/`。
- **开放问题答复**（来自上文"待用户裁定"节）：
  - Q1 默认时间：先选 8:30，看用户反馈再调（可面板自改，无需代码改动）。
  - Q2 节假日 / 周末：暂不区分，复杂度让位于"先跑顺一周"。
  - Q3 缺日历 / 天气时仍触发：是。LLM 拿不到工具结果会自动只组织已有信息，无需额外开关。
