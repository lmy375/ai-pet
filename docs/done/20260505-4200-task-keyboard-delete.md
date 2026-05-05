# 任务行键盘 Delete/Backspace 取消快捷键 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务行键盘删除快捷键：focusedIdx 非 null 时按 Delete / Backspace 弹出"取消任务？"内联确认（与现有取消 reason input 复用），减少鼠标依赖。

## 目标

键盘选中焦点行后，按 Delete / Backspace 触发既有"取消 reason 输入"内联弹层
（等价于点了行内「取消」按钮）。`autoFocus` 让焦点立刻跳到 reason 输入框，
键盘党可以连续完成"翻 → 触发取消 → 输原因 → 确认"全流程。

## 非目标

- 不直接立即 cancel（终止性动作不该靠单键完成）—— 触发 reason 输入的二步
  确认是必经路径，与鼠标点击一致。
- 不为终态任务（done / cancelled）触发（取消已结束的任务无意义，cancel inner
  也会拒绝）—— focusedIdx 落到终态行时按 Delete 不响应。
- 不写 README —— 任务面板键盘可达性补强。

## 设计

复用上一轮 keyboard nav 的 ref-pattern：加 `handleCancelOpenRef` 让 window
keydown listener 拿最新 callback。在 onKey 加 Delete / Backspace 分支：

```ts
} else if (e.key === "Delete" || e.key === "Backspace") {
  setFocusedIdx((prev) => {
    if (prev === null) return null;
    const item = list[prev];
    if (!item) return prev;
    // 仅 pending / error 可被取消；终态行不响应
    if (item.status !== "pending" && item.status !== "error") return prev;
    e.preventDefault();
    handleCancelOpenRef.current(item.title);
    return prev;
  });
}
```

tagName 守卫已在头部排除 INPUT / TEXTAREA / SELECT / BUTTON —— 用户在搜索 /
取消 reason / 创建表单里打字时 Backspace 走原生删除字符。

### 测试

逻辑全前端 React state；项目无 vitest 设施，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | handleCancelOpenRef + onKey Delete/Backspace 分支 |
| **M2** | tsc + build + cleanup |

## 复用清单

- 上一轮 keyboard nav 的 ref-pattern
- 既有 `handleCancelOpen(title)` IO

## 进度日志

- 2026-05-05 42:00 — 创建本文档；准备 M1。
- 2026-05-05 42:05 — 完成实现：`PanelTasks.tsx` 加 `handleCancelOpenRef`；window keydown listener 加 Delete / Backspace 分支（仅 pending/error 行响应，handleCancelOpen 触发既有 inline reason 输入，autoFocus 让焦点跳过去）。`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 任务面板键盘可达性补强。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；ref-pattern 与上一轮键盘 nav 同源由 tsc 保证。
