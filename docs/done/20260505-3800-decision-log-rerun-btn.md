# 决策日志条目"立即重跑"按钮 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 决策日志条目"立即重跑"按钮：Spoke / LlmSilent 行后加一个小按钮，点击就 `trigger_proactive_turn` —— 调试 prompt 时再跑一次最快。

## 目标

Prompt 调试时常见动作：改了 SOUL.md / settings.proactive 之后想立刻看看新版
prompt 在同一个上下文里产出什么。当前只能去顶部工具栏点「立即开口」。本轮在
决策日志的 Spoke / LlmSilent 行右端加一个小「重跑」按钮，调既有
`trigger_proactive_turn` 命令，省去一次顶 → 底视线切换。

## 非目标

- 不为 LlmError / Skip / Run 行加按钮 —— Skip 是 gate 阻断（重跑会重复阻断）；
  LlmError 是 LLM 调用失败（重跑常见结果是同样失败，价值低）；Run 是 wrapper
  无 outcome。仅 Spoke / LlmSilent 是"LLM 跑出结果但你想换条件再看一次"的对象。
- 不做"用同一 prompt 重跑"——`trigger_proactive_turn` 是从 0 重新构建 prompt
  的入口，与"看新 prompt 怎么变"语义对齐；保留旧 prompt 测试是另一类需求。
- 不写 README —— 调试器维护补强。

## 设计

### 按钮逻辑

复用既有 `handleTriggerProactive` async handler + `triggeringProactive` busy
状态（与顶部「立即开口」按钮共享 lock —— 避免双触发）。每行末尾加：

```tsx
{(d.kind === "Spoke" || d.kind === "LlmSilent") && (
  <button
    onClick={handleTriggerProactive}
    disabled={triggeringProactive}
    title="立即用最新 prompt 重跑一次主动开口（与顶部「立即开口」共用 trigger_proactive_turn）"
    style={{
      fontSize: 10,
      padding: "1px 6px",
      borderRadius: 4,
      border: "1px solid #cbd5e1",
      background: triggeringProactive ? "#f1f5f9" : "#fff",
      color: triggeringProactive ? "#94a3b8" : "#475569",
      cursor: triggeringProactive ? "not-allowed" : "pointer",
      flexShrink: 0,
    }}
  >
    {triggeringProactive ? "…" : "重跑"}
  </button>
)}
```

放在外层 row div 的最后子元素，flex 让 reason span 自然 push 它到右端。
busy 时所有同类按钮一起灰掉（包括顶部那个）。

### 测试

无后端改动；纯 UI 微调。靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 行末按钮接入既有 handleTriggerProactive |
| **M2** | tsc + build + cleanup |

## 复用清单

- 既有 `handleTriggerProactive` / `triggeringProactive` busy 锁
- 既有 decision row flex 布局

## 进度日志

- 2026-05-05 38:00 — 创建本文档；准备 M1。
- 2026-05-05 38:05 — 完成实现：`PanelDebug.tsx` 决策日志行末追加条件渲染的「重跑」按钮（仅 Spoke / LlmSilent 行显示），onClick 复用既有 `handleTriggerProactive`，busy 状态共享 `triggeringProactive`（与顶部「立即开口」按钮共锁）。`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 调试器维护补强。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；仅一次条件渲染，由 tsc + 既有 trigger 路径保证。
