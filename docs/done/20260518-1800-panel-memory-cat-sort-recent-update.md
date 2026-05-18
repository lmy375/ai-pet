# PanelMemory toolbar「🕒 cat 按近期 update」cat-level 排序 toggle（iter #583）

## Background

iter #558 加 `🔥 cat 按 7d 净增 desc` toggle — 看「新增数」growth quantity。
本 iter 补 cousin `🕒 cat 按最近 update desc` — 看「最近触摸时刻」latest
activity recency。两者互补 cat-level 视图：

- 🔥 7d 净增：本周加了多少条新 item（growth quantity）
- 🕒 最近 update：哪 cat 最近被动过（recency proxy）

owner 可能用任一：「这个 cat 有新东西吗」vs「这个 cat 我最近碰过吗」。

## Change

`PanelMemory.tsx`：

1. 新 `sortCatsByRecentUpdate` boolean state + localStorage 持久化
   （key `pet-memory-sort-cats-recent`），紧贴 `sortCatsByGrowth7d`
2. 新 toolbar 按钮「🕒 cat 按近期 / cat 近期 -」（tint-cyan active 态
   与 🔥 tint-orange 错开）
3. ordering 逻辑：在 7d 净增分支之后加 recent-update 分支：
   ```tsx
   if (sortCatsByRecentUpdate) {
     // 算 max(items.updated_at) per cat
     // 有 updated_at 的按 desc 提顶；空 / 无 updated_at 末尾
   }
   ```

## Key design decisions

- **7d 净增 sort 优先于 recent**：if (sortCatsByGrowth7d) 早 return；
  仅在 7d 净增 toggle 关时才走 recent-update 路径。两 toggle 都开时
  7d 净增赢 — pragmatic：通常 owner 只开其中一个
- **空 cat / 无 updated_at 末尾保 default 序**：与 7d 净增 sort 同决策
  — 关 toggle 后 inactive cat 不视觉跳
- **tint-cyan active 态**：与 🔥 tint-orange 视觉错开 — 两 cat-level
  sort 状态可一眼区分（pinnedFilter 是 amber、idle 是 red、7d 净增 是
  orange、本 toggle 是 cyan）
- **localStorage 独立 key**：`pet-memory-sort-cats-recent`（不是 `-7d`
  的 alias）— 让 owner 偏好 fine-grained
- **7d 净增 与本 toggle 互斥语义**（虽然实现里 7d 优先）：toolbar 视觉
  上同时浮 cyan + orange 时 owner 会困惑「现在按哪个排」；future iter
  可考虑 radio 化（only-one-active）— 但当前互斥已隐含在 if-else 优先
  级里

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — toggle 加在熟悉位置，同 state pattern，无 layout
  race / sort 优先级 race（两 toggle 都开时 7d 优先 deterministic）

## Future iters (out of scope)

- **radio-style cat-sort picker**：3 个 cat-sort toggle（default / 7d 净
  增 / 最近 update）改成单 dropdown 选 1 个 — 减视觉密度 + 互斥语义
  显式。但 dropdown 比 toggle 多 1 步交互
- **「cat 按 30d 净增」cousin**：与 7d 配 30d 长周期 sort — 完成 cat-
  sort 矩阵。按需 propose
- **「cat 按 stale」反向 sort**：last update 最早的 cat 顶上 = stale
  cat 优先 audit。/cat_decay 类语义，但 toolbar 化 — 按需
