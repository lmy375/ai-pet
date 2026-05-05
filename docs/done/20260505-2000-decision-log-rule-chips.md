# 决策日志 Spoke 行 prompt 标签 chip — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 决策日志过滤已支持 4 类，`Spoke` 项内嵌 prompt 标签 chip：标签命中（如 `chatty`）一眼可见而不必 hover tooltip。

## 目标

调试面板的"主动开口判断"决策日志里，`Spoke` 行的 reason 是一坨 csv：
`chatty=5/5, source=loop, rules=icebreaker+chatty, tools=...`。`localizeReason`
把它包成"宠物开口（chatty=5/5, source=loop, rules=icebreaker+chatty, tools=...）"
单行，用户要扫"今天哪几个 prompt rule 在生效"得逐字读字符串。

本轮在 Spoke 行的局部化文案旁边，单独渲染**每个 rules 标签一个小 chip**（紫色
pill），让 `chatty` / `icebreaker` 等标签命中一眼可见。其它 tag（chatty=N/M /
source / tools）保持原文案不变。

## 非目标

- 不为 LlmSilent / LlmError / Skip 行加 chip —— 它们的 reason 不含 rules
  字段，本轮专注 Spoke。
- 不为 tools 加 chip —— tools 列表已在「环境工具命中率」段独立展示，本行
  重复展示无新信息。
- 不改 reason 字符串本身格式（后端 `record_proactive_outcome` 不动）—— 仅
  在前端解析显示。
- 不写 README —— 调试面板信息架构补强。

## 设计

### 解析

新 pure helper（放在 PanelDebug.tsx 内部，靠近 localizeReason）：

```ts
/// 从 Spoke 决策的 reason csv 里提取 rules=A+B+C 的标签数组。空 / 缺失返回 []。
function parseSpokeRules(reason: string): string[] {
  const parts = reason.split(", ");
  const rulesPart = parts.find((p) => p.startsWith("rules="));
  if (!rulesPart) return [];
  const value = rulesPart.slice("rules=".length).trim();
  if (value.length === 0) return [];
  return value.split("+").map((r) => r.trim()).filter((r) => r.length > 0);
}
```

格式契约：`record_proactive_outcome` 只在 `rules_tag = Some("rules=A+B")` 时
push，确保 split("+") 在标签自身名内不冲突（标签名约定不含 `+`）。

### 渲染

Spoke 行 JSX 现状是 `<span>localizeReason(d.kind, d.reason)</span>`。本轮在
该 span 后再加 `parseSpokeRules(d.reason).map(...)` 渲染 chip。

```tsx
{d.kind === "Spoke" && parseSpokeRules(d.reason).length > 0 && (
  <span style={{ display: "inline-flex", gap: 4, marginLeft: 6 }}>
    {parseSpokeRules(d.reason).map((label) => (
      <span key={label} style={ruleChipStyle}>{label}</span>
    ))}
  </span>
)}
```

`ruleChipStyle`：紫色 background `#ddd6fe`、深紫文字 `#5b21b6`、padding `0 6px`、
border-radius 8、fontSize 10、lineHeight 1.4。与既有 priBadge / mood-pill 同样
style 风格但配色独立（避免和已有 chip 撞色）。

### 测试

`parseSpokeRules` 是 pure，不带前端依赖。但项目无前端 vitest 配置；纯逻辑
极小（4 行），靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | parseSpokeRules + ruleChipStyle |
| **M2** | Spoke 行 JSX 加 chip 渲染 |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 `localizeReason` 不动
- decision_filter 与 PanelFilterButtonRow 不动

## 待用户裁定的开放问题

- chip 颜色：紫（`chatty` 等是 prompt-rule 性质，紫色与 mood-tag 颜色族
  独立）。也可用 cyan，本轮选紫保持调试面板内的 chip 配色多样。
- 多 rule 时 chip 是平铺 vs 折叠？本轮平铺 —— 单 turn 同时命中的 rule 数量
  实战很少 > 3，平铺直观。

## 进度日志

- 2026-05-05 20:00 — 创建本文档；准备 M1。
- 2026-05-05 20:15 — 完成实现：
  - **M1**：`PanelDebug.tsx` 加 `parseSpokeRules(reason)` pure helper（split csv → 找 `rules=` 段 → split `+`）+ `ruleChipStyle` 紫色小 chip 样式（与既有 mood-tag / pri-badge 配色错开）。
  - **M2**：决策日志 Spoke 行 JSX 加内嵌 chip 渲染：`localizeReason` 文案后追加 `<span flex>{labels.map(<span chip>)}</span>`，chip 含 hover title 解释 prompt 软规则命中。
  - **M3**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 调试面板信息架构补强。
  - **设计取舍**：仅渲染 rules 字段的 chip，不为 tools 加 chip（环境工具命中率段已独立展示）；后端 reason 字符串格式不动（前端 parse-only）；标签名约定不含 `+` 让 split 安全。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；`parseSpokeRules` 是 pure 4 行，由 tsc + record_proactive_outcome 既有测试侧面验证（产 reason 字符串的格式契约在后端单测里钉牢）。
