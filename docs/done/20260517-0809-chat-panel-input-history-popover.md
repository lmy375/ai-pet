# ChatPanel 输入框「💡 最近输入」浮按钮（iter #268）

## Background

ChatPanel 已有 shell-style ↑↓ 历史召回（按 ↑ 翻到上条 / ↓ 翻到下条），但
仅鼠标 user 不便（要先聚焦 textarea 再按键盘）。复杂调试 / 长 prompt 重发
等场景，owner 想直接"扫读最近输入 + 点选"。

本迭代加 textarea 右上角的「💡」浮按钮：click 弹 popover 列最近 5 条
sentHistory，row click 把内容灌进 textarea。键盘 ↑↓ 路径完全保留，二者共
存（点选 row 会同步 historyCursorRef + recalledValueRef，让接下来按 ↑ 仍
正确接着翻）。

## Changes

仅 `src/components/ChatPanel.tsx`：

- 新增 `historyPopoverOpen: boolean` state + useEffect 挂 `mousedown`
  outside-click / Esc 关闭监听
- 把 `<textarea>` 包进 `<div style="position: relative">`，加两个浮层兄弟：
  - 💡 按钮：絕對定位 top: 6 / right: 8 / 22×22 圆形，仅
    `sentHistory.length > 0` 时渲染（避免新用户看到空状态按钮）
  - popover：bottom: calc(100% + 6px) 浮在 textarea 上方，列 `sentHistory
    .slice(0, 5)`，每条 row click → `setInput(entry) + historyCursorRef
    .current = i + recalledValueRef.current = entry + setHistoryPopoverOpen
    (false) + 聚焦 textarea + 光标落末尾`
- textarea `paddingRight: sentHistory.length > 0 ? 34 : 14`，让按钮不遮挡
  最后字符
- 每条 row 用 `replace(/\s+/g, " ")` flatten 多行 + 80 字符截断 + 末尾 `…`
  避免长 prompt 撑爆 popover；hover row 显完整 title tooltip

## Key design decisions

- **键盘 ↑↓ 路径完全保留**：popover 是鼠标入口"加成"而非替代。点选 row 时
  同步 historyCursorRef + recalledValueRef，让接下来按 ↑ 接着翻（与既有
  shell-history 模式无缝衔接）。
- **仅最近 5 条**：sentHistory 最多保留 30+ 条历史，popover 全列会很长 +
  浮动卡片可能溢出视口。5 条够覆盖"重复发刚发过的 prompt + 上一条 + 几条
  早一点的"典型场景；想看更早走 ↑ 键盘翻。
- **`position: relative` 包 textarea 而非整个 parent**：parent 已有
  `position: relative`（dragActive overlay 需要）；加在 textarea 自身 wrapper
  让 💡 + popover 锚到 textarea 边而非更外层，定位更准。
- **按钮 22×22 圆形**：与 ChatMini 顶部 ⛶ / 📋 / 🌐 时区 chip 同视觉风格，
  系统按钮密度感统一。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)
