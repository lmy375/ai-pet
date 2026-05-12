# 任务详情 detail.md 编辑器抛光（UI 美化 迭代 21）

## 背景

任务面板里 detail.md 是用户写"进度笔记"的核心区。三个相关容器（detail 渲染框 / 编辑 textarea / 预览面板 / raw description 框）都偏简陋：
- padding 6/8 视觉局促
- borderRadius 4 锐角
- 没 shadow
- `border: 1px solid #f1f5f9` / `dashed #cbd5e1` 写死 light slate 值
- lineHeight 1.55 略紧

## 改动

`PanelTasks.tsx`：

### `s.detailMdBox`（detail.md 渲染框）
- padding 6/8 → 12/16
- borderRadius 4 → 8
- border `#f1f5f9` → `var(--pet-color-border)`
- lineHeight 1.55 → 1.65
- 新增 `boxShadow: var(--pet-shadow-sm)`

### `s.rawDescBox`（任务 raw description 灰底框）
- padding 6/8 → 10/14
- borderRadius 4 → 8
- 新增 `border: var(--pet-color-border)`
- lineHeight: 1.6（新增，原无）

### `s.detailPanel` / `s.bulkSubPanel` 边线 token 化
- `border: 1px dashed #e2e8f0` / `#cbd5e1` → `var(--pet-color-border)`
- bulkSubPanel radius 4 → 6

### detail.md 编辑 textarea + side-by-side preview / preview-only
- textarea padding 8/10 → 12/14；minHeight 100 → 120；radius 4 → 8；lineHeight 1.55 → 1.65；border 走 token；新增 `boxShadow: var(--pet-shadow-sm)` 和 `background: var(--pet-color-card)`
- preview pane：dashed border 用 `color-mix(<border> 80%, <accent>)` 让"预览态"有 accent 染色提示用户视图差异
- preview-only 模式同样升级

## 验收

- 展开任务详情时，detail.md 框、编辑 textarea、预览面板三块视觉一致：圆角 8、border tokens、padding 更宽松、lineHeight 1.65 更舒展。
- 编辑器 textarea 有 shadow-sm 浮起感；预览态 dashed border 偏 accent 提示。
- 浅 / 深主题自动跟随。
- `npx tsc --noEmit` 通过。

## 完成

- [x] detailMdBox / rawDescBox / detailPanel / bulkSubPanel 升级
- [x] detail.md textarea + preview pane 升级
- [x] 移到 docs/done/
