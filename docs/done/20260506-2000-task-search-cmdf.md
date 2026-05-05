# 任务面板搜索框 ⌘F 聚焦 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务面板搜索框 ⌘F 聚焦：现状要鼠标点搜索框；多数 mac 用户惯于按 ⌘F；监听全局 keydown，PanelTasks 处于活跃 tab 时拦截 ⌘F → 聚焦搜索框，与系统直觉对齐。

## 目标

PanelTasks 现在搜索框 focus 必须靠鼠标点。mac 用户在浏览器 / Finder /
Notion / VS Code 等几乎所有应用都靠 ⌘F 进搜索；Win / Linux 同语义是 Ctrl
+F。本轮把这个直觉接进任务面板：PanelTasks 处于活跃 tab 时按 ⌘F / Ctrl+F
→ search input 聚焦 + 选中已有内容（方便直接输新词覆盖）。

## 非目标

- 不改其它 panel —— PanelTasks 是搜索功能最完整的那个；其它 panel 的
  搜索框（chat sessions / settings）有各自的 UX 路径，不一刀切。
- 不拦截浏览器原生 Find（Tauri webview 默认无此 UI，但 user 若开 dev
  tools 仍可能触发；preventDefault 主要是吃掉无意义的浏览器行为）。
- 不做 Esc 退出搜索 + 清空 —— Esc 在 input 里默认不做事，加上后会与"按
  Esc 关闭模态 / 取消编辑"的隐式约定冲突。后续需要再补。

## 设计

### ref

`searchInputRef = useRef<HTMLInputElement>(null)`，挂到既有 `<input>` 上。

### 键盘处理

复用现有 PanelTasks 的全局 keydown 监听（在 `useEffect` 已有方向键 / 空格 /
Enter 处理）。在 handler 顶部、tagName 守卫**之前**，加一个 ⌘F / Ctrl+F
分支：

```ts
if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
  e.preventDefault();
  const el = searchInputRef.current;
  if (el) {
    el.focus();
    el.select(); // 选中已有内容，方便直接输新词覆盖
  }
  return;
}
```

放在 tagName 守卫之前的理由：用户在其它 input / textarea 里按 ⌘F 也应该
跳到搜索框（"⌘F 永远找搜索"是 mac 直觉）；如果加守卫会出现"在创建表单
里 ⌘F 没反应"的迷糊 UX。

`e.key.toLowerCase()` 容错 ⌘⇧F / CapsLock 等情况下大写字母路径。

### 视觉提示

不改搜索框 placeholder / 加 hint —— mac 直觉本身够强；hint 反而是噪音。
后续可以加 tooltip "⌘F 聚焦"，但本轮不做。

## 测试

PanelTasks 是 IO 重的容器；前端无 vitest。靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | searchInputRef + ⌘F 分支接进 keydown handler |
| **M2** | tsc + build + cleanup |

## 复用清单

- 既有 keydown useEffect
- 既有 search input element

## 进度日志

- 2026-05-06 20:00 — 创建本文档；准备 M1。
- 2026-05-06 20:10 — M1 完成。`searchInputRef` ref 挂搜索 input；keydown handler 在 tagName 守卫**前**加 ⌘F/Ctrl+F 分支：preventDefault → focus + select。placeholder 文案补 "⌘F 聚焦" hint 防止用户不知道。
- 2026-05-06 20:15 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 925ms)。归档至 done。
