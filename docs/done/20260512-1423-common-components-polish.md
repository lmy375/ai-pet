# common/ 组件视觉对齐（UI 美化 迭代 24）

## 背景

`src/components/common/` 下 4 个共享组件，是被多个 panel 复用的视觉单元，但 hex 残留没收。

## 改动

### `PanelFilterButtonRow.tsx`

filter chip 风格升级（与任务 badges 迭代 3、TaskProposalCard 迭代 23 同 pill 节奏）：
- `padding 2/8 → 2/9`
- `borderRadius 10 → 999`（真正 pill）
- 加 `letterSpacing: 0.3`
- inactive border `#cbd5e1` → `var(--pet-color-border)`
- inactive bg `#fff` → `var(--pet-color-card)`
- inactive color `#475569` → `var(--pet-color-muted)`
- fontSize 10 → 10.5

`opt.accent` 仍由 caller 传 hex（各 timeline 有自己的色域语义），保留 prop 兼容。

### `ImageThumb.tsx`

复制按钮的 success / error toast bg：
- `rgba(22,163,74,0.92)` → `color-mix(<tint-green-fg> 92%, transparent)`
- `rgba(220,38,38,0.92)` → `color-mix(<tint-red-fg> 92%, transparent)`
- idle 态 `rgba(15,23,42,0.78)` 保留（小 chip 角标，需要 hardcoded 深色不依赖主题）

### `ImageLightbox.tsx`

复制 / 下载按钮的 success / error bg：
- 4 处 `rgba(22,163,74,0.85)` → `color-mix(<tint-green-fg> 85%, transparent)`
- 4 处 `rgba(220,38,38,0.85)` → `color-mix(<tint-red-fg> 85%, transparent)`

保留：
- backdrop `rgba(0,0,0,0.85)`（图片查看器 deliberate 强对比黑底）
- 按钮 idle 态 `rgba(255,255,255,0.15)` 透明白底 + 白字（lightbox 整体强制 dark UI）
- ✕ 关闭按钮和边框上的 `rgba(255,255,255,0.x)` 透明白（同上）

## 验收

- 任何 panel 用 `PanelFilterButtonRow` 的 chip 都变 pill 形、节奏与其它 badges 一致。
- 图缩略图 hover 状态的复制按钮 success/error toast 浅 / 深主题下色相一致。
- ImageLightbox 整体仍强制 dark UI（图片查看器约定），复制 / 下载按钮 success/error 跟随 tint token。
- `npx tsc --noEmit` 通过。

## 完成

- [x] PanelFilterButtonRow chip 升级
- [x] ImageThumb / ImageLightbox toast bgs
- [x] 移到 docs/done/
