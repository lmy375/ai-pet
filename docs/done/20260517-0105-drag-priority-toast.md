# PanelTasks 拖行改 priority 后浮 1.5s toast 反馈

## 背景

PanelTasks 行可拖到另一行，自动把 source 行 priority 改成 target 行 priority。但操作完没数字反馈 —— owner 拖完一脸懵 "我刚拖到 P 几了？"。容易误操作不知道。

加 1.5s toast "🎯 拖动「X」P3 → P5（↑ 升）"显式反馈。

## 改动

### `src/components/panel/PanelTasks.tsx`

`handleDragDropPriority` 在 reload 后追加 toast 触发：

```ts
const oldPri = source.priority;
const newPri = target.priority;
try {
  await invoke<void>("task_set_priority", { title: sourceTitle, priority: newPri });
  await reload();
  const arrow = newPri > oldPri ? "↑ 升" : "↓ 降";
  setBulkResultMsg(`🎯 拖动「${sourceTitle}」P${oldPri} → P${newPri}（${arrow}）`);
  window.setTimeout(() => setBulkResultMsg(""), 1500);
} catch (e) {
  setActionErr(`拖拽改 priority 失败：${e}`);
}
```

## 关键设计

- **复用 setBulkResultMsg toast slot**：与既有 inline-edit P pill click 改 priority / ✦ +1/-1 / 复制 markdown 等 priority-related 操作同 toast 区，UX 一致。
- **arrow ↑ 升 / ↓ 降**：让 owner 一眼分辨方向 —— 拖到下一行（更"重要"位置）可能升也可能降，文字明示比箭头位置更直觉。
- **含 title + 旧值 → 新值**：让 owner 立刻看到完整操作信息，可立刻撤销若误操作（虽然没绑 undo —— 但 owner 至少看到改了什么）。
- **1.5s 短显**：与其它 toast 同 timeout；拖完即知不挡新操作。
- **失败仍走 setActionErr**：分错误反馈渠道；成功路径才 setBulkResultMsg。

## 不做

- **不绑 undo**：当前面板无 undo 栈基础；owner 想撤销可手动改回。
- **不绑 ⌘Z toggle**：textarea / form 已有 native undo；自定义 undo 会冲突。
- **不实时跟随拖**：drag-over 时实时 preview 改 priority 视觉太重；drop 后 toast 已足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.21s
- 改动 ~15 行（capture oldPri / newPri + toast 文案 + 注释）。既有 task_set_priority IPC / reload / drag drop 路径完全不动。

## TODO 状态

剩 3 条留池：
- PanelSettings 顶 search input
- PanelMemory "今天新增" chip drill-down
- PanelChat session 右键菜单加「📌 钉住会话」

## 后续

- 同款 toast 给 inline P pill click 改 priority（数字按钮也"看到改了什么"反馈一致）。
- toast 加 "撤销" inline button 5s 内可一键 revert —— 这步要 undo state 栈，后续 iter 加。
- 拖到自身 / 同 priority 时也提示 "拖动忽略（priority 相同）"让 owner 知道操作无效而非"是不是没拖到"。
