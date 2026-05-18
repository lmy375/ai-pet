# PanelMemory cat-sort radio refactor（iter #590）

## Background

iter #558 加了 `🔥 cat 按 7d 净增` toggle；iter #583 加了 `🕒 cat 按
近期 update` toggle。两 toggle 各自独立 boolean — owner 同时开两个时
两 chip 都浮 active 态视觉混乱；ordering 分支用 priority fallback 隐
含「7d 赢 recent」语义 owner 看不出。

本 iter 把两 toggle 重构为单 radio group 3-mode 互斥 — 「⊕ 默认 /
🔥 7d / 🕒 近期」visible 状态唯一。

## Changes

`PanelMemory.tsx`：

1. 新 `selectCatSortMode(mode: 'default' | '7d' | 'recent')` setter —
   mutually exclusive 设两 boolean + 同步写两 localStorage key
2. 新 derived `catSortMode: 'default' | '7d' | 'recent'` — 从 booleans
   推导（7d 优先与 ordering 分支一致）
3. 删旧 `toggleSortCatsByGrowth7d` / `toggleSortCatsByRecentUpdate`
   （toolbar 不再 callers）
4. toolbar 替换两 toggle 按钮 为单 radio group:
   ```tsx
   <span role="radiogroup">
     <button role="radio" aria-checked={catSortMode==='default'}>⊕ 默认</button>
     <button role="radio" aria-checked={catSortMode==='7d'}>🔥 7d</button>
     <button role="radio" aria-checked={catSortMode==='recent'}>🕒 近期</button>
   </span>
   ```
   3 button 紧凑 inline-flex，borderRadius:0 (group)，active 态染 tint
   与既有 mode 配色一致（default→muted / 7d→orange / recent→cyan）

## Key design decisions

- **保留 boolean state + localStorage key**：让 backward-compat 用户
  偏好 transparent migrate — 旧用户的 sortCatsByGrowth7d=1 自动 → 新
  radio active='7d'
- **`selectCatSortMode` 写两 key 而非 unified**：避免新增 third
  localStorage key + 让 ordering 分支不变（仍读两 boolean）
- **radio 用 ARIA role**：accessibility 标识 group + checked 状态
- **删旧 toggle 函数**：toolbar 不再 callers；保留导致 TS unused
  warning — clean delete 比 `#[allow]` 干净
- **优先级与 derived state 一致**：catSortMode 推导用 `7d ? '7d' :
  recent ? 'recent' : 'default'` — 与 ordering 分支「if(7d) ... else
  if(recent) ...」同优先级；选 '7d' 时强制 recent=false 实现互斥
- **保留 ordering 分支不动**：ordering 用 two-boolean if-else 仍
  works — radio refactor 只改 UI 入口，逻辑层不变

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — UI 改造但 state pattern 与 localStorage 不变；
  既有偏好 transparently 工作

## Future iters (out of scope)

- **3rd cat-sort mode**：加「按 cat 容量 desc」radio option — capacity
  axis（与 TG /cat_top iter #589 对偶）。3 option 容易扩；按需 propose
- **state unify to single string**：把 `sortCatsByGrowth7d` /
  `sortCatsByRecentUpdate` 两 boolean 合并为单 `catSortMode: 'default'
  | '7d' | 'recent'` state + 单 localStorage key。需 migration shim
  读两旧 key；按需 propose
- **keyboard shortcut**：⌘1 / ⌘2 / ⌘3 切 radio mode — 比鼠标点更快。
  现有 PanelMemory shortcuts 已多；按需谨慎扩
