# PanelTasks 顶 ⌘N hint chip 浮出（实际 hotkey 早已 wire）

## 背景

上轮 auto-propose 的 "PanelTasks 顶 + 新建 按钮绑 ⌘N 全局快捷 + 按钮 label 加 hint" 是 stale TODO —— grep 确认 `useEffect` 在 line 1922-1949 已挂 `⌘N / Ctrl+N` 全局监听打开 quickAddOpen modal + 自动 focus title input。

但 owner 没有视觉提示能发现 ⌘N —— PanelTasks 顶 "新建任务" collapsible section header 只显 `▸ 新建任务` 没有 hotkey 提示，EmptyState 模板按钮 tooltip 也不提。

本 iter 只补可见 hint，让 ⌘N 可发现。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### 1. Section title 加 hotkey chip

```tsx
<div onClick={() => setCreateFormExpanded((v) => !v)} title="...⌘N 任意时刻弹快速建任务 modal">
  <span>{createFormExpanded ? "▾" : "▸"}</span>
  <span>新建任务</span>
  {!createFormExpanded && (
    <span style={{ fontSize: 10, fontFamily: monospace, background: "border", padding: "1px 5px", opacity: 0.7, ... }}>
      ⌘N
    </span>
  )}
</div>
```

- chip 仅在 collapsed 时显（展开时下面就是大 form，hotkey 提示冗余）
- 风格与 PanelTasks 内既有 ⌘F 搜索 hotkey chip 一致

#### 2. EmptyState 模板按钮 label + tooltip 加 hint

```tsx
<button title="...任意时刻 ⌘N 也可弹空白 modal">
  📋 用范例预填一条 (⌘N 弹空白)
</button>
```

## 关键设计

- **stale TODO discovery**：iter 之前没 grep 确认 ⌘N 已实现就把它写进 auto-propose 列表。教训：auto-propose 阶段也要 grep。本 iter 仍 close 此 entry，把"修复 hint 可见性" 作为实际成果记录。
- **chip 仅 collapsed 显**：展开时 form 已占大量 vertical space，header 不再需要 hotkey hint；collapsed 时 header 几乎是仅有的"建任务入口" UI 元素，chip 引人眼一眼可见。
- **风格一致**：与 PanelTasks 内既有 ⌘F 搜索 chip / ⌘[ ⌘] 导航 chip 等 hotkey hint 同 monospace + border bg + 0.7 opacity。
- **tooltip 写出 mac / Windows / Linux 各自键位**："⌘N（macOS）/ Ctrl+N（Windows/Linux）"明确告诉跨平台 owner 不同键。
- **EmptyState 按钮也补 hint**：owner 队列空时第一次看 panel 就该知道有 ⌘N 这条路径，免于"必须先点空状态按钮才能建"的错觉。

## 不做

- **不引新 hotkey**：⌘N 已稳态使用，不改 binding。
- **不加 toast "已开建任务 modal"**：modal 自身视觉出现就是反馈。
- **不在每个 panel 都散布 ⌘N hint**：仅 PanelTasks 本面板 owner 期望"在这里"建任务时寻找 hint —— 主入口集中显示，避免 UI 噪音。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.17s
- 改动 ~30 行（section title chip 25 + EmptyState 按钮 hint 2 + 注释 3）。⌘N keydown handler / quickAddOpen modal / handleCreate 既有路径完全不动。

## TODO 状态

剩 3 条留池：
- ChatMini bubble 底 "⏱ N 分前" hover chip
- PanelMemory item 行右键「📅 显创建时间」
- butler_task `[silent]` marker

## 后续

- ⌘⇧N 让 collapsed 表单展开 + focus（与 ⌘N 弹 modal 形成 "inline form" vs "modal" 双键路径）。
- 在 PanelChat / PanelMemory 等其它面板也加 ⌘N hint chip（每面板对应自己最常用的"建"操作 —— PanelChat → 新会话；PanelMemory → 新 memory）。
- Settings 加键盘 cheatsheet 一页统一陈列所有快捷键，让 owner 一次性看清。
