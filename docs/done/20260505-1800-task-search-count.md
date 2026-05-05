# 任务搜索框结果计数 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务搜索框结果计数：现「任务」面板搜索框命中后没显示"X / N 条匹配"，加一个微小计数让用户知道过滤强度。

## 目标

「任务」面板搜索 / tag chip 过滤命中后没数字反馈，用户不知道"我这次过滤把
70 条收窄成了 5 条"还是"全没命中"（empty list 时只看到 "没有匹配筛选条件
的任务"，但全可见时也没参考量）。本轮在搜索行右端加一个微小的 `X / N 条匹
配` 灰色文字，仅在 search 或 tag 过滤启用时显示。

## 非目标

- 不为 showFinished toggle 添加计数 —— 它是"显示已结束 yes/no" 的二元 lens，
  不是缩窄的过滤；filtersActive 已经把 showFinished 排除在外。
- 不显示加权 / 类型分布（"3 pending / 2 done"等）—— 列表项自身的 status
  badge 已能扫读。
- 不写 README —— 任务面板的可见性微调。

## 设计

PanelTasks 的 search 行末加一段：

```tsx
{filtersActive && (
  <span style={s.searchCount}>
    {visibleTasks.length} / {tasks.length} 条匹配
  </span>
)}
```

样式 `s.searchCount`：12px 灰色（`#94a3b8`），`alignSelf: center`，`flex-shrink: 0`，
`whiteSpace: nowrap`，左侧 padding 6px 让数字与 ✕ 按钮分开。

`filtersActive` 已有定义（`trimmedSearch.length > 0 || selectedTags.size > 0`），
本轮直接复用 —— 与"没有匹配筛选条件的任务"empty-state 的判定同源，避免漂移。

`tasks.length` 用全集（含 done / cancelled）—— 让用户感受到的是"原始队列里
有 N 条，我命中了 X 条"，而不是再叠一层 showFinished 过滤的中间结果（中间数
字会让 X 与 N 关系含糊）。

## 测试

无后端改动；纯 UI 微调。靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | s.searchCount 样式 + JSX 插入 |
| **M2** | tsc + build + cleanup |

## 复用清单

- `filtersActive` / `visibleTasks` / `tasks` 现有派生
- `s.searchRow` flex 容器

## 待用户裁定的开放问题

- 计数位置：search 行末 vs tag 行末？search 行末（与 search 输入框语义直接关联）。

## 进度日志

- 2026-05-05 18:00 — 创建本文档；准备 M1。
- 2026-05-05 18:10 — 完成实现：
  - **M1**：`PanelTasks.tsx` 加 `s.searchCount` 样式（12px 灰 + alignSelf center + flex-shrink 0 + nowrap）；search 行末插入条件渲染 `<span>X / N 条匹配</span>`，仅 `filtersActive` 时显示（避开 showFinished toggle 噪音）。复用既有 `filtersActive` / `visibleTasks` / `tasks` 派生。
  - **M2**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 任务面板可见性微调。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；纯 UI 字段，与既有 `filtersActive` empty-state 同源判定，由 tsc 保证。
