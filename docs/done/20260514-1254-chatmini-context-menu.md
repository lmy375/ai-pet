# ChatMini 右键菜单 —— 单条消息动作的发现入口

## 背景

TODO（auto-proposed 几轮之前）：

> 桌面 ChatMini 右键菜单：单条复制 / 标 mark / 打开 Panel 定位 — 把现散在双击 / Shift+Click 的入口聚合到一个发现入口。

ChatMini 已经有「📋 复制按钮」「💭 针对这条再问」「双击进入 Panel」等动作，但都是：

- 📋 按钮：hover 才显，新用户压根看不到；
- 💭 按钮：仅 assistant 显，user 行没有；
- 双击进 Panel：纯隐藏交互，文档里有但桌面 UI 里无可见提示；

结果是熟手能享受，但新用户一脸懵。IM 类应用都把"单条消息动作"放在右键菜单（macOS Messages / Telegram / Discord 都这样），既不增加常态视觉负担又给所有用户一条可发现的入口。

## 改动

### `src/components/ChatMini.tsx`

**1. 状态机 + 关闭机制**

```ts
const [ctxMenu, setCtxMenu] = useState<{ idx: number; x: number; y: number } | null>(null);
useEffect(() => {
  if (!ctxMenu) return;
  const onDocClick = () => setCtxMenu(null);
  const onKey = (e) => { if (e.key === "Escape") setCtxMenu(null); };
  window.addEventListener("mousedown", onDocClick);
  window.addEventListener("keydown", onKey);
  return () => { /* cleanup */ };
}, [ctxMenu]);
```

用 mousedown 而非 click 让"按下那一刻立即关菜单"，跟手感。菜单内部 onMouseDown 自身 stopPropagation 防自关。

**2. `.pet-mini-row` 加 onContextMenu**

```tsx
onContextMenu={(e) => {
  e.preventDefault();        // 吃 webview 默认菜单（Tauri 已禁，再守一道）
  e.stopPropagation();        // 防被 wake-up / drag 等 handler 抢走
  setCtxMenu({ idx, x: e.clientX, y: e.clientY });
}}
```

**3. 菜单渲染（fixed 定位，边界夹紧）**

```tsx
{ctxMenu && (() => {
  const m = visibleItems[ctxMenu.idx];
  const text = extractText(m.content);
  // 菜单约 200×180px，超出 viewport 右/下时向左/上挪
  const x = Math.min(ctxMenu.x, vw - 220 - 4);
  const y = Math.min(ctxMenu.y, vh - 180 - 4);
  return (
    <div onMouseDown={stopProp} onClick={stopProp} style={{ position: "fixed", left: x, top: y, ... }}>
      <button>📋 复制本条</button>
      <button>⌚ 复制 · 含时间戳</button>
      {isAssistant && hasText && <button>💭 针对这条再问</button>}
      {onOpenPanel && <button>⛶ 在 Panel 中打开聊天</button>}
    </div>
  );
})()}
```

每个 item 行：transparent 底 / 12px 字、6×12 padding、hover → `var(--pet-color-bg)` 浅染色。

**菜单 items 逐条说明**：

| Item | 行为 | 何时显示 |
|---|---|---|
| 📋 复制本条 | 走既有 `handleBubbleCopy(idx, text)` 同路径 → 1.5s 绿对勾反馈 | 任何 text 非空的 bubble |
| ⌚ 复制 · 含时间戳 | `${formatBubbleTimestamp(m.ts)} ${text}` 一并写剪贴板（与既有"📋 复制最近 N 条"的 copyIncludeTime 模式呼应，但应用到单条） | 同上；ts 无效时 fallback 仅 text |
| 💭 针对这条再问 | dispatch `pet-mini-respond-to` CustomEvent（与既有浮按钮同路径），ChatPanel 监听后把 `关于「...」` 拼到 input | 仅 assistant + 非空 text |
| ⛶ 在 Panel 中打开聊天 | 调 `onOpenPanel()`（与既有双击 bubble 同路径） | 总是（props 提供时） |

## 不做

- **不加"标 mark / pinned"**。ChatMini 目前没有 marked 概念（仅 PanelChat 有 `markedMessages: Set<string>`）。要加得在 ChatMini 引入新 state + 视觉标记 + 持久化 —— 不在本"右键菜单聚合发现"的本意范围内。如果未来想要，可以独立一轮做。
- **不做"打开 Panel + 滚到这条"**。当前的"打开 Panel"已切到 Chat tab；要"滚到 idx" 需要让 Panel 内的 PanelChat 知道目标 visibleItems idx，但 PanelChat 是独立组件、items 来自 session 反序列化，跨进程定位需要协议设计。本轮先把"在 Panel 中打开聊天"做成 onOpenPanel 等价；"定位"留给跨会话搜索 `/search` 那条路径。
- **不挂键盘快捷键打开菜单**。右键菜单的本意就是鼠标手势；要键盘版可以未来用 `m` 之类键，但本轮不做。
- **不写测试**。前端无 vitest；菜单只是 IO 调度 + 现有 handler 复用，逻辑明显。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.16s
- 改动 ~140 行（state 25 + onContextMenu 8 + 菜单 render ~110）；既有 📋 / 💭 浮按钮、双击行为、ts 折叠、search hits 等全部不动。

## 后续

- 加"在 Panel 中打开聊天 + 滚到这条"—— 需要 deeplink 协议扩展 chat-message-jump。
- 加"标 mark 这条"—— 先在 ChatMini 引入持久化 markedMessages（参考 PanelChat 同模式）。
- 加"复制全段（含前后 N 条）"—— 适合"导出一段对话片段"用例。
- 桌面 PanelChat 内也对偶右键菜单（PanelChat bubble 当前没有右键，可参考本实现）。
