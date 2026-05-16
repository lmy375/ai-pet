# detail.md 编辑器「🔢 显行号 gutter」toggle（iter #265）

## Background

detail.md 编辑器底部状态栏已显"行 N / 共 M"光标行号 chip，但 owner 在长文
编辑时常想"扫读全文同时知道每段在第几行"。IDE 风格的左侧 gutter 行号列是
习惯入口。

本迭代加 markdown toolbar 的「🔢」toggle，开启后在 edit 模式 textarea 左侧
浮一列行号；状态持久化 localStorage 跨重启保留偏好。仅 edit 模式启用 —
split 模式横向已紧 + wrap mismatch 更明显，本迭代不动 split。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- **state**：
  - `showDetailGutter: boolean`（localStorage `pet-detail-gutter` 持久化）
  - `detailGutterRef: useRef<HTMLDivElement>`（用 imperative scrollTop 同步）
  - `toggleShowDetailGutter` useCallback

- **toolbar 按钮**：在既有 `✓ 完成行` 与 `「」 插 task ref` 之间插「🔢」
  toggle，仅 `detailViewMode === "edit"` 时显示。toggle on 时背景变 blue
  tint + 字色对比。aria-pressed 反映状态。

- **edit 模式 textarea 包装**：原 `<textarea ...>` 改为
  `<div display: flex>{<gutter>?}<textarea onScroll=sync /></div>`。
  - gutter div：宽 36px / `padding: 12px 4px 12px 8px` / 同 textarea 的
    fontSize 12 + fontFamily 'SF Mono' + lineHeight 1.65 让行号刻度匹配
  - 内容：`Array.from({length: lineCount}, (_, i) => i + 1).join("\n")`
    （`whiteSpace: pre` 保 `\n` 显成多行）
  - textarea `borderLeftWidth: 0` + `borderRadius: "0 8px 8px 0"`（gutter
    打开时），让 gutter + textarea 视觉拼成一块
  - textarea onScroll → `detailGutterRef.current.scrollTop = scrollTop` 同步

## Key design decisions

- **按 `\n` 分段（逻辑行）而非视觉行**：实现简单 + 与"行 N / 共 M" status
  bar 同语义。代价是 textarea 的视觉 wrapping 会让 gutter 比 textarea 短，
  长 markdown 行会让行号对不齐。tooltip 文案说清楚 + 默认 off + 持久化让
  owner 自己评估是否启用。
- **仅 edit 模式启用**：split 模式横向只剩 50% 宽，wrap 概率高 + mismatch 严
  重；preview 没 textarea 无意义。toolbar 按钮也仅 edit 模式渲染避免 owner
  在 split / preview 模式下点了无效。
- **imperative scrollTop sync 而非 state**：onScroll 高频触发；state 改
  re-render gutter 不必要 — 直接读 ref + set DOM scrollTop 是最便宜的同步
  机制。
- **localStorage 持久化 + 默认 off**：行号 gutter 是个人偏好（程序员喜欢 /
  作家未必），不预设；持久化让 owner 第一次开启后跨重启保留。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)
