# 桌面 pet 窗口 ⌘O 打开面板全局快捷键

## 背景

TODO 上 auto-proposed 一条："桌面 pet 窗口 ⌘O 打开面板全局快捷键：与 Esc 收起对偶，让键盘党不必鼠标点 '💬 / ⛶ / 右键菜单' 之一打开 panel。"

近一轮 Esc 收起窗口快捷键 ship。Esc / ⌘O 是对偶的"出 / 入"动作 —— Esc 把宠物滑到桌边，⌘O 把面板召出来。当前 3 条打开 panel 路径都是鼠标：
- mini chat ⛶ 按钮
- 输入栏 💬 按钮（旧）/ ChatPanel 顶部按钮
- 右键菜单「📋 打开面板」

键盘党加 ⌘O 即省去鼠标定位。⌘O = "Open" 的 IDE / 浏览器 convention。

## 改动

### `src/App.tsx`

#### 新 useEffect 挂全局 keydown 监听

放在 `openPanel` 定义之后（让 useEffect deps 能引用到）：

```ts
useEffect(() => {
  const onKey = (e: KeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    if (e.shiftKey || e.altKey) return;
    if (e.key.toLowerCase() !== "o") return;
    const ae = document.activeElement;
    if (
      ae instanceof HTMLInputElement ||
      ae instanceof HTMLTextAreaElement ||
      (ae instanceof HTMLElement && ae.isContentEditable)
    ) {
      return;
    }
    e.preventDefault();
    openPanel();
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}, [openPanel]);
```

#### tooltip 更新（discovery）

- 右键菜单「📋 打开面板」加 `title="...— 全局快捷键 ⌘O / Ctrl+O"`
- ChatMini ⛶ 按钮 title 同步追加「— 也可按 ⌘O / Ctrl+O」

让 owner hover 这些入口时能学到快捷键。

## 关键设计

- **不在 collapsed 时禁用**：Esc gate `hidden` 让 collapse 后再 Esc 是 noop（已经在桌边）。⌘O 反向 —— 即便 hidden 也允许触发，让 owner 从桌边宠物直接召 panel。`invoke("open_panel")` 后端命令与 hidden 状态正交。
- **输入控件聚焦时让位**：textarea / input / contentEditable focus 时 ⌘O 可能是 browser default "open file" / 应用内自定义行为，不抢键。与既有 ⌘K 全局任务 picker / Esc 收起同 guard 模式。
- **`!shift && !alt` 严格 modifier**：避免与 ⌘⇧O / ⌘⌥O 等可能的浏览器扩展 / Tauri menu 等其它快捷键冲突。
- **不在 PanelChat / PanelTasks 同款绑**：panel 窗口已经打开了 —— 再 ⌘O 没意义。该热键专属 pet window。
- **`e.preventDefault()`**：吃掉浏览器默认 ⌘O（开文件对话框）。Tauri webview 默认 ⌘O 行为有时弹"打开文件"，preventDefault 防干扰。
- **tooltip 加快捷键 hint**：discovery 路径，owner hover 鼠标按钮就知道有键盘等价。比埋 README 角落或 keyboard help overlay 更早被发现。

## 不做

- **不接 KeyboardHelpOverlay**：那个目前只在 panel 窗口内显，pet 窗口没专属帮助层（仅 Esc / ⌘O 两快捷键，专门加帮助层 over-engineering）。
- **不写测试**：纯 DOM keydown + invoke 一行；既有 Esc 同模式无单测。视觉验证（pet 窗口聚焦 → ⌘O → panel 弹出 → 同既有 mini chat ⛶ 路径）足够。
- **不抢 input 内 ⌘O**：见上 guard 理由。
- **不动 panel window 内 ⌘O**：panel 已开就在那；再热键打开自身是 no-op，没价值。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.18s
- 改动 ~40 行（useEffect 25 + 2 处 tooltip 更新 + 注释）；既有 openPanel / 右键菜单 / ChatMini ⛶ 按钮 / Esc collapse 路径完全不动。

## TODO 状态

6 条 auto-proposed 已完成 1 条，余 5 条留池：
- detail.md 大纲浮窗 active heading 高亮
- ChatPanel session list ⌘N 新建会话快捷键
- detail.md preview hover heading 复制 section 按钮
- 任务详情顶部「📤 导出整体 markdown」按钮
- mini chat ⌘C 复制最近一条

## 后续

- 同款 ⌘\\ 切 collapse / 展开（系统级 toggle）—— Esc 现在只能"收"，没"展开"对偶；⌘O 是"召 panel"非"展开 pet 窗口"。要 toggle pet hidden 需绑独立键 + slideFromEdge 路径。
- 全局快捷键（Tauri global_shortcut）—— pet / panel 都未聚焦时也能 ⌘O 召唤宠物。复杂度大（需注册系统级 hotkey 防与其它 app 冲突），等真有诉求再做。
