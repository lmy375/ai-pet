# detail.md 📜 popover 加「↶ 直接 restore」按钮（iter #308）

## Background

iter #305 ship 了 detail.md 自动版本历史 + 📜 N chip popover，但点击 entry
只复制到剪贴板（owner 必须手动粘回 textarea）。owner 想"我直接想拿回这
份"时需要：复制 → ⌘A 全选 textarea → ⌘V 粘贴 — 3 步。本迭代加「↶」按
钮一键 restore，把 ts 文件内容直接替换到 textarea。

但直接 restore 有"覆盖当前 dirty 内容"风险（owner 正在写新版没保存）—
所以 dirty 时走 armed 二次确认，与既有 `cancelEditArmed` / undoLastDone
等 3s armed 模式同 pattern。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- 新 state `historyRestoreArmedTs: string | null` 跟踪哪个 ts 当前 armed
  （3s 内再点击确认 restore；超时自动清）
- editor 关闭 effect 内 reset `setHistoryRestoreArmedTs(null)`
- 📜 popover 内每条 entry 从单 button 改为 div 容器，包：
  - **ts row**（flex）：ts 文案（flex: 1）+ 📋 复制 mini-button + ↶ restore
    mini-button
  - **preview row**：50 字内容前缀（不变）
- 📋 按钮保留既有 copy-to-clipboard + ✓ 已复制 2.5s 反馈
- ↶ 按钮逻辑：
  - dirty（editingDetailContent !== editingDetailOriginalRef.current）+
    未 armed → setHistoryRestoreArmedTs(ts) + 3s timeout 清；按钮变橙色
    `再点确认` 文案
  - 非 dirty / 已 armed → setEditingDetailContent(entry.content) + 关
    popover + 4s toast `↶ 已 restore <ts>（按 ⌘S 保存写盘）`
- popover 头文案改 "📋 复制 / ↶ restore 替换 textarea" 释义两个动作
- entry 背景色三态：copied → green / restoreArmed → orange / 默认透明

## Key design decisions

- **不自动写盘**：restore 仅替换 textarea，owner 还得按 ⌘S 才落盘。让
  "看一下这版长啥样再决定要不要保存" 成为可能 —— 如果 restore 后觉得不
  对仍可继续编辑或 Esc 取消。同时让 dirty 状态从此刻起跟踪"restored 内
  容 vs 磁盘版" 的差异（与正常 edit 同语义）。
- **armed 仅在 dirty 时触发**：非 dirty 时 restore 等价于"打开一个旧版
  阅读"，零数据丢失，加 armed 反而吵；dirty 时则真的会把 owner 正写的
  东西吞掉，必须 armed。3s 窗口与既有 cancelEditArmed 同 — owner 一秒
  内反应过来。
- **两按钮独立 vs row 整体 onClick**：原 entry 是整 button row click =
  复制。现在两按钮独立后，row 其它空白处不再 clickable —— 但 owner 不
  会失去任何操作（两动作都有显式按钮）。代价是 row 不再 "click anywhere
  to copy"，但收益是 restore 入口直观可见。
- **复用 editingDetailOriginalRef 跟踪 dirty**：restore 后 textarea
  内容会与 originalRef 不同 → dirty marker 自然出现，owner 看到 "● 未
  保存" 知道要 ⌘S。这与正常 edit 行为完全一致，没引入特殊状态。
- **关 popover 而非保留**：restore 后 popover 自动关闭让 owner 视线
  回到 textarea + bulkResultMsg toast — 行为 self-contained 不需 owner
  二次点 chip 关 popover。
- **不引"导出 .md 文件下载"**：scope 控制 — 剪贴板 + textarea 替换两个
  入口已经覆盖典型回滚需求；磁盘上 `.history/<ts>.md` 文件本身仍存在，
  owner 想 export 可去 Finder 找。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)
