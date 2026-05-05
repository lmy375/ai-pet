# 任务详情面板宽度自适应窗口 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务详情面板宽度自适应窗口：现固定宽度，宽屏下大量空白，detail.md 长行被挤窄换行；面板宽度跟 window 宽度按比例伸展，detail 段最大宽度 800px 上限。

## 目标

任务面板的容器（`s.container`）已经是 `100%` 宽度，window 拉宽时内容自动
跟随。但 detail.md / raw_description / detail panel 全部 stretch，宽屏下：
- 单行 100+ 字 ⇒ 阅读视线大幅水平扫，眼累
- 大量空白也是反例 —— 信息密度低

本轮给"文本段"加 `maxWidth: 800px` 上限：保留 panel 整体跟 window 伸展
（任务列表行 / 时间线表格仍可宽用），但 detail.md / raw_description 段落
化文本框上限 800px，符合「典型阅读舒适行宽 ≈ 60-80 中文字符」的排版常识。

## 非目标

- 不动 `s.container` 整体宽度 —— 列表行的 priority badge / status badge /
  due 列在宽屏下天然分散看着舒服，不该被锁。
- 不动 history 时间线 —— 单行 `ts | action | snippet` 模式宽屏下可读，
  锁宽反而让 snippet 提前 wrap。
- 不动 textarea 编辑器 —— 编辑过程中"想看完整长行"是 raw 写作的核心需
  求；锁宽逼用户横滚反而干扰。

## 设计

只给 3 个 box 样式加 `maxWidth: 800`：
- `detailMdBox`：进度笔记浏览态
- `rawDescBox`：完整描述浏览态
- 不动 `detailPanel` 容器（包了 timeline）

```ts
detailMdBox: {
  ...
  maxWidth: 800,
},
rawDescBox: {
  ...
  maxWidth: 800,
},
```

`maxWidth` 不破坏 narrow window 体验（窗口 < 800 时仍 100%）；wide
window 时锁住到 800px，左对齐。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 给两个 box 加 maxWidth 800 |
| **M2** | tsc + build + cleanup |

## 复用清单

- 既有 detailMdBox / rawDescBox 样式

## 进度日志

- 2026-05-07 13:00 — 创建本文档；准备 M1。
- 2026-05-07 13:05 — M1 完成。`rawDescBox` / `detailMdBox` 各加 `maxWidth: 800`；窗口窄于 800 不生效，宽屏锁阅读行宽（~60-80 中文字符）；不动 timeline / textarea / 列表行。
- 2026-05-07 13:10 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 958ms)。归档至 done。
