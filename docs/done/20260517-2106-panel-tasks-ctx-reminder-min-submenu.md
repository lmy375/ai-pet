# PanelTasks 行右键加「⏰ reminderMin」子菜单（iter #330）

## Background

PanelMemory item 已有 reminderMin chip click popover（5/15/30 preset 快速
设到点前 N 分软提醒，iter #222）。但 PanelTasks 端缺这条快速入口 —
owner 在 PanelTasks 看 task 时要设 reminder 只能：
- 跳到 PanelMemory 找同 task → click reminderMin chip
- 或走完整编辑改 raw_description 加 `[reminderMin: N]` marker

本迭代在 PanelTasks 行右键 ctx menu 加「⏰ reminderMin」子菜单（与既有
priority 子菜单同 pattern），让 PanelTasks 端也能快速设 / 移除 reminderMin。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- `taskCtxMenu` state 加 `reminderSubmenu: boolean` 字段（与既有
  `prioritySubmenu` 同 pattern）+ 开 ctx menu 时初始化为 false
- viewport-clamp 计算 H 时加 `+ (m.reminderSubmenu ? 60 : 0)` 让子菜单
  展开时纵向预留空间
- 新 callback `handleSetReminderMin(title, newMin | null)`：
  - 找 task → strip 旧 `[reminderMin: N]` → 追加新（或 newMin === null
    时仅 strip）→ invoke `memory_edit("update")` → reload + 3s toast 反
    馈
  - 算法与 PanelMemory `quickSetReminderMin` 同源（去重 + 清多余空白 +
    trim）
- ctx menu 在 priority 子菜单之后插「▸/▾ ⏰ reminderMin（当前 N 分 / 未
  设）」主项：
  - 主项 click → toggle `reminderSubmenu` 展开 / 收起
  - 展开时 5-列 grid 显「5 分 / 15 分 / 30 分 / 60 分 / 移除」预设
  - 当前命中 preset → 高亮 + disabled cursor（与 priority 子菜单 P{n}
    active 视觉一致）
  - click preset → setTaskCtxMenu(null) + 调 handleSetReminderMin

## Key design decisions

- **子菜单 而非独立 popover**：复用 priority 子菜单同模式让 owner 心智
  连贯（"右键 → 子项 → 选预设" 同 affordance）。独立 popover 需要额外
  state + 定位逻辑 + outside-click 处理，复杂度收益不匹配。
- **算法 strip-then-append 复用 PanelMemory**：相同 regex
  `/\[reminderMin:\s*\d+\s*\]/g`、相同清白空 + trim 模式 — 让两端 UI
  改 marker 写出来的 description 形态一致，避免漂移。
- **5/15/30/60/移除 五预设**：5 / 15 / 30 与 PanelMemory 同；加 60 分钟
  覆盖"一小时前提醒"常见场景；"移除"作为 explicit 选项让 owner 一键清
  marker 不必走"编辑 → 删 marker → 保存"。
- **handleSetReminderMin 走 memory_edit("update") 而非 task 专用命令**：
  reminderMin 是 description 字段的小修改 — 与既有 PanelMemory 同路径
  让 SQLite mirror / butler_history hook 自动跟进。不引新 backend 命令。
- **bulkResultMsg 3s toast 反馈**：与既有 priority change / clone /
  copy title 等行内 action 同反馈 channel；让 owner 看到具体值变更。
- **不引入 unit test**：纯 JSX submenu + handler 复用既有 memory_edit
  invoke 路径；与 priority 子菜单 / clone 等同型行为也未单测。通过 tsc
  + vite build 验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.25s)
