# PanelDebug 决策日志行 hover bg 高亮（Iter R133）

> 对应需求（来自 docs/TODO.md）：
> PanelDebug 决策日志行 hover bg 高亮：现 .pet-decision-row 仅 copy-btn hover-only 显隐；加 :hover bg overlay rule（与 R130 反馈行 / R131 会话行同款 rgba），让行边界即时可见。

## 目标

PanelDebug 决策日志已用 `.pet-decision-row` className（R83/R84）做 copy-btn
显隐：copy-btn 默认 opacity 0、hover 行时 opacity 1。但行整体 bg 没变化，
密集列表中扫不到光标当前在哪行。

加 `.pet-decision-row:hover` bg overlay（与 R130 反馈行 / R131 会话行
同款 `rgba(0,0,0,0.04)`），与现有 copy-btn 显隐叠加 — 不冲突。

## 非目标

- 不动 copy-btn opacity 转换（既有 R83/R84 已 work）
- 不引入选中态高亮 / 锁定行 —— 决策行无 click expand 语义
- 不改决策日志容器 bg（R83 灰色 var(--pet-color-bg)）—— hover 是 row 级，
  容器级不动

## 设计

既有 `<style>` block 末追加 hover bg rule。`.pet-decision-row` 已有
`transition: opacity 0.12s ease`（仅 transition opacity）；但本轮加
background-color 也希望平滑切换，扩展 transition：

```diff
 .pet-decision-row .pet-decision-copy-btn {
   opacity: 0;
   transition: opacity 0.12s ease;
 }
+.pet-decision-row {
+  transition: background-color 0.12s ease;
+}
+.pet-decision-row:hover {
+  background: rgba(0, 0, 0, 0.04);
+}
 .pet-decision-row:hover .pet-decision-copy-btn {
   opacity: 1;
 }
```

不用 `!important` —— 决策行 inline style 没设 background（容器决定 bg）；
普通 CSS rule 优先级足够。`rgba(0,0,0,0.04)` 跨 light/dark 都呈微 subtle，
与 R130/R131 一致。

### 测试

无单测；手测：
- 决策行 hover → bg 出现微暗 overlay + copy-btn opacity 1
- 移开 → 都恢复 0
- 与既有 kindColor 色条 / 文字色不冲突
- light / dark 主题切换都生效

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `<style>` 加 transition + :hover rule |
| **M2** | tsc + build |

## 复用清单

- 既有 `.pet-decision-row` className
- R130 / R131 hover overlay 风格

## 进度日志

- 2026-05-10 14:00 — 创建本文档；准备 M1。
- 2026-05-10 14:08 — M1 完成。既有 `<style>` block 内 `.pet-decision-row .pet-decision-copy-btn` 之后追加 `.pet-decision-row` transition + `:hover` rgba bg overlay rule（rule order 让 hover bg 在 copy-btn opacity 之前；CSS 不分顺序但 readability 上分两组）。
- 2026-05-10 14:11 — M2 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
