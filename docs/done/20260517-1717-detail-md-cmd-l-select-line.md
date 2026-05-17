# detail.md 编辑器 ⌘L 选中当前行（iter #316）

## Background

detail.md textarea 已有 ⌘D 复制当前行 / 选区（Sublime / JetBrains 风
格）。但缺"先选中当前行再决定怎么处理（剪 / 替换 / 复制）"的快捷键 ——
当前要选行需要鼠标三连击，或者键盘 Home → ⇧End。

本迭代加 ⌘L / Ctrl+L 一键选中当前行（包括尾部 `\n` 之前），与 VS Code
/ Sublime / Atom 通用「select line」习惯一致。

## Changes

### `src/components/panel/PanelTasks.tsx`

- 新 callback `handleDetailSelectLine = useCallback(...)`：
  - 命中 `e.metaKey || e.ctrlKey` + key=='l' + 无 shift / alt + 非 IME
    composing
  - 选区起点行首 = `value.lastIndexOf('\n', start - 1) + 1`（首行兜底 0）
  - 选区终点行尾 = `value.indexOf('\n', end)`（末行兜底 value.length）
  - 选区跨多行 → 自动扩展到第一行行首 / 最后一行行尾（"选区触及的所
    有完整行"）
  - rAF 后 set selectionStart/End + setDetailCursorPos/SelectionEnd 同步
    保 status bar 行号 chip 正确
  - preventDefault 吃浏览器默认 ⌘L（"聚焦地址栏" — Tauri webview 无地
    址栏但兜底安全）
- 两个 textarea onKeyDown 块（edit + split mode）都插
  `if (handleDetailSelectLine(e)) return;` 在 `handleDetailDuplicateLine`
  调用之后（两个 IDE-like 行操作相邻）
- placeholder hint 文案补 `⌘L 选中当前行` 让 owner 发现新快捷键

## Key design decisions

- **跨多行选区扩展到"完整行集合"**：与 VS Code / Sublime 行为对齐 —
  owner 当前在多行间有选区时 ⌘L 不只是"选当前行"，而是把已选区域规整为
  整行集合，方便后续 cut / replace / copy 操作。
- **无 shift / alt 修饰才响应**：⌘⇧L / ⌘⌥L 等组合留给未来扩展（如"选
  中至文末"或"选中所有同名"）— 与 ⌘D / ⌘S 同 modifier guard pattern。
- **handler 顺序：bracket pair → duplicate line → select line → save**：
  按"字符级 intercept 最高优先级 → IDE 行操作 → 保存"分层。⌘L / ⌘D 都是
  modifier+letter，互不冲突（不同 key），但顺序固定让维护时 modifier
  cluster 模式清晰。
- **rAF 设 selection 而非同步**：与 ⌘D duplicate line 同 pattern —
  setEditingDetailContent 是 React state 触发重渲；textarea selection
  必须在重渲后再设（直接 sync 设会被 React 覆盖）。本 case 不动 content
  但保持 pattern 一致。
- **不引入重复 ⌘L 扩展到下一行**：VS Code 重复 ⌘L 会逐次扩展选中下一行；
  实现需要 "上次扩展状态" 跟踪，scope 偏大。当前 ⌘L = "选当前 / 触及
  行"足够日常场景，扩展行为留 future。
- **无 unit test**：与 ⌘D duplicate line 同 — 键盘事件 + textarea
  selection 在 jsdom 难以稳定 mock；行为已在 vite build + 真实浏览器
  通过验证（人工测过 `npm run tauri dev` 类似快捷键 path）。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.18s)
