# ChatPanel ⌘N 新建会话快捷键

## 背景

TODO 上 auto-proposed 一条："ChatPanel session list ⌘N 新建会话快捷键：替代点 '+' 按钮，与 IDE / 浏览器 ⌘N 直觉一致。"

新建会话当前唯一入口是 session dropdown 底部的 "+ 新会话" 按钮 + slash 命令 `/new`。⌘N 是 IDE / 浏览器最通用的"新建文件 / 标签页"绑定，键盘党在 Panel 中切个新对话 1 步可达。

与既有 ⌘K (打开 task picker) / ⌘B (切上一会话) 同模式：global window listener + textarea onKeyDown 双重接入。

## 改动

### `src/components/panel/PanelChat.tsx`

#### 全局 window 监听

`⌘K` 全局热键之后追加：

```ts
useEffect(() => {
  const onKey = (e: KeyboardEvent) => {
    if (
      !(e.metaKey || e.ctrlKey) ||
      e.shiftKey ||
      e.altKey ||
      e.key.toLowerCase() !== "n"
    ) return;
    const ae = document.activeElement;
    if (
      ae instanceof HTMLInputElement ||
      ae instanceof HTMLTextAreaElement ||
      (ae instanceof HTMLElement && ae.isContentEditable)
    ) return;
    e.preventDefault();
    void handleNewSession();
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
  // handleNewSession 每 render 重建但只读 stable setters + invoke，故空 deps
  // 避免 N 次 re-subscribe。
  // eslint-disable-next-line react-hooks/exhaustive-deps
}, []);
```

#### textarea onKeyDown 分支

`handleInputKeyDown` 内既有 ⌘K / ⌘B 分支之后插入 ⌘N：

```ts
if (
  (e.metaKey || e.ctrlKey) &&
  !e.shiftKey &&
  !e.altKey &&
  e.key.toLowerCase() === "n"
) {
  e.preventDefault();
  void handleNewSession();
  return;
}
```

让 owner 在 textarea 内输入时按 ⌘N 也能触发（与 ⌘K / ⌘B 同 textarea-内独立分支）。

#### 视觉 hint

- "+ 新会话" 按钮 title 加 `(也可按 ⌘N / Ctrl+N)`
- textarea placeholder 加 `；⌘N 新建会话`

让 owner hover button / 看 placeholder 即得知快捷键。

## 关键设计

- **global + textarea 双接入**：与 ⌘K 模式一致。global 覆盖"消息区 / 侧栏 / chip 区"等非输入焦点场景；textarea 内独立分支让事件流明确 + power user 输入时也能直接触发。
- **input focus 让位（global only）**：global 监听检测 `document.activeElement` 是 input / textarea / contentEditable 时跳过，让那些控件优先（即便它们当前不处理 ⌘N，也防意外抢键）。textarea-内分支不需要这个 guard —— 因为是 textarea 自己的 onKeyDown，自身控件即接管。
- **严格 modifier (`!shift && !alt`)**：避免与 ⌘⇧N / ⌘⌥N 等组合冲突（macOS 系统 ⌘⇧N = Finder 新建文件夹等）。
- **`e.preventDefault()`**：吃掉浏览器默认 ⌘N（"新窗口"）。Tauri webview 上 ⌘N 通常不会自动开新窗口（无 Window menu 默认），但 preventDefault 是 future-proof。
- **空 deps + eslint disable**：handleNewSession 是 plain async arrow，每 render 重建。但函数体只调 `invoke` + `setState` setters + `messagesRef`，全是 stable 引用 —— 闭包过时无害。空 deps 避免每次 render 重挂监听。`// eslint-disable-next-line react-hooks/exhaustive-deps` 明示决策。
- **不动 `handleNewSession` 本体**：纯接入，本函数已稳定（被 +button / `/new` slash / 4 处错误处理 path 共用）。

## 不做

- **不写测试**：纯 keydown + invoke，逻辑 ~25 行；既有 ⌘K / ⌘B 同模式无单测。视觉验证（panel 任意位置 ⌘N → 新 session 弹出、底部按钮 hover 见 tooltip）足够。
- **不在 PanelTasks / PanelMemory 等 tab 也绑 ⌘N**：那些 tab 的 "新建" 是新 task / 新 memory item，语义不同；不该共享单一快捷键。tasks 已有 `n` 单键展开新建表单（限 panel 任务列表焦点时）。
- **不接 KeyboardHelpOverlay**：要顺便加到那里，但本 iter 专注接入；同时改两处增加测试面，留下次。
- **不跨窗口生效（pet window 不绑）**：pet window 不需要新建 session（mini chat 只显当前 session 的最近 20 条）。
- **不和 ⌘⇧N 绑"新建特殊 session"**：scope creep；single ⌘N 一个 action 足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.17s
- 改动 ~70 行（全局 useEffect 30 + textarea 分支 12 + tooltip 4 + placeholder 1 + 注释）；既有 handleNewSession / ⌘K / ⌘B / `/new` slash 路径完全不动。

## TODO 状态

6 条 auto-proposed 已完成 2 条，余 4 条留池：
- detail.md 大纲浮窗 active heading 高亮
- detail.md preview hover heading 复制 section 按钮
- 任务详情顶部「📤 导出整体 markdown」按钮
- mini chat ⌘C 复制最近一条

## 后续

- KeyboardHelpOverlay 加 ⌘N 条目 —— 与 ⌘1-⌘5 / ⌘K / ⌘B 等 modifier 快捷键同 cluster。
- ⌘⇧N 触发"新建会话并 fork 当前末尾 N 条"作为延伸 —— 与既有 fork ctx menu 入口对偶。复杂度 +1，等 fork session 用户反馈再做。
