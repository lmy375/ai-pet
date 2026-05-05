# 设置面板 Esc 清空搜索 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 设置面板搜索清除快捷键：Esc 键清空搜索框（当 search 输入框聚焦时），与 PanelChat 跨会话搜索的 Esc 行为统一。

## 目标

设置面板搜索框现在只能点 ✕ 按钮清空。`PanelChat` 的跨会话搜索面板已经在
input 的 `onKeyDown` 里处理 Esc 退出（`setSearchMode(false) + setSearchQuery("")`）。
本轮把同模式扩到 PanelSettings —— Esc 聚焦搜索框时清空内容（panel 不会因此
退出，只是回到"全部展开"状态）。

## 非目标

- 不在搜索框失焦后全局监听 Esc —— 设置面板 Esc 默认行为保留（如未来加
  modal 等）。仅 input focus 时拦截。
- 不写 README —— 键盘可达性微调。

## 设计

input 加 `onKeyDown` handler：

```tsx
onKeyDown={(e) => {
  if (e.key === "Escape") {
    setSearchQuery("");
  }
}}
```

仅这一行。input 内 Esc 默认无浏览器行为，preventDefault 不必。

## 测试

无后端改动；纯 UI 微调，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | input onKeyDown 拦截 Esc |
| **M2** | tsc + build + cleanup |

## 复用清单

- 既有 `searchQuery` / `setSearchQuery` 状态
- PanelChat 同 Esc 拦截语义

## 进度日志

- 2026-05-05 26:00 — 创建本文档；准备 M1。
- 2026-05-05 26:05 — 完成实现：`PanelSettings.tsx` 搜索 input 加 `onKeyDown`，Esc → `setSearchQuery("")`，与 PanelChat 跨会话搜索 Esc 行为统一。`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 键盘可达性微调。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；2 行改动由 tsc 保证。
