# 任务详情面板宽度记忆 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务详情面板宽度记忆：现 maxWidth 800 固定；用户可能希望调整阅读宽度；加左右把手拖动调整且 localStorage 持久化（限 600-1200px 范围）。

## 目标

`detailMdBox` / `rawDescBox` 当前 maxWidth 固定 800px。不同用户偏好不同
阅读行宽（800 是中文 60-80 字推荐，但有人想要更紧凑或更宽广）。本轮加
个 `<input type="range">` 滑条让用户调，限 600-1200px，写 localStorage
跨 session 记忆。

## 非目标

- 不做 free-form 数字输入 —— range slider 提供边界保护 + 物理拖感，比
  text input 误输 - 后崩溃 UI 安全。
- 不做"自动模式（窗口宽度的 N%）"—— 阈值绝对值（如 800px）对中文阅读
  舒适度可控；按窗口比例反而不稳定。
- 不动其它面板（chat / debug / persona）—— 它们各自布局逻辑不同，统一
  不强求。

## 设计

### state + 持久化

`detailMaxWidth: number` default 800。组件 mount 时从 localStorage 读
`pet-task-detail-max-width` 数字键，clamp 到 [600, 1200]；解析失败 / 越
界 / null → 用 default 800。每次变更立即写回。

### UI 位置

在 detail panel 顶部（"完整描述" 标签前）一个小工具行：
```
┌─ 阅读宽度 [—————•——] 900px ─┐
```
`<input type="range" min="600" max="1200" step="50">` + 数值显示。

只在有 expanded detail 时显示（与 panel 容器条件一致）。

### 应用

`detailMdBox` / `rawDescBox` 的 `maxWidth` 改为内联读 state。需要把这两
个静态 style 表里的 `maxWidth: 800` 抽出 — 改成在 JSX 内联 `style={{
...s.detailMdBox, maxWidth: detailMaxWidth }}`。两处 styles 共享同 state。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + localStorage load/save + clamp 边界 |
| **M2** | range slider UI + 数值显示 |
| **M3** | detailMdBox / rawDescBox 改 inline style |
| **M4** | tsc + build + cleanup |

## 复用清单

- 既有 `s.detailMdBox` / `s.rawDescBox` 样式（保留所有非 maxWidth 字段）
- 既有 detailPanel 容器作为工具行 anchor

## 进度日志

- 2026-05-07 22:00 — 创建本文档；准备 M1。
- 2026-05-07 22:05 — M1 完成。`detailMaxWidth` state 默认 800，初始化时 lazy 从 localStorage 读 + clamp [600, 1200]；变更时 useEffect 写回，try/catch 兜底无痕模式 / 配额满。
- 2026-05-07 22:10 — M2 完成。detail panel 顶部加灰字小工具行：`<input type="range" min=600 max=1200 step=50>` + 数值实时显示；title hover 解释跨 session 记忆。
- 2026-05-07 22:15 — M3 完成。`detailMdBox` / `rawDescBox` 改 inline `{...s.X, maxWidth: detailMaxWidth}`，覆盖原 800 静态值。
- 2026-05-07 22:20 — M4 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 1.06s)。归档至 done。
