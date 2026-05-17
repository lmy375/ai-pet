# PanelMemory 段内 fuzzy live filter（iter #318）

## Background

PanelMemory 已有搜索框 + 「搜索」按钮，调 backend `memory_search` 跨 cat
返命中列表。但工作流是"键入 → Enter → 切换到 results view" — 这种 mode-
switch 适合"我想搜什么"明确时，不适合"我大概知道在哪个 cat 但找不到具
体 item"的快速定位场景。

owner 想"快速过滤当前 cat 里 30 条 item，看哪条标题 / description 含 keyword"
时希望"边输入边过滤" UX —— 不必切换 view，cat 结构保留，只是不匹配的
item 隐藏。

本迭代复用既有搜索框输入，加段内 live filter：输入即过滤每个 cat 的 item
列表（title / description case-insensitive 子串匹配），Enter 仍走 backend
跨 cat search 切 results view。两种工作流分流，输入框一处用。

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- `scheduleFilteredItems` IIFE 顶部加新过滤段：
  - 计算 `inplaceFilter = searchKeyword.trim().toLowerCase()`
  - gate `searchResults === null`（未在 results view 时才 live filter）
  - 命中条件：`title.toLowerCase().includes(inplaceFilter) ||
    description.toLowerCase().includes(inplaceFilter)`
  - 命中后 `pool` 继续走既有 silent / schedule kind / sortByRecent /
    sortBulterByNextFire 管道，所有过滤维度叠加生效（AND 关系）
- 搜索框 placeholder 文案：`搜索记忆…` → `搜索记忆…（输入即段内过滤 ·
  Enter 跨 cat 命中清单 · ⌘F 聚焦）` 让 owner 发现新行为

## Key design decisions

- **复用 `searchKeyword` 而非新增 state**：避免两个并存输入框增加视觉
  复杂度。同一个搜索框承担两种工作流的 entry：typing = 段内过滤，
  Enter = backend 跨 cat 查询切 results view。owner 心智模型: "我想
  搜的范围逐步收窄 — 先在当前 view filter，要全局查就 Enter"。
- **gate on `searchResults === null`**：当 owner 已经按 Enter 进了
  results view 后，再 typing 不该再触发 cat 树过滤（这时候 cat 树本身
  不显，filter 也没意义）。保持两 mode 解耦。
- **case-insensitive 子串而非 fuzzy 编辑距离**：fuzzy 距离需引算法 +
  ranking 复杂度，且对 owner 不可预测（"我打 'down' 怎么命中了 'Drown'"）。
  子串匹配是 ack-everywhere 习惯（grep / find / Notion 搜索框都这样），
  确定性高。
- **title + description 双轨**：与 backend memory_search 同维度（避免
  "输入这个词 backend 找到但 live filter 没找到" 行为不一致）。detail.md
  不参与本 filter 因为：owner 期望边输边过滤 = 毫秒级响应，detail.md 是
  lazy-load 大文件（每个 N KB），同步过滤会卡。
- **AND 与既有 silent / schedule / sortByRecent 等过滤维度叠加**：放在
  `scheduleFilteredItems` IIFE 第一段即生效，下游所有维度自然继承。
  Owner 选了 "every" + "[silent]" + typing "周报" → 三条件交集，符合
  直觉。
- **不在 cat header 显 "filtered N/M" chip**：owner 已经在 input 看到
  自己输入的 keyword，cat header items 数量变化也是 visual feedback。
  加 chip 增加复杂度但收益小 — scope 不扩张。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.22s)
