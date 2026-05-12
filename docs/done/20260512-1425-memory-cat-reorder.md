# PanelMemory category 顺序自定义

## 需求

PanelMemory 顶层把 memory 按 category 分 section 展示，顺序由前端
const `CATEGORY_ORDER = ["butler_tasks", "todo", "ai_insights",
"task_archive", "general", "user_profile"]` 写死。不同用户使用模式不
同：有人最在意 todo / 提醒，有人 daily 看 ai_insights 反思，有人重
点维护 user_profile 档案。让用户拖 section header 改顺序，localStorage
持久化（与已存在的 categoryLabels / pinnedKeys / expandedCategories
同模式）。

## 实现

`src/components/panel/PanelMemory.tsx` 单文件：

- 新增 state `savedCatOrder: string[]`，localStorage key
  `pet-memory-cat-order`：
  - 默认 `[]` —— 用户没拖过即走原 CATEGORY_ORDER 默认
  - 用户拖一次后存"完整 effective 顺序"，保证下次稳定还原
- 新增 helper `persistCatOrder(order)`：set state + 写 localStorage
- 渲染时计算 `effectiveOrder`（IIFE，inline 在 .map 前）：
  1. `savedCatOrder` 按其顺序首批
  2. `CATEGORY_ORDER` 未在 saved 里的接其后
  3. `Object.keys(index.categories)` unknown 的（运行时新增类目）接末尾
  - 用 `seen Set` 去重；最后 .filter cat 实际存在
- 新增 state `dragSrcCat` / `dragOverCat`：拖动来源 / hover 目标
- section title 第一个位置加 `⋮⋮` drag handle（draggable=true）：
  - `cursor: grab`，淡灰 muted 色，hover tooltip 提示功能
  - `onDragStart` 记 source；`onDragEnd` 清两个 state
- section 外层 div listens `onDragOver` / `onDragLeave` / `onDrop`：
  - dragOver 上保留 `e.preventDefault()` + `dropEffect=move`
  - drop 实现：构 cur effective order（同 render 时的逻辑），filter 掉
    src，再 splice 到 target 当前位置。等同"drop before target"语义
- 视觉反馈：
  - source section 透明度 0.4（与 PanelTasks 拖卡视觉同款）
  - target section dashed accent outline + 6px 圆角
- `localStorage.setItem` 全部 try/catch（私密模式 / 容量满）

## 验证

- `cargo check`：clean（无后端改）
- `npx tsc --noEmit`：clean
- 行为：
  - 拖 ⋮⋮ handle 把 ai_insights 拖到 butler_tasks 上方 → 顺序对调，
    刷新 panel 仍维持
  - 拖 todo 到末尾 → 持久化；下次打开"todo 在最末"仍然成立
  - localStorage 清掉 `pet-memory-cat-order` → 回到默认 CATEGORY_ORDER
  - 后端将来新增 category（不在 saved / CATEGORY_ORDER 里）→ 自动接
    到末尾，用户拖动后会被纳入 saved
  - 拖到原位置（src===target）→ noop，state 清；不写 storage
  - 拖 onDragStart 触发后离开窗口 → onDragEnd 清 state，防 dangling

## 不在本轮范围

- 没做"drop above / below" 双区指示器：单 outline + drop-before 已经
  覆盖了 99% 拖动语义；加双区会让 drop zone 视觉变碎
- 没加 "重置默认顺序" 按钮：现 UI 已经够紧；用户想重置可拖回去 / 清
  localStorage。如果后续用户反馈混乱再补
- 没做 keyboard reorder（↑↓ 调整聚焦行）：键盘可访问性不在本轮目标，
  panel 主要走鼠标 / 触控板
- 搜索结果视图不受影响：那是 flat 单 header，不分 category

## TODO 池剩余

空。下一轮需自主提需求。
