# 决策日志支持复制单行 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 决策日志支持复制单行：每条 decision 行 hover 出现 "复制" 按钮，复制 `[HH:MM] kind reason` 到剪贴板，方便贴到 issue / debug 笔记里追问。

## 目标

排查"为啥宠物这小时一直 Skip"时，用户经常想把某条具体决策贴进 issue
/ Slack / 调试笔记。当前没复制入口，得手动框选三个 span 文本（容易少
选 / 多选）。本轮在每行 hover 时显示一个 "复制" 按钮，一键拼好整行
markdown-friendly 字符串。

## 非目标

- 不动 Spoke/LlmSilent 行的"重跑"按钮 —— 那是另一种动作，复用同列没问题
  但语义不同。复制按钮单独存在，与"重跑"并列。
- 不批量复制 N 条 —— 用 reason 搜索缩小范围 + 浏览器 select-all 已能办
  到；批量按钮反而占位。
- 不做"复制为 markdown link" / "复制为 jsonline" —— 单一格式足够；多
  format 选择反而让用户犹豫。

## 设计

### 复制格式

`[{timestamp}] {kind} {reason}` 单行：
- timestamp 用原始 RFC3339（精确到秒，便于跨日志对照）
- kind 大写驼峰原样
- reason 用**原始** d.reason 而非 localizeReason —— 原始字符串包含
  `cooldown (60s < 1800s)` 这种数字细节，本地化后会丢一些；贴出去给
  自己 / 他人 debug 时更有价值

例：
```
[2026-05-07T01:05:30+08:00] Skip cooldown (60s < 1800s)
```

### UI

每行末尾（在"重跑"按钮**之前**，因为 hover 复制对所有 kind 都有效）插
一个小按钮，hover-only 显示：
- 默认 opacity 0；行 hover 时 opacity 1（CSS sibling pattern）
- 点击 → clipboard.writeText + 2s "已复制" 文案反馈
- ack 复用既有 `setCopyMsg` 状态（与 prompt/reply 复制按钮共用 toast）

### 实现

按钮内联在 row map 里。透明度切换通过 className + 既有 panel-decision-row
hover CSS（新增），与既有 pet-detail-copy-btn 同模式。

## 测试

PanelDebug 是 IO 重容器；前端无 vitest，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | hover-only CSS + 复制按钮 + clipboard write + ack |
| **M2** | tsc + build + cleanup |

## 复用清单

- 既有 `setCopyMsg` toast
- 既有 navigator.clipboard.writeText
- 既有 row 视觉布局

## 进度日志

- 2026-05-07 02:00 — 创建本文档；准备 M1。
- 2026-05-07 02:10 — M1 完成。`<style>` 块内联 `.pet-decision-row` hover-only 显隐 CSS（同 PanelTasks/Chat 的 `.pet-*-copy-btn` 模式）；每行加 className + 复制按钮；格式 `[timestamp] kind reason`（原始 reason 比 localized 适合贴 issue）；ack 复用既有 `setCopyMsg` toast 1.5s 自动复位。
- 2026-05-07 02:15 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 931ms)。归档至 done。
