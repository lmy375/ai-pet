# PanelTasks 顶部「📊 tag」分组 sort 模式（iter #256）

## Background

PanelTasks 顶部已有 `queue / due ↑ / P ↓` 三种排序模式。owner 在 #工作 /
#家务 / #health 三类 tag 混杂的长队列里想"集中处理同 tag 的 task"时，需要先
点 tag chip 过滤掉其它 tag —— 这是单 tag 视图。如果想"看全表但按 tag 分组
展示"（每个 tag 一段，便于扫读"每类有哪些"），目前没入口。

本迭代加第 4 个 sort 模式 `tag`：按 primary tag（`t.tags[0]`）字典升序分段，
每段顶有 `# 标签名` 子标题；无 tag 段 sentinel 排到末尾显 `🏷 无 tag`。
unfinished 段用此排序，finished 段仍按时间倒序（与 R94 既有约定一致）。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- **`sortMode` 类型扩展**：`"queue" | "due" | "priority"` → 增 `"tag"`。

- **`sortedUnfinished` 分支增加**：`sortMode === "tag"` 时按 `primaryTag(t)`
  字典升序 sort。空 tag → `"￿"` sentinel（U+FFFF 大于任何常用字符），
  让无 tag 段自然落到末尾。同 tag 内利用 JS stable sort 保持 backend
  compare_for_queue 综合序。

- **toggle UI**：第 4 个按钮 `📊 tag`，每个按钮挂独立 tooltip（mode 含义）
  让 owner 不必猜各模式区别。

- **section title 文案**：tag 模式下显"队列（按 primary tag 分段）"。

- **render loop 加 `showTagHeader` 计算**：与 R94 的 bucketHeader 同模式
  —— 仅在 unfinished 段跑（与 bucketHeader 在 finished 段跑互斥不冲突），
  相邻 task 的 primary tag 不同时插 header。

- **header 渲染**：复用既有 `s.bucketHeader` 样式；无 tag 显 `🏷 无 tag`，
  有 tag 显 `# {tagName}`。

## Key design decisions

- **primary tag = `t.tags[0]`**：实际场景里大部分 task 只有 1 个 tag；多
  tag 的 task 归到第一个 tag 是有损但简单的归类（与"#工作 #紧急" 显示在
  #工作 段下符合直觉）。owner 想精细看可用 tag chip 过滤。
- **sentinel `"￿"` 代表无 tag**：让无 tag 段自然 sort 到末尾，无需
  独立分支判断；render header 时再翻回 `""` 检查显示。
- **不动 finished 段**：finished 段已被 R94 bucketHeader 按时间分桶（今日 /
  昨日 / 本周）；tag 模式仅给 unfinished 段重排，避免与 bucketHeader 冲突
  / 双 header。
- **复用 `s.bucketHeader` 样式**：现有 header 视觉权重适中，再造一个 tag
  header style 会增加不一致；同样的 灰底 + count chip 模板让 owner 心智
  模型连贯。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.18s)

## Notes

- 拖拽 priority 仍只在 `sortMode === "priority"` 启用（dragEnabled 判断未
  动）；tag 模式下不能拖卡片改 tag — 那需要单独的 "drop into tag group"
  逻辑，本迭代不涉及。
- ⚡ NOW 标记的浮顶逻辑放在 sort 之后；tag 模式下若有 NOW 任务，它们仍
  会被推到队列首（不属于任何 tag 段），与既有 NOW 浮顶语义一致。
