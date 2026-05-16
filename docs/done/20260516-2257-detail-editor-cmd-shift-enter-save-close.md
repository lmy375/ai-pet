# detail.md 编辑器 textarea ⌘⇧Enter 保存并关闭

## 背景

iter #215 给新建任务表单加了 ⌘⇧Enter "建并打开 detail 编辑器" 的"⌘⇧Enter = 一键完成本轮工作 + 进入下个"心智。

owner 进编辑器写完后想"保存并关闭"对偶动作 —— 既有 ⌘S 仅保存但已经会顺手 setEditingDetailTitle(null) 关闭（handleSaveDetail 内部行为）。所以 ⌘⇧Enter 在 editor 内功能等同 ⌘S。

但保留 ⌘⇧Enter 是为了 **owner 心智一致** —— "建任务用 ⌘⇧Enter 进编辑，编辑完用 ⌘⇧Enter 完成"。muscle memory 一致而不必区分两个不同的"完成" 快捷键。

## 改动

### `src/components/panel/PanelTasks.tsx`

两个 textarea (edit / split 模式) 各加 ⌘⇧Enter 分支于既有 ⌘S 处理之后：

```ts
if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key === "Enter") {
  e.preventDefault();
  if (savingDetail) return;
  handleSaveDetail(t.title);  // 内部 setEditingDetailTitle(null) 关编辑器
  return;
}
```

两个 placeholder 同步更新：
```
（⌘S 保存 / ⌘⇧Enter 保存并关闭 / Esc 取消）
```

## 关键设计

- **复用 handleSaveDetail**：handleSaveDetail 内部就关闭编辑器（line 2775 `setEditingDetailTitle(null)`），所以 ⌘⇧Enter = ⌘S 实际行为一致。本 iter 仅加 alias 让 owner 心智一致而非新行为。
- **`e.shiftKey && e.key === "Enter"` 优先在 ⌘S 之前**：实际放置顺序无所谓 —— ⌘S 仅匹配 `key === "s"`，⌘⇧Enter 匹配 `key === "Enter"`，互不重叠。
- **无 ⇧ 时不抢**：textarea 原生 ⇧Enter 是换行。仅 ⌘⇧Enter 三键组合才触发。
- **两 textarea 同步**：edit 模式 + split 模式 (preview / edit 并排) 各有独立 textarea，两处都加。replace_all 因 old_string 含其它独有注释只匹配 1 处 → 手动 edit 第 2 处。
- **placeholder 文案补 hint**：让 hover 文档发现新快捷键。

## 不做

- **不在新建任务表单加 ⌘S**：表单内 ⌘Enter / ⌘⇧Enter 已够；⌘S 在表单内无 save 语义（任务还没建）。
- **不写 toast "✓ 已保存关闭"**：handleSaveDetail 关编辑器是视觉反馈 —— 编辑器消失 → owner 知道完成。多余 toast 噪音。
- **不写测试**：纯 keydown handler；与既有 ⌘S 同 pipeline。视觉验证（开 detail 编辑器 → 改内容 → 按 ⌘⇧Enter → 应保存并关）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.21s
- 改动 ~30 行（两 textarea handler 各加 12 + 两 placeholder 同步 + 注释）。既有 ⌘S / Esc / handleSaveDetail / handleCancelEditDetail / autosave / dirty 检测 pipeline 完全不动。

## TODO 状态

剩 4 条留池：
- butler_task 行 [reminderMin: N] chip click 弹快速编辑
- PanelMemory ai_insights banner 加 daily_review 计数链接
- TG /markers 命令一次列 pinned + silenced
- pet 区 hover 显本机时区 chip 浮卡

## 后续

- 同款 ⌘⇧Enter 给 ChatPanel 顶 "新会话" / "重命名" 等 inline edit 输入框 —— "完成本轮编辑"通用 gesture。
- ⌘⌥Enter "保存但不关" 让 owner 想继续写但先 commit 一段。
