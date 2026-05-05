# 决策日志按 kind 着色 timeline 色条 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 决策日志按 kind 着色 timeline 头：当前每行 kind 字标颜色；加左侧 4px 色条贯穿整行让快速 skim 时类型一眼可见，与现状互补不替换。

## 目标

PanelDebug 决策日志行格式 `<ts> <kind 字> <reason>`。kind 字虽已上色但
位置在 `ts` 之后，纵向 skim 时多种 kind 混在一长串里仍要逐字看"颜色 +
文字"才能识别。本轮在每行最左侧加 3px 色条贯穿整行（用同款 `kindColor`），
让用户竖向扫视时颜色就成主信息通道。

## 非目标

- 不替换 kind 字标色 —— 字标色提供具体名字，色条只放视觉锚点；两者互补
  不冲突。
- 不动 ts / reason 区颜色。
- 不做 hover 高亮 —— 鼠标 hover 已有复制按钮显出，多一个 kindColor 高亮
  反而抢眼。

## 设计

### 实现

在每行最左侧加 `<span>` 作为色条：宽 3px、与行同高（自动）、背景
`kindColor(d.kind)`、`flexShrink: 0`、`borderRadius: 1`。

由于行容器是 `display: flex; gap: 8px`，色条在第一位 → 与 ts 自然有 8px
gap。整体多占 3 + 0(原 gap 已有) ≈ 4px 水平空间，放在 200px 高度的滚动
容器里完全可接受。

行容器需要 `alignItems: "stretch"`（覆盖 `baseline`）才能让 1-line 内的
`<span>` 拉到行高度全高。但其它 children 用 baseline 对齐文本是关键 ——
方案：色条用 `align-self: stretch` 单独覆盖；其它 children 仍按容器
默认 baseline。

实际 hack：`align-self` 在 baseline 容器里 stretch 不一定按预期 — flex
spec 在 baseline 容器里子项的 cross-size 默认是 auto，stretch 应仍生效
（Chromium / Safari 验证过类似模式）。如果效果不对，回退用 `min-height`
或把整个容器改成 `alignItems: "stretch"` + 给文本子项加 `align-self:
baseline`。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 行内插色条 span + alignSelf |
| **M2** | tsc + build + cleanup |

## 复用清单

- 既有 `kindColor(d.kind)` 函数
- 既有 `pet-decision-row` flex 容器

## 进度日志

- 2026-05-07 19:00 — 创建本文档；准备 M1。
- 2026-05-07 19:05 — M1 完成。每行最左侧插 3px aria-hidden span：背景 `kindColor(d.kind)`、`flexShrink: 0`、`alignSelf: stretch` 拉满行高（即便其它 children 是 baseline 对齐）；与 kind 字标色互补不替换。
- 2026-05-07 19:10 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 951ms)。归档至 done。
