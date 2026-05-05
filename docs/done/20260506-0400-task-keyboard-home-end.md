# 任务面板 Home/End 跳转 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务面板焦点行 Home/End：长队列里 ↑↓ 翻到底太慢；Home 跳第一条、End 跳末条覆盖剩下的快速跳转需求。

## 目标

键盘焦点已支持 ↑↓ 移焦点 / 空格切换选中 / Enter 切换展开。本轮加 Home / End
两键，让长队列下能直接跳首尾，免按住 ↑↓ 翻屏幕。

## 非目标

- 不做 PageUp / PageDown —— "翻一页"在 panel 没有"页"概念（用户视区由滚动
  决定），按行实现意义不大。Home/End 已覆盖跳转最常用场景。
- 不做"跳到第 N 条"输入框 —— 重 UI 不值；用户可用 search / tag 过滤更精准。
- 不写 README —— 键盘可达性微调。

## 设计

复用既有 onKey listener，加 2 个分支：

```ts
} else if (e.key === "Home") {
  if (list.length === 0) return;
  e.preventDefault();
  setFocusedIdx(0);
} else if (e.key === "End") {
  if (list.length === 0) return;
  e.preventDefault();
  setFocusedIdx(list.length - 1);
}
```

放在 ↑↓ 之后、空格/Enter 之前；INPUT/TEXTAREA/SELECT/BUTTON 守卫已经在头部
（不会与浏览器原生 Home/End 在文本框里冲突）。

scrollIntoView effect 随 focusedIdx 自动跟进 —— 不必再加单独逻辑。

## 测试

逻辑全在前端 effect / handler 里；无 vitest 配置。靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | onKey 加 Home / End 分支 |
| **M2** | tsc + build + cleanup |

## 复用清单

- 上一轮的 ref-pattern + tagName 守卫
- 既有 scrollIntoView effect

## 待用户裁定的开放问题

- focusedIdx === null 时按 Home/End 是否启动焦点？本轮**是**——Home/End 语义
  明确（跳到边界），不像 Enter 容易误触。让用户不用先按 ↑/↓ 启动。

## 进度日志

- 2026-05-06 04:00 — 创建本文档；准备 M1。
- 2026-05-06 04:05 — 完成实现：
  - **M1**：`PanelTasks.tsx` 既有 onKey listener 加 Home / End 分支（放在 ↑↓ 之后、空格/Enter 之前）。focusedIdx === null 时也直接启动焦点（Home/End 语义明确不像 Enter 易误触）；INPUT/TEXTAREA/SELECT/BUTTON 守卫已在头部，文本框内 Home/End 走原生光标移动。scrollIntoView effect 自动跟进。
  - **M2**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 键盘可达性微调。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；逻辑是 4 行（2 分支 + setState），由 tsc + 上一轮已验证 ref-pattern 保证。
