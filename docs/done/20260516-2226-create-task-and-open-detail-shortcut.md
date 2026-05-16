# PanelTasks 新建任务 ⌘⇧Enter 创建并立即打开 detail.md 编辑器

## 背景

iter R120 加了 ⌘Enter / Ctrl+Enter 在创建表单内提交任务。但 owner 创建任务后常希望立刻进 detail.md 编辑器写"我打算这样做 / 待办子步骤 / 进度笔记"。当前路径：
1. ⌘Enter 创建
2. 找到新建行（队列底）
3. 点击展开
4. 点击「✎ 编辑详情」
5. 终于进编辑器

5 步。加 ⌘⇧Enter 一键到位。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### 1. `handleCreate` 加 `openDetailAfter: boolean = false` 参数

```ts
const handleCreate = async (openDetailAfter: boolean = false) => {
  ...
  await invoke<string>("task_create", { args: {...} });
  ...
  await reload();
  setQuickAddOpen(false);
  // ⌘⇧Enter 路径
  if (openDetailAfter) {
    handleEnterEditDetail(titleTrimmed, "");  // 新 task detail.md 初始 ""
    setPendingTitleFocus(titleTrimmed);       // 滚到新行 + scrollIntoView
  }
};
```

#### 2. `handleFormKeyDown` 用 e.shiftKey 区分

```ts
if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
  e.preventDefault();
  if (creating) return;
  void handleCreate(e.shiftKey);  // ⇧ 同按 → openDetail
}
```

#### 3. 两个"创建任务" button 的 onClick 改 `() => void handleCreate(false)` + tooltip 加 hint

inline form 内 + quickAdd modal 内两处按钮 tooltip 加：
```
⌘Enter / Ctrl+Enter 仅创建。⌘⇧Enter 创建并打开 detail 编辑器。
```

## 关键设计

- **复用 handleCreate**：单一函数 + 可选参数 `openDetailAfter` 避免代码重复。两条路径共享创建 IPC / state reset / queue reload 行为。
- **e.shiftKey 区分**：⌘Enter 与 ⌘⇧Enter 同一 handler，shiftKey 标志位决定后续 path。owner 心智 = "modifier 升级 = action 升级"。
- **handleEnterEditDetail(title, "")**：新 task detail.md 后端未写过 → 初始空字符串。既有 handleEnterEditDetail 路径接受 currentMd 参数；后续 owner 写 + ⌘S 时 backend 才落 detail.md 文件。
- **setPendingTitleFocus**：复用既有 jump-to-task pipeline —— 滚 visibleTasks 找 idx + scrollIntoView。owner 切到新行时视觉上立刻看到目标位置。
- **不修改 ⌘Enter 行为**：既有快捷键保持不变（owner muscle memory 保护）；新增是 ⌘⇧Enter，对原行为非破坏性增量。
- **tooltip 列两个 modifier**：让 owner hover 创建按钮就发现新快捷。

## 不做

- **不绑 ⇧Enter（无 ⌘）**：⇧Enter 在 textarea 内是 native 换行，劫持会破坏 owner 写 body 时的自然多行输入。
- **不引入 toast "✓ 已创建 + 进入编辑"**：handleEnterEditDetail 立即切到编辑器是视觉反馈，多余 toast 噪音。
- **不写测试**：纯键盘 handler + 既有 handleCreate / handleEnterEditDetail 链路（已验证）；视觉验证（填表 + ⌘⇧Enter → 行新建 + 直接进编辑器）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.18s
- 改动 ~30 行（handleCreate 加参数 + openDetail branch 10 + handleFormKeyDown 改 shiftKey 1 + 两按钮 onClick 改 + tooltip 6 + 注释）。既有 task_create IPC / reload / setPendingTitleFocus / handleEnterEditDetail 路径完全不动。

## TODO 状态

剩 1 条留池：
- 桌面 pet 右键加「⏰ 设倒计时 N 分钟 nudge」

## 后续

- ⌘⇧Enter 后自动 focus detail 编辑器 textarea —— 当前 setPendingTitleFocus 仅滚行 + outline，textarea focus 仍需 owner 点击。
- "建+打开 detail" 的 toolbar 按钮（视觉对偶按钮）让鼠标 owner 也能一键 access。
- ⌘⌥Enter 创建并立即标 NOW（与 ⚡ NOW chip 同语义），让 keyboard owner 多种 action 都覆盖。
