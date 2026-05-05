# proactive 调试器 prompt 字数 token-pressure 提醒 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> proactive 调试器面板的 prompt 字数提醒：当 prompt > 8000 char 时把字数标红，提醒 prompt 工程师 token 即将吃紧。

## 目标

`PanelDebug` modal header 当前用灰色显示 `prompt N / reply M chars`。N 接近
context 上限时（实战中文 LLM 单 turn ≤ 16K-32K context，prompt 8K char ≈ 一半
context 已被占）需要视觉警示。本轮在 N > 8000 时把数字标红 + 加一段 hover
tooltip 解释门槛。

## 非目标

- 不做精确 token 计数（涉及 tokenizer 依赖）—— char count 作为粗 proxy 已足够
  让 prompt 工程师感知"接近水位"。
- 不做对 reply 长度做警告（reply 通常 < 500 字符，警示意义低）。
- 不写 README —— 调试器内嵌交互微调。

## 设计

8000 char 阈值取自经验：中文 LLM ~3 char ≈ 1 token，8000 char ≈ 2700 tokens；
配上常见 6K context 留 ~3.3K 给 reply 是较紧的水位。

```tsx
const PROMPT_PRESSURE_CHARS = 8000;
const promptOver = lastPrompt.length > PROMPT_PRESSURE_CHARS;

<span
  style={{
    fontSize: "11px",
    color: promptOver ? "#dc2626" : "#94a3b8",
    fontWeight: promptOver ? 600 : 400,
  }}
  title={promptOver
    ? `prompt 超过 ${PROMPT_PRESSURE_CHARS} char（约 ${Math.round(lastPrompt.length / 3)} tokens），离 context 上限不远。考虑收紧 system soul / 减少 tools / 调小 max_context_messages。`
    : undefined}
>
  {lastPrompt ? `prompt ${lastPrompt.length} / reply ${lastReply.length} chars` : ""}
</span>
```

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 阈值常量 + 颜色 / weight / tooltip 条件渲染 |
| **M2** | tsc + build + cleanup |

## 复用清单

- 既有 `lastPrompt` / `lastReply` 派生

## 待用户裁定的开放问题

- 阈值 8000 vs 6000 vs 12000？8000 是 ~half 16K context 的"紧但未爆"档；如反
  馈嫌早 / 嫌晚再调。
- 是否对 `tools` 也提醒？工具数过多有自己的 cache hit 指标，本轮只动 prompt
  字数。

## 进度日志

- 2026-05-05 32:00 — 创建本文档；准备 M1。
- 2026-05-05 32:05 — 完成实现：`PanelDebug.tsx` 加 `PROMPT_PRESSURE_CHARS = 8000` 常量；prompt char count span 改 IIFE 渲染 —— promptOver 时颜色 `#dc2626` + fontWeight 600 + hover tooltip 解释（中文 char/token 比 + 收紧建议）。`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 调试器内嵌交互微调。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；纯条件渲染由 tsc 保证。
