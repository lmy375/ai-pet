# PanelChat ⌘K 全局任务 ref picker（脱离 textarea 焦点）

## 背景

TODO 上 auto-proposed 一条："桌面 ChatPanel ⌘K 全局任务 ref 选单：现 @ 触发器要求 input 已聚焦；⌘K 不必先点输入框，直接弹 picker 选任务插入光标。"

PanelChat 既有 @ 提及 picker + ⌘K picker 都挂在 `composeTextareaRef.current.onKeyDown` 上 —— 必须 textarea 处于焦点才能触发。owner 工作流常态：
- 正在读消息区（鼠标 / 键盘焦点在某条 bubble 上）
- 在侧栏 session 列表里寻找
- 顶部 chip filter 区操作

此时按 ⌘K 没反应，得先点 textarea。多一步摩擦。

把 ⌘K 升级为 Panel window 内的"全局热键"：textarea 焦点时走既有 textarea handler，其它位置时走新增的 window-level 监听。

## 改动

### `src/components/panel/PanelChat.tsx`

在 `openTaskPicker` 定义之后追加 useEffect：

```ts
useEffect(() => {
  const onKey = (e: KeyboardEvent) => {
    if (
      !(e.metaKey || e.ctrlKey) ||
      e.shiftKey ||
      e.altKey ||
      e.key.toLowerCase() !== "k"
    ) {
      return;
    }
    // textarea 自带 handler → 这里跳过避免双触发
    if (document.activeElement === composeTextareaRef.current) return;
    // 其它输入控件（session 标题 rename / 搜索框 / contentEditable）也跳过
    const ae = document.activeElement;
    if (
      ae instanceof HTMLInputElement ||
      ae instanceof HTMLTextAreaElement ||
      (ae instanceof HTMLElement && ae.isContentEditable)
    ) {
      return;
    }
    e.preventDefault();
    void openTaskPicker();
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}, [openTaskPicker]);
```

## 关键设计

- **textarea 路径独占优先**：现有 textarea `onKeyDown` 处理 ⌘K 不动 —— 当 textarea 处于焦点时不让 global handler 双触发（picker open 虽幂等但 `preventDefault` 调两次违反事件流单一职责）。
- **跳过任何其它输入框**：除了主 textarea，Panel 还有 session 标题 rename input / 跨会话搜索输入 / 标题筛选 input 等。在这些里按 ⌘K 应该让它们自己接管（如果未来想给搜索框加 ⌘K 清空 query 之类）。HTMLInputElement / HTMLTextAreaElement / contentEditable 三态判断覆盖几乎所有可输入元素。
- **window-level addEventListener**：keydown 在 textarea 内冒泡到 window 顶部，全局监听器能拿到。Tauri 多 webview 下 `window` 是 Panel 独立 webview 的实例，不会与 pet / debug 窗口串扰。
- **依赖 openTaskPicker**：useCallback 已稳定（空 deps），useEffect 重订阅频率近 0 —— 不会因每次 render 重挂监听。
- **不动 ⌘K 抢键时序优先级**：textarea handler 仍在 image 历史 / slash menu / @ mention / Enter 提交等分支之前 —— 那个保护不变。global handler 是补 case："textarea 不在焦点时也想触发" 的兜底。

## 不做

- **不在 textarea handler 里删除 ⌘K 逻辑只保留 global**：textarea 内 `preventDefault` + return 的早返让其它分支不被波及；global 路径里同样 preventDefault 但语义上是"Panel-wide hotkey"，两者角色清晰。
- **不在桌面 ChatPanel.tsx（pet 窗口）加 ⌘K**：桌面 pet 窗口的输入框不持任务 ref 概念 —— 它是流水入口（聊天 + slash + image paste），插任务 ref 应该到 panel 大窗口操作。Tauri 多 webview 下两个窗口独立，桌面 ⌘K 与 panel ⌘K 不冲突。本 iter 专注 Panel window 全局化。
- **不引入 react-hotkeys-hook 等库**：单一快捷键 + ~30 行 useEffect 不值得加 dep。
- **不写测试**：jsdom 下 `document.activeElement` 行为与真 webview 偶有偏差；既有 PanelChat 各种快捷键路径都视觉验证 —— 视觉验证（在消息区 / 侧栏 / chip 区按 ⌘K → picker 弹出）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.23s
- 改动 ~40 行（useEffect 30 + 注释 + comment）；既有 textarea onKeyDown handler / openTaskPicker / @ picker 路径完全不动。

## TODO 状态

empty —— 6 条候选 auto-proposed 全部完成，下次启动 TODO 流程会进入 auto-propose 分支提新需求。

## 后续

- ⌘K 触发时 picker modal 内的 query input 自动 focus（既有逻辑应该已 autoFocus，验证下）。
- 桌面 ChatPanel（pet 窗口）若未来需要 task ref，引入 @ 提及 inline picker 即可（与 PanelChat 共用 helpers）—— 当前 owner 工作流不要求。
- Panel 顶部加 ⌘K hint 角标 / palette icon 让发现性更强。
