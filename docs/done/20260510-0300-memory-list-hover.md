# PanelMemory items 列表 hover 高亮（Iter R122）

> 对应需求（来自 docs/TODO.md）：
> PanelMemory items 列表 hover 高亮：list 行默认无 hover 反馈；hover 时背景 var(--pet-color-bg) 与 card 反差（仿 PanelChat session list 同款），让光标位置 / 可点性更直观。

## 目标

PanelMemory 的 item 列表（搜索结果 + 各分类 items）默认是 `s.item`：bg
= card 白底 + border。鼠标悬停时无任何变化，用户不知道哪行 hover 中、
哪些行可点（实际"编辑" / "删除"按钮在每行内）。

加 hover 高亮：item bg 切到 `var(--pet-color-bg)`（与 card 反差一档），
让光标位置即时可见；与 PanelChat session list / PanelDebug decision row
等其它 panel 的 hover 习惯一致。

## 非目标

- 不动 item border / 文字色 —— 保持稳定，仅 bg 微变避免视觉跳跃
- 不引入 onMouseEnter / Leave + state per row —— inline style 不支持
  `:hover` 伪类，但 `<style>` block + className 模式更优（无 React state，
  也无 N 行 state 开销）
- 不动 modal 内 button hover —— 那是另一个交互层级

## 设计

### 加 className + `<style>` 块

```tsx
<style>
  {`
    .pet-memory-item {
      transition: background-color 0.12s ease;
    }
    .pet-memory-item:hover {
      background: var(--pet-color-bg) !important;
    }
  `}
</style>
```

`!important` 是必需 —— 因为 inline `style.background = card` 优先级高于
普通 CSS rule；只有 `!important` 能反转。

放在 `<div style={s.container}>` 内顶部（与既有 message 等同位置），让
样式作用域贴近 PanelMemory 树根。

### 渲染：给两处 item 加 className

```diff
 // 搜索结果（line 558）
-<div key={i} style={s.item}>
+<div key={i} className="pet-memory-item" style={s.item}>

 // 分类 items（line 1007）
-<div key={i} style={s.item}>
+<div key={i} className="pet-memory-item" style={s.item}>
```

### 测试

无单测；手测：
- light 模式：白卡 item hover → bg 切到极浅灰（#f8fafc），微微反差
- dark 模式：暗卡 item hover → bg 切到深蓝（#0f172a），微微反差
- 切到搜索结果 → 同样有 hover 反馈
- 与既有"编辑 / 删除"按钮 hover 不冲突（按钮自己有 hover 行为，叠在 item
  hover 上）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 加 `<style>` 块 + 两处 className |
| **M2** | tsc + build |

## 复用清单

- 既有 token 系统 var(--pet-color-bg) / card
- PanelDebug 决策行 / PanelChat session list 同款 hover 模式

## 进度日志

- 2026-05-10 03:00 — 创建本文档；准备 M1。
- 2026-05-10 03:08 — M1 完成。容器顶部插 `<style>` block 定义 `.pet-memory-item` + `:hover` rule（transition 0.12s + bg `var(--pet-color-bg) !important`，`!important` 反压 inline `s.item` 优先级）；两处 item div（搜索结果 line 570 + 分类 items line 1020）加 `className="pet-memory-item"`。
- 2026-05-10 03:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 1.03s)。归档至 done。
