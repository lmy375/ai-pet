# PanelMemory item 右键 ctx menu（iter #439）

## Background

PanelMemory item 行 chip 链已发展到 5-7 个（📋 复制 detail / 📑
副本 / 🔗 inline ref / ↗ 跳任务面板 / 📜 历史快照 / 🚀 立即跑 /
🗑 删 / ⏰ alarm…）— mouse 党 hover 才显的 chip 一行密度高，新
owner 也难找入口。

本 iter 加右键 ctx menu — 聚合既有 chip 动作（✏️ 改名 / 📑 副本 /
🔗 inline ref / ↗ 跳任务面板 / 🗑 删）让 mouse 党快速操作；与
always-visible inline chip 互补不替代（hover-党 / 触屏党仍走 chip
路径）。

## Changes

### `src/components/panel/PanelMemory.tsx`

#### 1. State + Esc/outside-click 关

```ts
const [memItemCtxMenu, setMemItemCtxMenu] = useState<
  | { catKey, title, detailPath, description, x, y }
  | null
>(null);

useEffect(() => {
  if (!memItemCtxMenu) return;
  const onDocClick = () => setMemItemCtxMenu(null);
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") setMemItemCtxMenu(null);
  };
  window.addEventListener("mousedown", onDocClick);
  window.addEventListener("keydown", onKey);
  return () => { ... };
}, [memItemCtxMenu]);
```

与 PanelTasks taskCtxMenu / ChatMini ctxMenu 同 outside-mousedown +
Esc 关 pattern。

#### 2. item row 加 onContextMenu

```tsx
<div ... onContextMenu={(e) => {
  e.preventDefault();
  e.stopPropagation();
  endPreviewHover();   // 防 hover preview tooltip 卡在屏幕
  setMemItemCtxMenu({ catKey, title, detailPath, description, x, y });
}}>
```

preventDefault 阻浏览器默认 ctx menu（Tauri 已禁但保险）；
stopPropagation 防外层 row click 冒泡；endPreviewHover 防 hover
preview tooltip 与 ctx menu 同屏冲突。

#### 3. Popover JSX（component 内 return 末插）

5 个动作 + 1 个 separator：
- ✏️ 改名 → `setRenamingMemoryKey(itemKey); setRenameMemoryDraft(title);`
- 📑 复制副本 → 内联现有 inline chip 的副本生成逻辑（read
  detail_full + auto -copy-N + memory_edit create）
- 🔗 复制 inline ref → clipboard write `[[catKey/title]]`
- ↗ 跳到任务面板 → `onRequestFocusTask(title)`（仅 catKey ===
  "butler_tasks" + prop 传入时显）
- (separator)
- 🗑 删除 → `handleDelete(catKey, title)`（armed/confirm 2 步与
  既有 chip 同模式）— armed 状态下 label 变红 `⚠ 再点确认删除`，
  实际删除后 menu 才自动关

每按钮 hover 微染 var(--pet-color-bg) 让 owner 看清当前
keyboard/mouse 位置（与 PanelTasks itemBtnHoverIn / Out 同 pattern）。

popover viewport 夹紧（W=200 / H=200 clamp from edge 8px）防贴边
切掉。

## Key design decisions

- **聚合动作而非新增**：每个 action 都已有 always-visible chip 入
  口；本 menu 是「mouse 党快路径」复合入口，与 chip 等价路径
- **不复用 PanelTasks taskCtxMenu**：那个是 task 专用（priority /
  due / NOW / silent / blockedBy 等），与 memory item 操作矩阵
  不重叠。两菜单独立各自演化
- **armed delete 不立刻关 menu**：第一次点击仅 arm（红警告），
  owner 需在 menu 内再点确认。这避免「点了 🗑 但没确认 → menu
  关了 → 不知道 armed 状态」混淆
- **inline 副本逻辑而非提取 helper**：与既有 inline 副本 chip
  共 ~30 行逻辑；抽 helper 要 5 参数（itemKey / detailPath /
  description / catKey / index）不划算。如未来加第三处再重构
- **不引 keyboard nav (上下箭头选项)**：右键菜单是 mouse-focused
  入口；键盘党有 chip + shortcut 路径足够
- **不为单 popover 引 unit test**：所有 action 都委托既有 handler
  （已有边界测试覆盖）；build pass + 手测足够（右键 item 看 menu
  弹起 → 点 ✏️ 看 inline rename → Esc → 右键 → 点 🗑 看红警告
  → 再点 → 看删除 + menu 关 → ↗ 仅 butler_tasks 显，其它 cat
  不显）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.83s)
- 后端无改动 — 复用既有 handleDelete / memory_edit 通道
