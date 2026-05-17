# detail.md 编辑器 ⌘⌥Enter 保存并跳下一条 task（iter #336）

## Background

detail.md textarea 已有 ⌘⇧Enter 保存并关闭（完成本轮编辑）+ ⌘[ / ⌘]
切换上下条（不保存）。owner 在"连续 review / 改 detail 笔记"工作流时
当前需要：⌘S 保存 → ⌘] 切下条；两步键序在多 task 时累积成 friction。

本迭代加 ⌘⌥Enter 一键完成"保存 + 跳下一条"，与既有 ⌘⇧Enter "完成本
轮"对偶 — "⌘⌥ = 继续下一条" 心智。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- 新 callback `handleSaveAndNavigateNext(curTitle: string)`：
  - 在 save 之前先 `visibleTasks.findIndex` 算 nextTask（save 内会
    setEditingDetailTitle(null) 关闭编辑器，nav 逻辑会失去 anchor）
  - 末条 task → 仅保存（退化为 ⌘⇧Enter 等价行为）
  - 非末条：await handleSaveDetail → fetch next detail（cache 命中直接
    用 / miss 走 task_get_detail / 失败兜底空内容）→ handleEnterEditDetail
    (next.title, targetMd) → setPendingTitleFocus 让目标行 scroll into
    view
- 两 textarea onKeyDown 块（edit + split mode）都加 ⌘⌥Enter 检查：
  - `(metaKey || ctrlKey) && altKey && !shiftKey && key === "Enter"`
  - preventDefault + savingDetail 守卫（防双触）+ void handleSaveAnd
    NavigateNext
- 既有 ⌘⇧Enter 分支扩 `!e.altKey` 守卫避免冲突（⌘⇧⌥Enter 不该同时
  触发两条路径）
- placeholder 文案补「⌘⌥Enter 保存并跳下一条」
- ⌘/ cheatsheet modal detail-editor 段加新条 ⌘⌥Enter → "保存并跳下一条
  task（连续 review 流）"

## Key design decisions

- **末条退化为 ⌘⇧Enter 行为**：避免末条按 ⌘⌥Enter 时无 nextTask 而陷
  入"按了没反应"困惑。退化为"保存并关闭"是自然终态 — owner 看到编辑
  器关闭即知道"我已经看到底了"。
- **save 前算 nextTask**：handleSaveDetail 内 setEditingDetailTitle(null)
  会关闭编辑器；callback 闭包内的 navigate 读 visibleTasks + 当前 idx
  必须在那之前完成。否则 navigate 会找不到当前任务。
- **复用 handleSaveDetail 完整路径**：detailMap patch / draft 清 /
  history refresh / refetch fresh detail 等所有副作用都不重复 — 一致
  with ⌘S / ⌘⇧Enter / 「保存」按钮路径。
- **⌘⇧Enter 加 `!e.altKey` 守卫**：modifier cluster 严格不重叠 —
  ⌘⇧Enter 是"保存并关闭"，⌘⌥Enter 是"保存并跳下一条"，⌘⇧⌥Enter
  当前没语义但守卫防御未来歧义。
- **末条 nextTask 算法走 visibleTasks**：filter 后的视图序列，与既有
  handleNavigateDetail("next") 同源 — owner 改了过滤后 ⌘⌥Enter 序列
  与 ⌘] 一致。
- **不引入 unit test**：与既有 ⌘D / ⌘L / ⌘⇧K detail editor 快捷键同
  型；键盘事件 + textarea selection 在 jsdom 难稳；通过 vite build +
  真实交互验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.23s)
