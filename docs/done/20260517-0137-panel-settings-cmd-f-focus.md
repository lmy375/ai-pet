# PanelSettings 顶 search input 加 ⌘F 全局聚焦快捷 + 上轮 TODO stale 替换

## 背景

### 上轮 TODO "PanelSettings 顶部加 search input" stale 移除

上轮 auto-propose 的 "PanelSettings 顶部加 search input" 是 stale —— grep 发现 PanelSettings.tsx line 877+ 已经有搜索输入框 + matchSection / HighlightedText / SearchableSection / TOC 全套搜索基础设施（含 Esc 清空 / 红字高亮 / 11 个 keywords 注册的 section）。移除该项，替换为 6 条新提案。

教训：auto-propose 阶段必须 grep。

### 本 iter 实现：⌘F 全局聚焦快捷

既有 search input 缺失 owner 心智上自然的 ⌘F 快捷 —— "在任何 panel 内按 ⌘F 应聚焦 search 框"。本 iter 补一个全局 keydown handler 让 PanelSettings 内任意位置 ⌘F → 聚焦 search input + select 既有 query 串方便覆盖输入。

## 改动

### `src/components/panel/PanelSettings.tsx`

#### 1. 新 ref + useEffect 监听 ⌘F

```ts
const settingsSearchInputRef = useRef<HTMLInputElement>(null);
useEffect(() => {
  const onKey = (e: KeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    if (e.shiftKey || e.altKey) return;
    if (e.key !== "f" && e.key !== "F") return;
    if (!settingsSearchInputRef.current) return;  // raw YAML 模式无 input
    e.preventDefault();
    settingsSearchInputRef.current.focus();
    settingsSearchInputRef.current.select();  // select 既有内容覆盖输入
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}, []);
```

#### 2. ref 绑到既有 search input

```tsx
<input
  ref={settingsSearchInputRef}
  type="text"
  value={searchQuery}
  onChange={...}
  placeholder="搜索设置（按标题或关键字过滤；如 api / mute / regex / 工具）· ⌘F 聚焦"
  ...
/>
```

placeholder 末尾补 "· ⌘F 聚焦" hint 让 owner hover input 发现快捷。

## 关键设计

- **过滤 ⇧/⌥ modifier**：避免与 macOS 系统级 ⌘⇧F / ⌘⌥F 等冲突。仅纯 ⌘F / Ctrl+F 触发。
- **`!settingsSearchInputRef.current` 兜底 raw YAML 模式**：raw 模式 input 未 mount，ref 为 null —— 让原生 ⌘F 浏览器查找走（不抢用）。
- **focus + select**：select 既有 query string 让 owner 按 ⌘F 后直接输入即覆盖（与 IDE / 浏览器 search bar 同 UX）。
- **window keydown 全局监听**：PanelSettings 是单 webview / 单挂载组件；window-level handler 简单。组件 unmount 时 cleanup。
- **placeholder hint**：让发现快捷键不靠文档。

## 不做

- **不绑 / 键盘快捷（vim-style）**：⌘F 是更通用 muscle memory。
- **不在 raw YAML 模式做 fallback ⌘F**：raw 模式没 search 概念；浏览器原生 ⌘F 也未必有用，但保持不干预安全。
- **不写测试**：纯 window keydown handler + ref focus；视觉验证（开 PanelSettings → 按 ⌘F 任意位置 → search input 应被聚焦 + 既有 query 全选）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.19s
- 改动 ~25 行（ref + useEffect 20 + ref 绑定 1 + placeholder hint 1 + 注释）。既有 matchSection / HighlightedText / SearchableSection / 11 keywords 注册 / Esc 清空 / 清空按钮 路径完全不动。

## TODO 状态

剩 5 条留池：
- PanelMemory item 描述行级 hover preview 含完整内容
- ChatPanel 输入框历史栈 hover 显 idx / total
- detail.md 编辑器 ⌘K 唤起 task quick-find palette
- pet 区双击 happy motion 后随机播鼓励 line
- PanelTasks 列表行 hover idx / total 角标

## 后续

- ⌘F 在 PanelTasks / PanelMemory / PanelChat 各面板的 search 入口也补统一 hint（hover 时 placeholder 含 ⌘F）—— 但 PanelTasks 已有 ⌘F (line 510 in ChatMini precedent)，可独立检查。
- ⌘F 后按 Esc 第一次清 query，第二次 blur input —— 给 keyboard owner 两段退出。
