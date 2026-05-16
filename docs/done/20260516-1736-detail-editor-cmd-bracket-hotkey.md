# detail.md 编辑器 ⌘[ / ⌘] 键盘绑定 prev/next 任务导航

## 背景

iter #183 给 detail.md 编辑器加了 ↑/↓ 按钮（view-mode row）。键盘党更想"打字 + 切下一条"不必抬手摸鼠标点按钮 —— 加一对 ⌘[ / ⌘] 同语义快捷键。

⌘[ / ⌘] 是 IDE / 浏览器内常用的"后退 / 前进"绑定（VSCode 跳上一个 / 下一个编辑位置）—— owner 直觉。

## 改动

### `src/components/panel/PanelTasks.tsx`

在 `handleNavigateDetail` useCallback 之后加：

```ts
useEffect(() => {
  if (editingDetailTitle === null) return;  // 不在编辑 detail 时不挂
  const handler = (e: KeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    if (e.shiftKey || e.altKey) return;  // ⇧/⌥ 同按时不抢，留给其它 shortcut
    if (e.key === "[") {
      e.preventDefault();
      void handleNavigateDetail("prev");
    } else if (e.key === "]") {
      e.preventDefault();
      void handleNavigateDetail("next");
    }
  };
  window.addEventListener("keydown", handler);
  return () => window.removeEventListener("keydown", handler);
}, [editingDetailTitle, handleNavigateDetail]);
```

### 按钮 tooltip 加 ⌘[/⌘] hint

```tsx
title={hasPrev ? `跳到上一条任务（... #${curIdx} → #${curIdx - 1}） · ⌘[。...` : "已是第一条"}
title={hasNext ? `跳到下一条任务（... #${curIdx} → #${curIdx + 1}） · ⌘]。...` : "已是最后一条"}
```

发现按钮的用户也能 hover 学到键盘绑定。

## 关键设计

- **gate on `editingDetailTitle !== null`**：仅在打开某 task 的 detail.md 时挂监听 → unmount 时 cleanup。owner 没编辑时按 ⌘[ 不抢用，留给系统 / 其它快捷键。
- **复用 `handleNavigateDetail`**：与按钮同 handler，dirty flush draft / detailMap 缓存 / IPC fallback / setPendingTitleFocus 走一条路径 —— ↑ 按钮 == ⌘[ 行为永远一致。
- **不区分 textarea focused vs blurred**：⌘[ / ⌘] 在 textarea 内默认无特殊行为（不像 ⌘B / ⌘I 那样可能与 RTL 输入冲突），抢用安全。owner 在打字时按 ⌘] = 切下条任务 + dirty 自动入草稿 —— 完美匹配心智模型。
- **过滤 ⇧ / ⌥ modifier**：避免 ⌘⇧[ / ⌘⌥] 类 macOS 系统级 shortcut 被这条规则误吃。仅"纯 ⌘[" 和 "纯 ⌘]" 触发。
- **`metaKey || ctrlKey` 跨平台**：mac ⌘ + Linux/Windows Ctrl 都能用。
- **preventDefault 防系统默认**：mac 浏览器版会用 ⌘[ = back，preventDefault 阻断。PanelApp 是 Tauri webview 内 SPA，没 history，但仍保险。
- **tooltip 加 ⌘[ / ⌘] 提示**：键盘党不读发布说明，但常 hover 按钮 → 自然发现快捷键。

## 不做

- **不写 textarea 选区内的 `[` `]` 字符插入冲突处理**：在 selection 内按 ⌘+] 是导航，不是输入 —— 与 owner "我在编辑时切换" 直觉一致。⌘+] 单按 = 切换；要输入 `]` 字符直接按 `]` 即可。
- **不绑 ⌃P / ⌃N 同义**：避免与 readline-style 全局光标移动 hijack；⌘[/⌘] 已够。
- **不让 textarea blur 时也触发**：编辑器外按 ⌘[/⌘] 没意义；effect 已 gate 在 editingDetailTitle 非空。
- **不写测试**：纯键盘 -> 既有 handler 一线触发；handleNavigateDetail 路径 iter #183 已视觉验证。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.18s
- 改动 ~35 行（useEffect 20 + 两 tooltip hint 2 + 注释 13）；既有 handleNavigateDetail + ↑/↓ 按钮 + dirty flush / detailMap 缓存路径完全不动。

## TODO 状态

新一批 6 条 auto-proposed，本 iter 完成 1 条，余 5 条：
- ChatMini 历史区双击 user/assistant 气泡内的「title」ref token 跳 PanelTasks
- 桌面 pet 右键菜单加「切 Live2D 模型」子菜单
- PanelMemory 类目卡 sparkline 全 7 天 0 时显「闲置 7d+」灰 hint
- butler_task 描述新增 [reminderMin: N] 标记
- PanelTasks 行右键菜单加「复制为 markdown 引用块」

## 后续

- ⌘⇧[ / ⌘⇧] 在编辑器视图模式间切换（view ↔ edit ↔ split）—— 当前要点 view-mode row 三按钮。
- 加 ⌘K 通用"快速跳到任务" mini palette（input → fuzzy 命中 → Enter 切到该 task detail）—— 比 ⌘[/⌘] 顺序导航更强力。
- 按 ⌘[/⌘] 时给一次 toast/visual flash 反馈 "→ task 「X」"，让 owner 视觉确认目标。
