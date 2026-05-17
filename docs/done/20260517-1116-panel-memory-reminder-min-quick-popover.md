# PanelMemory butler_tasks reminderMin chip 改为 popover 快速编辑（iter #285）

## Background

butler_tasks 行内 reminderMin chip click 当前直接打开一个完整 modal —
解释段 + 草稿确认 + preset 按钮 + custom input + 清除。多数 owner 操作是
"在 5/15/30 三档之间快切"，full modal 太重。

本迭代改为 mini popover：click chip → 5/15/30 preset 一键写盘；"自定义…"
fallback 到既有 modal（保留全功能）；"🗑 移除"清除 marker。

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- **state**：
  - `reminderQuickPickerTitle: string | null` — 哪个 task 的 popover 打开
  - `reminderQuickBusy: boolean` — invoke 期间禁用 popover 按钮防双触
  - outside-click + Esc 关 useEffect

- **`quickSetReminderMin(title, description, newN | null)` 异步 helper**：
  - regex strip 旧 `[reminderMin: N]` 段 + collapse whitespace
  - `newN === null` → 仅 strip（移除）；否则 append `[reminderMin: newN]`
  - `invoke("memory_edit", action: "update", ...)` 写回
  - loadIndex + 3.5s 反馈 toast

- **chip 改造**：原 onClick 打开 modal，改为 toggle popover：
  - 包 chip 为 `<span style="position: relative">` 让 popover 绝对定位贴底
  - popover 内含：5/15/30 preset 按钮（active 状态 = 当前值 highlight）+
    分隔线 + "✏️ 自定义…" 跳 modal + "🗑 移除" 清除
  - hover 灰底 / busy 期 0.5 opacity + cursor default

## Key design decisions

- **popover 不替 modal，而是 fast path**：常见操作（5/15/30 切换）走 popover
  一步搞定；模板 / 解释段 / 任意 N（如 7 分）走 modal 完整路径。两者并存，
  popover 内"✏️ 自定义…"作为升级入口。
- **复用既有 memory_edit invoke 路径**：popover 助手内部直接调
  `memory_edit("update")` —— 与 modal commit 路径同后端，状态一致。
- **active preset highlight**：当前值是 5 时 popover 内"🔔 -5 分 ·当前"
  绿底显，让 owner 一眼看到从哪儿切到哪儿；click 同值是 no-op（但仍走
  invoke 路径，不影响 idempotent）。
- **outside-click + Esc 关闭**：与既有 snoozePicker / priorityPicker /
  dueShiftPicker 等同 popover 模板对齐 — 减少新模式学习。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.19s)
