# 决策日志 chip — inactive 弱色 accent 边框（Iter R84）

> 对应需求（来自 docs/TODO.md）：
> 决策日志按 kind 着色 chip 文字：现 chip 选中时填充背景色 + 白字；改成 inactive 也用 accent 边框（弱色），让 inactive 也能一眼区分类型，强化 timeline 色条联动。

## 目标

PanelDebug 决策日志顶部的 kind 多选 chip（开口绿 / 沉默紫 / 跳过橙 / 全部灰），
当前 inactive 态用中性边框 `var(--pet-color-border)`，看不出"这个 chip 对应哪
种 kind"——必须 hover label 才认得。

把 inactive 边框换成 `${accent}66`（40% alpha 的 accent）：

- 开口 chip 即便不选中也带淡绿色边框
- 沉默 chip 淡紫
- 跳过 chip 淡橙
- "全部" chip 淡灰（accent=`#475569`，本身就是中性 slate，不变）

视觉链路：决策行左侧 3px `kindColor` 色条 ↔ 顶部 inactive chip 的边框色，
两侧用同色锚定，纵向 skim 时颜色成为主信息通道。

## 非目标

- 不改 active 态视觉（仍是 accent 填充 + 白字）
- 不改文字颜色 — 仍是 `var(--pet-color-fg)`，避免 amber `#f59e0b` 等低对比度
  色直接当 body text（WCAG AA 难达标）。颜色信号交给边框承担。
- 不改决策行内部的 kindColor 标签（已是 accent 文字 + 色条）

## 设计

### chipStyle 当前态（迭代 3 之后）

```ts
const chipStyle = (isActive: boolean, accent: string): React.CSSProperties => ({
  ...
  border: `1px solid ${isActive ? accent : "var(--pet-color-border)"}`,
  background: isActive ? accent : "var(--pet-color-card)",
  color: isActive ? "#fff" : "var(--pet-color-fg)",
});
```

### 改后

```ts
const chipStyle = (isActive: boolean, accent: string): React.CSSProperties => ({
  ...
  border: `1px solid ${isActive ? accent : `${accent}66`}`,
  background: isActive ? accent : "var(--pet-color-card)",
  color: isActive ? "#fff" : "var(--pet-color-fg)",
});
```

`${accent}66`：CSS 8-digit hex `#RRGGBBAA`，`66` ≈ 40% alpha。所有 5 种 accent
(`#16a34a` / `#a855f7` / `#f59e0b` / `#dc2626` / `#475569`) 都是 6-digit hex，
统一拼 `66` 即得到弱化版。

### 测试

无单测；手测：
- "全部" 单选时 → 其它 3 个 kind chip inactive，边框各显淡绿/淡紫/淡橙
- 点中"沉默" → "全部" inactive（淡灰），"沉默" active（紫填充）
- light + dark 两种主题下都看到弱色边框（白卡 + dark 卡上都可见）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | chipStyle 一行改动 |
| **M2** | tsc + build |

## 复用清单

- 既有 chipStyle / 决策日志多选过滤
- 既有 kindOptions accent 数组

## 进度日志

- 2026-05-08 07:00 — 创建本文档；准备 M1。
- 2026-05-08 07:05 — M1 完成。chipStyle inactive 边框从 `var(--pet-color-border)` 改为 `${accent}66`（40% alpha 8-digit hex）；加注释说明为什么文字保留 fg 而非 accent（amber/red 等色直接做 body 难达 WCAG AA）。
- 2026-05-08 07:08 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (499 modules, 947ms)。归档至 done。
