# detail.md 打开自动滚到最新 `- [x]` 行

## 背景

TODO 上 auto-proposed 一条："detail.md 打开自动滚到最新 `- [x]` 行：打开 detail.md 时若末尾有完成行 scrollIntoView，让 owner 一眼看到『最近一次动作是什么』。"

近几轮强化了 detail.md 作为"任务进度笔记"载体的工具链：

- 工具栏 `✓ 完成行` 按钮插 `- [x] YYYY-MM-DD HH:MM ` 模板
- ☑ checklist 进度 chip 显完成度
- 工具栏 📅 当前时间按钮

但 detail.md 写多了之后（5-20 条完成记录），打开 detail 时 textarea 默认从文首开始 —— owner 想看"上次做到哪了"还得手动滚到底。逻辑很自然：打开 = 想接着写 = 应该看到最新动作。

本 iter 在进入编辑时自动找最后一条 `- [x]` 行，光标落到该行末尾（按 Enter 即起新一行接着记），并把 textarea 滚到该行附近视觉中央。

## 改动

### `src/components/panel/PanelTasks.tsx`

在 `handleEnterEditDetail` 末尾追加 rAF 块：

```ts
requestAnimationFrame(() => {
  if (!currentMd) return;
  const lines = currentMd.split("\n");
  let lastDoneLineStart = -1;
  let lineIdxOfLastDone = -1;
  let offset = 0;
  for (let i = 0; i < lines.length; i++) {
    if (/^\s*- \[[xX]\] /.test(lines[i])) {
      lastDoneLineStart = offset;
      lineIdxOfLastDone = i;
    }
    offset += lines[i].length + 1; // +1 for \n
  }
  if (lastDoneLineStart < 0) return;
  const ta = detailEditorRef.current;
  if (!ta) return;
  // 光标到该行末尾。用户敲 Enter 即新起一行写下一条完成记录。
  const lineEnd = currentMd.indexOf("\n", lastDoneLineStart);
  const cursor = lineEnd === -1 ? currentMd.length : lineEnd;
  ta.selectionStart = ta.selectionEnd = cursor;
  ta.focus();
  // 强制把那行滚到 textarea 中央
  const lineHeight = 12 * 1.65;
  ta.scrollTop = Math.max(
    0,
    lineIdxOfLastDone * lineHeight - ta.clientHeight / 2,
  );
  setDetailCursorPos(cursor);
});
```

## 关键设计

- **光标落到行末而非行首**：用户敲 Enter 即起新一行 —— 接着写下一条完成记录是"打开 detail 后最高频"的动作（看到最新进度 + 接着记）。落行首反而让 Enter 在该行之前插空白，破坏 markdown 结构。
- **居中滚动而非滚到顶 / 底**：让 owner 看到"最新动作 + 上下文一行/两行"。`lineIdxOfLastDone * lineHeight - ta.clientHeight / 2` 把目标行放视窗 50% 位置。textarea 不够高时 `Math.max(0, ...)` 保 scrollTop 非负。
- **lineHeight 估算 12 × 1.65 = 19.8px**：直接读 CSS `fontSize: 12` × `lineHeight: 1.65` 配置值，比 `getComputedStyle` 简单 + 数字稳定（除非 CSS 改否则不漂）。略低估造成行偏高一点点，但视觉容差大。
- **rAF 等 React 提交 + textarea autoFocus**：`setEditingDetailContent` 之后立即操作 textarea 会拿不到新 value（React 还没 paint）。rAF 跨过提交 + 浏览器默认 autoFocus 行为，再设 selection / scrollTop。
- **同步更新 detailCursorPos**：让底部「行 N / 共 M」状态栏立即反映光标位置 —— 不更新会导致行号 chip 显 "行 1"，但 cursor 已在第 47 行，逻辑不一致。
- **正则 `^\s*- \[[xX]\] `**：与 `toggleEditChecklistLine` / `insertDoneLineAtCursor` / checkbox 进度 chip 同形（识别 `- [x]` 和 `- [X]`，行首允许 indent）。一致 marker 协议 = 三个工具看到的"done 行"完全相同。
- **无 done 行时不动**：保持文首默认行为 —— 新任务 / 全 `[ ]` 待办的 detail.md 没"最近动作"概念，强行跳变奇怪。

## 不做

- **不跳到最新 `[ ]` 未完成行**：未完成行有多条 + 没序时序，"最新"含义模糊；done 行天然有"做完的顺序"。
- **不显"已滚到最新 [x]" toast**：滚动本身视觉可见 —— 一行 toast 反噪音。
- **不写测试**：textarea selectionStart / scrollTop / requestAnimationFrame 在 jsdom 下行为不真实；视觉验证（开个含多条 `[x]` 的 detail 看是否滚对位置）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.20s
- 改动 ~40 行（rAF 块 + comment）；既有 handleEnterEditDetail 路径 + textarea autoFocus + dirty marker 等行为不变。

## TODO 状态

6 条候选 auto-proposed 已完成 3 条，余 3 条留池：
- session 下拉按月份分组折叠
- 桌面 ChatPanel ⌘K 任务 ref picker
- TG /pinned 命令

## 后续

- 滚到最新 `- [x]` 的"打开行为" 配 localStorage 开关：偏好"每次从文首读"的用户能关掉。当前默认开符合多数 active 任务工作流。
- 同一逻辑扩到"最新 `[ ]` 未勾"：当无 done 行时滚到第一条未勾 —— 让"接着做"也成为默认入口。
- 自动 highlight 那行 ~1.5s（粉色 / 黄色背景渐隐）让 owner "我滚到这里了" 视觉清楚；当前仅 cursor 位置 + 视窗中部位置，无 highlight。
