# PanelMemory item 「📅 created chip click 复制 ISO」（iter #453）

## Background

PanelTasks 行已有「ts chip click 复制 ISO」入口 — owner 想引用某 task
具体创建 / 更新时刻时直接 click chip 复制 ISO 时间戳。但 PanelMemory
item 行的 `📅 创建 X 前 · 🔄 更新 Y 前` 文字 chip 当前仅 display，没
click handler — 与 PanelTasks ts chip 不对偶。

owner 在 detail.md / chat / TG 引用 memory item 想配「这条 memory 在
2026-05-12T08:30 写的」具体时刻时只能从 hover tooltip 选中复制（多步），
或走 SQL / yaml dump。

本 iter 让 `📅 创建` / `🔄 更新` 两 chip 各自成 click 入口 → 复制对
应字段 ISO。

## Changes

### `src/components/panel/PanelMemory.tsx`

将既有 `<div>{parts.join(" · ")}</div>` 平文本改为两个独立可点击
`<button>` chip：

```tsx
{!Number.isNaN(createdMs) && (
  <button
    style={chipStyle}
    onClick={(e) => { e.stopPropagation(); void copyIso(item.created_at, "created_at"); }}
    title={`复制 created_at ISO：${item.created_at}`}
  >
    📅 创建 {fmt(createdMs)}
  </button>
)}
{!Number.isNaN(createdMs) && showUpdated && " · "}
{showUpdated && (
  <button
    style={chipStyle}
    onClick={(e) => { e.stopPropagation(); void copyIso(item.updated_at, "updated_at"); }}
    title={`复制 updated_at ISO：${item.updated_at}`}
  >
    🔄 更新 {fmt(updatedMs)}
  </button>
)}
```

`copyIso(iso, field)` 内：

```ts
await navigator.clipboard.writeText(iso);
setMessage(`📋 已复制 ${field} ISO: ${iso}`);
setTimeout(() => setMessage(""), 2500);
```

`chipStyle` 透明 button — visually 与原文本无差异，仅 cursor: pointer
+ click 行为 hint owner。`stopPropagation` 防 click 冒到外层（item row
双击触发 description 编辑）。

## Key design decisions

- **复用 setMessage 通道**：与既有 PanelMemory 复制族（📋 disk usage /
  📂 logs dir / detail.md path / inline ref 等）同模板 — 顶部提示条 2.5s
  自动消失，不引入新 ✓ 状态机
- **chipStyle 透明 button**：保视觉与原 plain text 一致（同字号 /
  monospace / muted color），仅 cursor: pointer 提示可点击。比 `<span>`
  + role="button" 更标准（无障碍 / 键盘 tab 顺序天然支持）
- **stopPropagation**：item row 上层有 onDoubleClick 进 description 编
  辑、onContextMenu 弹 ctx menu — 阻 click 冒泡防 chip 误触外层
- **两 chip 各自独立 button（而非一段公共 click 区）**：owner 心智「点
  📅 复制 created / 点 🔄 复制 updated」 — 各 chip 单独动作；公共 click
  + 弹 popover 选 field 不必要的多一步
- **不写 unit test**：纯 click + clipboard 副作用；逻辑 trivial（既有
  `parts` 计算路径不变，仅 wrap button）。GOAL.md "meaningful tests
  only" 规则下不引装饰性测试。`tsc + vite build` clean 即够
- **`setMessage` 显具体 ISO 而非"已复制"通用文案**：让 owner 看到 actual
  string（如 `已复制 created_at ISO: 2026-05-12T08:30:00+08:00`）— 验
  证「我刚复制的是这个时刻吗」无需再粘出来看

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.27s)
- 后端无改动 — 纯前端 UI 增强
- 手测：PanelMemory 任一 item → click 「📅 创建 X 前」chip → 顶部消息条
  显「📋 已复制 created_at ISO: …」→ click 「🔄 更新 Y 前」chip 同
  效；item row 双击 description 编辑 / 右键 ctx menu 不受 chip click
  误触
