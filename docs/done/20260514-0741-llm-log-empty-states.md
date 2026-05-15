# LlmLogView 空态用共享 `EmptyState`

## 背景

LlmLogView 有两条裸 `<div>` 空态：
1. `entries.length === 0` → "暂无 LLM 日志..."
2. `visibleEntries.length === 0` → "当前过滤无命中..."

其它 panel 都已迁到 `EmptyState`（统一 icon + title + hint + compact 节奏）。LlmLogView 是最后一片漏网区。

## 改动

`src/components/panel/LlmLogView.tsx`：

- import `EmptyState`
- 第一条："📜" + "暂无 LLM 日志" + "发送聊天消息后会产生记录。"
- 第二条："🔍" + "当前过滤无命中" + '点上方"清过滤"或扩大窗口（"加载更早"）。'

icon 选择与 PanelTasks / PanelChat 同款（🔍 = filter empty，📜 = log dump）。

## 不做

- 不抽 LlmLogView 其它部分；只换两段空态
- 不动错误反馈 banner（不属空态范畴）

## 验收

- `npx tsc --noEmit` ✅
- 「调试」tab LLM logs section：无日志 / 过滤无命中 → 两条空态走 EmptyState 节奏

## 完成

- [x] LlmLogView.tsx: 两段 inline div 替换 EmptyState
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
