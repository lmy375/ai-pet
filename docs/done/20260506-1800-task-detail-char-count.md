# 任务详情进度笔记字数计数 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务详情进度笔记字数计数：detail.md 编辑器现在没字数显示，写到一半不知道总字数；在编辑框右下角加 "N 字" 灰色小字，与 PanelChat 输入框 hint 风格对齐。

## 目标

PanelTasks 的 detail.md 编辑器（textarea + 保存 / 取消按钮）目前没有字数
显示。用户写进度笔记时，"已经写了多少 / 是不是该收尾了"完全靠肉眼估。
本轮在保存按钮行末尾加一个灰色 "N 字" 小字，让用户随时知道当前长度。

## 非目标

- 不做字数上限 / 警告 —— 后端 detail.md 没有 hard limit，加阈值警告会
  误导用户，反而约束自由写作。
- 不动 saved 模式 (`detailMdBox`) 的字数显示 —— 那是浏览态，没有"还在
  写"的语境，加显示是噪音。
- 不做 grapheme cluster 精确计数 —— 用 `Array.from(text).length`（=
  code point 数）已能在中文 / emoji / ASCII 上给出符合直觉的"字数"；
  Intl.Segmenter 在 emoji ZWJ 序列上更准但本身复杂度大、且字符级 1 偏差
  对用户决策无影响。

## 设计

### 计算

```ts
const charCount = Array.from(editingDetailContent).length;
```

在 render 内联一行即可，不为它再起 useMemo（textarea 输入频率与 React
re-render 频率一致，缓存反而引入心智成本）。

### UI

在保存 / 取消 / err 同一行的最末尾加：
```tsx
<span style={{
  marginLeft: "auto",
  fontSize: 11,
  color: "#94a3b8",
  whiteSpace: "nowrap",
}}>
  {charCount} 字
</span>
```

`marginLeft: auto` 把它推到行末；`editDetailErr` 在它之前（保留既有
位置）。错误 + 计数同时存在时各占一边视觉上协调。

### 风格统一

颜色 `#94a3b8` 与 `s.detailHint` 同灰；fontSize 11 与既有 `s.actionBtn`
等小元素一致；`whiteSpace: nowrap` 防止"123 字"被换行劈开。

## 测试

无测试 — 单行 `Array.from(text).length` 与 React render；前端无 vitest。
靠 tsc + 手测足以保证。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 计数 + 行末 span |
| **M2** | tsc + build + cleanup |

## 复用清单

- 既有 `editingDetailContent` state
- 既有保存 / 取消按钮 flex row 容器

## 进度日志

- 2026-05-06 18:00 — 创建本文档；准备 M1。
- 2026-05-06 18:05 — M1 完成。`Array.from(editingDetailContent).length` 字数 + `marginLeft: auto` 推到保存/取消行末；mono 字体灰字与既有 `detailHint` 协调。
- 2026-05-06 18:10 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 959ms)。归档至 done。
