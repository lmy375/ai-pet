# 决策日志重跑按钮 hover 反馈（Iter R148）

> 对应需求（来自 docs/TODO.md）：
> 决策日志重跑按钮 hover 反馈：line 1895-1909 的「重跑」按钮无 hover 态，
> 与同区域「清空」chip / filter 一致性差；加 hover bg overlay (rgba 0.04) +
> cursor 反馈，复用 R130/R135 既有模式。

## 目标

每条决策行（最近 16 条）右侧有「重跑」chip-style button，触发
`handleTriggerProactive` 用最新 prompt 复跑一次主动开口。当前样式仅
`cursor: pointer` 反馈，鼠标进入 / 离开均无视觉变化，与同段 hover-able
元素（reason 文本、复制 chip、决策行整行）的微互动反差明显。

加 R130 (.pet-feedback-row) / R135 (.pet-tool-history-row) 既有 rgba 0.04
overlay 模式 —— 该模式已在多处 buffer 行验证。

## 非目标

- 不动 disabled 态（`triggeringProactive` 时 cursor: not-allowed +
  inline grey bg/fg），disabled 不该 hover 反馈，符合 a11y 期望。
- 不引入新 token —— 现有 rgba 跨主题 ok（dark 下 0.04 几乎不可见但不破坏，
  light 下 subtle hint，与 R130 / R135 一致）。
- 不动按钮位置 / 尺寸 / 文案。

## 设计

### CSS 规则

加在现有 `<style>` block (line 1421-1454) 末尾：

```css
/* R148: 决策行重跑按钮 hover 反馈。disabled 时由 :not(:disabled) 屏蔽
   —— 与 R130/R135 buffer 行 hover 同款 0.04 overlay。inline 背景是
   var(--pet-color-card) 浅底，rgba 叠 4% 黑变 subtle 灰，点完得反馈。 */
.pet-rerun-btn {
  transition: background-color 0.12s ease;
}
.pet-rerun-btn:not(:disabled):hover {
  background: rgba(0, 0, 0, 0.04) !important;
}
```

`!important` 必要：button inline `background: var(--pet-color-card)` 优先级
比类规则高，必须覆盖。R135 同样原因加了 !important。

### button 标记

line 1895 button 加 `className="pet-rerun-btn"`。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 加 CSS 规则 + className |
| **M2** | tsc + build |

## 复用清单

- R130 / R135 既有 rgba 0.04 hover overlay
- pet-feedback-row / pet-tool-history-row className 命名约定

## 进度日志

- 2026-05-11 05:00 — 创建本文档；准备 M1。
- 2026-05-11 05:20 — M1 完成：CSS rule + className；M2 tsc + build 通过。
  归档。
