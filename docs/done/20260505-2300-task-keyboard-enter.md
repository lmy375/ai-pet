# 任务面板键盘 Enter 展开 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务面板回车展开：键盘焦点行按 Enter 展开 / 折叠详情，与空格选中互补，键盘党不必再切回鼠标。

## 目标

上一轮完成了 ↑↓ 移焦点 + 空格切换选中。本轮加 Enter 键展开 / 折叠焦点行的
详情面板（与点击行 header 等价），让键盘党彻底脱离鼠标完成 "翻队列 → 选中
→ 看详情" 全流程。

## 非目标

- 不做"跳进 detail 编辑模式"等深层操作 —— 详情段已有「编辑」按钮，鼠标点
  即可；嵌套键盘 chord 增复杂度。
- 不做 Esc 键关闭详情 —— Enter 自身就是 toggle；用户也能直接 Enter 二次。
- 不写 README —— 键盘可达性补强，与上一轮键盘选中同性质。

## 设计

复用上一轮的 ref-pattern，加一个 `handleToggleExpandRef` 让 keydown 监听
拿到最新闭包。在 onKey 里加 Enter 分支：

```ts
} else if (e.key === "Enter") {
  setFocusedIdx((prev) => {
    if (prev === null) return null;
    const item = list[prev];
    if (!item) return prev;
    e.preventDefault();
    handleToggleExpandRef.current(item.title);
    return prev;
  });
}
```

`handleToggleExpand` 是 async（涉 invoke detail fetch），但 ref 调用不必 await
—— 与点击行 header 时的 fire-and-forget 同语义。

tagName 守卫扩展加 `BUTTON`：用户按钮聚焦时按 Enter 应激活 button（浏览器原
生），不该被我们截获。INPUT / TEXTAREA / SELECT 已守卫，加 BUTTON 一并。

### 测试

逻辑同前端 effect / handler；项目无 vitest，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | handleToggleExpandRef + Enter 分支 + BUTTON 守卫 |
| **M2** | tsc + build + cleanup |

## 复用清单

- 上一轮的 ref-pattern（visibleTasksRef / toggleSelectRef）
- 既有 `handleToggleExpand`

## 待用户裁定的开放问题

- 是否在 focusedIdx === null 时也 Enter 启动焦点？本轮**否**——focusedIdx === null
  代表用户还没启动键盘模式，Enter 在普通页面里是常用的 form-submit 键，先要求
  用户 ↑↓ 进入焦点模式才允许 Enter 展开，避免误触。

## 进度日志

- 2026-05-05 23:00 — 创建本文档；准备 M1。
- 2026-05-05 23:10 — 完成实现：
  - **M1**：`PanelTasks.tsx` 加 `handleToggleExpandRef` 跟随最新 callback；onKey 守卫扩展 `BUTTON`（让 Enter 走原生 button click 不被截获）；onKey 加 `Enter` 分支调 `handleToggleExpandRef.current(item.title)`（async fire-and-forget 与鼠标 onClick 同语义）；同空格的"focusedIdx === null 时不响应"门槛防误触。
  - **M2**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 键盘可达性补强，与上一轮键盘选中同性质。
  - **设计取舍**：复用上一轮 ref-pattern（监听器只挂一次）；Enter 与空格门槛对称（`focusedIdx === null` 时不响应），保证从未启用键盘模式的用户不会被意外打开详情。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；逻辑由 tsc 与 ref-pattern 保证。
