# 任务详情进度笔记快捷保存（⌘S） — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务详情进度笔记快捷保存（⌘S）：编辑 detail.md 时点保存按钮要鼠标，⌘S 是文档编辑直觉；textarea 内监听 ⌘S/Ctrl+S → 触发保存逻辑。

## 目标

PanelTasks 编辑 detail.md 时，保存只能点按钮。⌘S/Ctrl+S 是几乎所有桌面
文档编辑器（VS Code / Notion / TextEdit / Notes）的肌肉记忆；用户一边
打字一边按 ⌘S 是普遍习惯。本轮在 textarea 内监听 ⌘S → 触发既有
`handleSaveDetail`，与按按钮等价。

## 非目标

- 不全局拦截 ⌘S —— 只在编辑 textarea 聚焦时响应；其它地方按 ⌘S 保留
  浏览器 / 系统默认（Tauri webview 上 ⌘S 默认无 UI，但仍不强占）。
- 不做"自动保存" —— 用户对显式保存有期待（点保存 → 写盘的预期清晰）；
  自动保存 + 撤销窗口是更大的产品决策，本轮不做。
- 不做 ⌘Enter 替代 ⌘S —— ⌘S 已是文档编辑标准；多键位反而散乱。

## 设计

### 监听位置

直接在 textarea 上挂 `onKeyDown`，**不**走全局 keydown listener：
- 只在编辑模式（`editingDetailTitle === t.title`）下渲染该 textarea，
  所以监听天然 scoped。
- 全局 listener 已被 ⌘F 占用，多塞快捷键容易冲突。

### 行为

```ts
onKeyDown={(e) => {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
    e.preventDefault();   // 吃掉 webview 默认 (页面保存)
    if (savingDetail) return;  // 复点保护
    handleSaveDetail(t.title);
  }
}}
```

`savingDetail` 守卫：保存进行中再按 ⌘S 不重发请求（与按钮 `disabled`
语义一致）。

### 视觉提示

placeholder 文案补 "（⌘S 保存）" 让用户知道功能存在。同 PanelChat /
PanelTasks 搜索框补 hint 的模式。

### 保存按钮 title

按钮 `title` 加 "（⌘S 等价）" 让用户从按钮 tooltip 也能发现快捷键。

## 测试

textarea inline handler；前端无 vitest，靠 tsc + 手测。`handleSaveDetail`
本身已有保存路径单测覆盖（task_save_detail backend），不重复。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | textarea onKeyDown ⌘S 分支 + placeholder hint + 按钮 title |
| **M2** | tsc + build + cleanup |

## 复用清单

- 既有 `handleSaveDetail` 函数
- 既有 `savingDetail` 守卫语义
- 既有 placeholder hint 模式（同搜索框 ⌘F）

## 进度日志

- 2026-05-07 00:00 — 创建本文档；准备 M1。
- 2026-05-07 00:05 — M1 完成。textarea inline `onKeyDown` 拦 ⌘S/Ctrl+S → preventDefault + savingDetail 守卫 + handleSaveDetail；placeholder 文案补"（⌘S 保存）"；保存按钮 title 加"⌘S 等价"。
- 2026-05-07 00:10 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 938ms)。归档至 done。
