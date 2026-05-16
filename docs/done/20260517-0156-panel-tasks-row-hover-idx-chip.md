# PanelTasks 列表行 hover 时显当前行 idx / total 微小角标

## 背景

owner 在长队列（20+ tasks）滚动时常想"我现在看的这条排第几 / 整队列多大"。当前面板没显行号 / 位置信号 —— scroll bar 粗略告诉但不准。

加 hover row 时右上角浮"idx / total" 9px muted 角标。

## 改动

### `src/components/panel/PanelTasks.tsx`

任务行容器内（既有 hover preview tooltip 之前）插入 idx chip：

```tsx
{taskPreviewHoverTitle === t.title && visibleTasks.length > 5 && (
  <span
    style={{
      position: "absolute",
      top: 4,
      right: 6,
      fontSize: 9,
      color: "var(--pet-color-muted)",
      fontFamily: "monospace",
      background: "var(--pet-color-card)",
      padding: "0 4px",
      borderRadius: 3,
      lineHeight: "12px",
      opacity: 0.6,
      pointerEvents: "none",  // click 穿透到 row hit area
      zIndex: 5,
    }}
    aria-hidden
  >
    {idx + 1} / {visibleTasks.length}
  </span>
)}
```

## 关键设计

- **`visibleTasks.length > 5` gate**：≤ 5 条队列 owner 一眼就看全，idx 反成噪音。> 5 才有定位价值。
- **复用 `taskPreviewHoverTitle === t.title` hover state**：与既有 hover preview 同 trigger gate；mouseLeave 自然清。
- **pointerEvents none**：让 row click / dragger / 右键菜单等不被 chip 拦截。
- **`idx + 1` 一基显**："1 / 20" 比 "0 / 19" 直观 —— 与 owner 心算"第 N 条"自然一致。
- **opacity 0.6 + muted color + 9px**：ambient 角标语言，与 ChatMini bubble 顶 ts chip / 底 ⏱ chip 相同存在感等级。
- **position: absolute top:4 right:6**：右上角浮 —— 不挤 row title / chips 区。
- **zIndex 5 较低**：不与 hover preview tooltip (zIndex 20) 等其它 popover 冲突。

## 不做

- **不绑 click → jump page**：scope creep；scroll 已自然提供位置感。
- **不显 keyboard-nav focused idx**：keyboard nav 已有 outline + 蓝色 focus 视觉。
- **不写测试**：纯 conditional render；视觉验证（队列 > 5 时 hover 某行 → 右上应显角标）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.20s
- 改动 ~25 行（chip + 注释）。既有 hover preview / drag drop / focus outline / 右键菜单 / item style 路径完全不动。

## TODO 状态

剩 3 条留池：
- PanelMemory item description 行级 hover preview 含完整内容
- ChatPanel 输入框历史栈 hover 显 idx / total
- detail.md 编辑器 ⌘K 唤起 task quick-find palette

## 后续

- 同款 idx 角标给 PanelMemory list rows（长列表也有"我看的是第几条"需求）。
- ⌘G / Cmd+G 弹"跳到第 N 条" mini input 让 keyboard owner 精准 jump。
- queue depth 极大（>100）时 idx chip 也显 "P{visibleTasks[idx].priority}" 让 owner 看到位置 + 优先级联动信息。
