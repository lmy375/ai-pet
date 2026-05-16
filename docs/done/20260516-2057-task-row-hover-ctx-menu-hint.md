# PanelTasks 任务行 hover preview 底加 "右键查看所有操作" onboarding hint

## 背景

PanelTasks 任务行右键菜单含 10+ 高频操作：done / retry / cancel / priority / snooze 预设 / pin / silent / 复制标题 / 复制 ref / 复制 markdown / 复制为引用块 / 复制 detail.md 绝对路径。

但首次用户无法发现 —— 行本身只有 click 折叠 + drag 改 priority 视觉提示。iter #201 给 PanelMemory item hover 加了"✏️ 双击编辑" hint；本 iter 给 PanelTasks 任务行 hover preview tooltip 底脚加对偶的 "🖱️ 右键查看所有操作" hint。

## 改动

### `src/components/panel/PanelTasks.tsx`

在 hover preview tooltip 最后一段（detailSnippet 段）之后加 hint footer：

```tsx
<div
  style={{
    marginTop: 6,
    paddingTop: 4,
    borderTop: "1px dashed var(--pet-color-border)",
    fontSize: 9,
    color: "var(--pet-color-muted)",
    fontStyle: "italic",
    opacity: 0.7,
  }}
>
  🖱️ 右键查看所有操作（done / 改 priority / snooze / pin / silent / 复制 / ...）· 点击行 折叠/展开
</div>
```

- dashed-top divider 与既有 chips / history / detail 段拉开视觉
- fontSize 9 + opacity 0.7 + italic muted —— 与 iter #201 PanelMemory hover hint 同 style 风格
- 列举具体操作（done / priority / snooze / pin / silent / 复制 / ...）让 hint 不只是"知道有 ctx menu"还能预期"里面有什么"

## 关键设计

- **样式镜像 PanelMemory hover hint (iter #201)**：同 dashed-top divider + 9px italic muted opacity 0.7。owner 在 panel 间切换感受一致的"hint 出现在 hover tooltip 底"模式。
- **不 gate 在"首次"**：每次 hover 都显，老 owner 一行小灰字噪音可忽略。
- **列举具体操作**：纯说"右键查看所有操作"对 ctx menu 内容陌生的用户帮助有限；列 done / priority / snooze / pin / silent / 复制 等让 hint 既诱导 click 也提供 mental model。
- **不写"双击行编辑 description"hint**：行本身没双击 handler（仅 onClick 折叠）；写错了的 hint 比没 hint 更坑。
- **hover preview tooltip 已有 gate（hasChips || history || detail 至少一个）**：空 task 行不显 hover preview → 不显 hint。targets 首次接触含内容的任务的用户场景。

## 不做

- **不加全屏 onboarding overlay 教学**：成本高 + 干扰老用户。hint 永显小字成本可控。
- **不绑 keyboard 快捷键打开 ctx menu**：右键已经是 standard interaction；用户用键盘 nav 一般会用 / / Tab / Enter 等更通用快捷键。
- **不在所有面板做 hint 散布**：只 PanelMemory item / PanelTasks 任务行两处 hover hint —— 这两类是 "list row with hidden 操作" 场景。ChatMini / PanelChat 等 message-style UI 不需要。
- **不写测试**：纯 UI 文本添加；视觉验证（hover 一个有 detail 的 task → tooltip 底显新 hint）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.18s
- 改动 ~20 行（hint footer div + 注释）。既有 hover preview pipeline / chips / recentHistory / detailSnippet / renderDetailTextWithLinkCards 路径完全不动。

## TODO 状态

剩 4 条留池：
- butler_task edit-schedule modal 扩支 every_weekdays
- detail.md 编辑器字数 chip 选区感知
- PanelChat session bar item hover 1s 浮 "最近 3 条" preview
- ChatMini bubble click + ⌘ 复制单条

## 后续

- 类似 hint 加到 PanelChat session bar item / ChatMini bubble / Live2D 区右键 etc.，建立一致的"hover 底 hint 显隐藏交互"模式。
- 配 Settings 加键盘 cheatsheet 一页 panel 集中展示所有 panel 的 hidden interaction，让 onboarding 不只靠 hover 发现。
- 任务行 hint 在 owner 用过右键 N 次后自动消失（与 chrome 的"快捷键 hint 用过就隐"模式）—— 但实现要写本地 count + threshold，scope 小可暂缓。
