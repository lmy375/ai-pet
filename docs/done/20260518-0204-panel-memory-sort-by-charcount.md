# PanelMemory「📏 按字数」sort toggle（iter #348）

## Background

PanelMemory 已有「📅 按时间 / 默认序」toggle (sortByRecent) + butler_tasks
段「⏰ next-fire 升序」toggle。但缺"按 content 体量排序"维度 — owner
想 audit 「哪些 memory item content 最重 / 该 consolidate / 该拆分」时
只能逐条手动扫，没快速排序入口。

本迭代加「📏 按字数」toggle：rest 段按 description char count + detail.md
字数总和倒序排。

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- 新 state `sortByCharCount: boolean` + localStorage 持久（key
  `pet-memory-sort-charcount`；与 sortByRecent / sortBulterByNextFire
  同 pattern）+ `toggleSortByCharCount` 函数
- 在 sortByRecent 之后插「📏 按字数」chip toggle button：
  - 激活态走 tint-blue 染底（与 sortByRecent 同视觉一致）
  - tooltip 区分两态行为
- pipeline 在 useNextFire 之后 / sortByRecent 之前插新分支：
  - `sizeOf(it) = Array.from(description).length + (detailSizes[detail_
    path] ?? 0)`
  - cmpSize: 倒序（最大在前）
  - pinned + rest 同 cmpSize 排
- sortedItems 兜底 fallback 条件加 `|| sortByCharCount` 让 rest 段排完
  仍能 fallthrough

## Key design decisions

- **三态互斥优先级 next-fire > 字数 > recent > 默认**：next-fire 是
  butler_tasks 专属（最具体语义），字数次之（content audit），recent
  最泛（时间维度）。同时开多 toggle 时按具体程度优先匹配。
- **`Array.from().length` 按字形计数**：与 PanelMemory 其它字数显（item
  hover preview 字数 / 📊 total chars chip）同算法 — emoji / 中文 /
  surrogate pair 统一按字形计数。`pd.description.length` UTF-16 code
  unit 对中文算 1 / emoji 算 2 不一致。
- **detail.md size 走 detailSizes 缓存**：detailSizes 在 mount + index
  变化时已 refresh，sort 时零 IPC。缺失（未刷新 / IO 失败）→ 0，让该
  item 按"仅 description 字数"排，行为可预期。
- **pinned 仍挂头**：与既有 sortByRecent / sortBulterByNextFire 同
  pattern — pinned 是"owner intent 强信号"，应当压过任何排序策略。
- **localStorage 持久**：与既有两 sort toggle 同 pattern — owner 切走
  再回到 panel 偏好保留。
- **不引入 unit test**：sort comparator 纯算法 + detailSizes 缓存依赖；
  jsdom 下 detailSizes IPC 难 mock；通过 vite build + 真实交互验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.24s)
