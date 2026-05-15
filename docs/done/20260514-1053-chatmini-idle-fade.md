# ChatMini 桌面聊天列表静默淡出 + 多个 TODO 项确认已实现

## 背景

本轮先盘 TODO，发现 6 条里多条其实已上线但没及时清单：

- **任务行右键菜单**：`taskCtxMenu` 在 PanelTasks.tsx ~6825 行已完整实现（展开详情 / 标 done / 标 NOW / due 今日 18:00 / due 明日 09:00 / 重试 / 取消 / 改 priority 子菜单 / 复制标题 / 复制为 ref token）。✅
- **PanelChat `/find`**：`/search` slash command（slashCommands.ts:34）+ 跨会话搜索面板 + 命中跳转高亮已上线，等价覆盖。✅
- **PanelTasks 多选批量操作**：PanelTasks.tsx 3792+ 的 bulkBar 已支持批量 重试 / 标 done / 取消 / 改优先级 / 改 due。✅
- **记忆列表行 hover 浮预览**：上一轮已确认实现（`startPreviewHover` + `memory_read_detail` IPC + cache）。✅

本轮真正实现的是剩下唯一新功能：**ChatMini 桌面气泡 idle 淡出**。

## 改动

### `src/components/ChatMini.tsx`

新增 idleFaded 状态机：

- `idleFaded: boolean` —— true 时整段聊天列表透明度降到 0.45（透出后面的 Live2D，桌面更干净）。
- 60 秒无活动则 fade。"活动"包含：新消息追加 (`messages.length` 变化)、streaming chunk (`currentResponse` 变化)、tool 状态变化、`isLoading` 跳变 —— 这四个 dep 一起监听让 useEffect 一处兜底，不漏不冗余。
- 鼠标进入或移动立即 wake 回满：`onMouseEnter` 总是 wake；`onMouseMove` 仅在 idleFaded 时才 wake（避免每帧无脑 setState，hover 时 React 还在跑 reconciliation 浪费）。
- `transition: opacity 600ms ease-out` —— 600ms 是足够觉察"哦它在淡入/淡出"又不打扰阅读的范围。
- `localStorage` 旁路：`pet-chatmini-idle-fade = "off"` 关闭整个特性（嫌烦用户随时退出）。`useMemo` 一次读取，运行时不动；改了刷新窗口生效。

实现要点：

```ts
const [idleFaded, setIdleFaded] = useState(false);
const idleFadeTimerRef = useRef<number | null>(null);
const idleFadeEnabled = useMemo(() => {
  try { return localStorage.getItem("pet-chatmini-idle-fade") !== "off"; }
  catch { return true; }
}, []);
const scheduleIdleFade = () => {
  if (!idleFadeEnabled) return;
  if (idleFadeTimerRef.current !== null) clearTimeout(idleFadeTimerRef.current);
  idleFadeTimerRef.current = setTimeout(() => {
    setIdleFaded(true);
    idleFadeTimerRef.current = null;
  }, 60_000);
};
const wakeIdleFade = () => {
  setIdleFaded(false);
  scheduleIdleFade();
};
useEffect(() => {
  wakeIdleFade();
  return () => { /* cleanup timer */ };
}, [messages.length, currentResponse, toolStatus, isLoading]);
```

容器：

```tsx
<div
  onMouseEnter={wakeIdleFade}
  onMouseMove={idleFaded ? wakeIdleFade : undefined}
  style={{
    flex: 1, position: "relative", padding: "8px 12px 0", minHeight: 0,
    opacity: idleFaded ? 0.45 : 1,
    transition: "opacity 600ms ease-out",
  }}
>
```

### `docs/TODO.md`

- 删 4 条 stale 项（右键菜单 / `/find` / 批量操作 / hover 预览 —— 都已实现）。
- 保留 1 条：聊天 user 消息编辑/重发（未实现，下一轮候选）。

### `README.md`

第 1 节"被动聊天"加亮点：ChatMini 60s 无活动自动淡到 45%，hover 立即回满，Live2D 成桌面焦点。

## 不做

- **不暴露 settings UI**。localStorage 旁路足够给"嫌烦"用户出口；加 settings 切换会污染 PanelSettings 视觉。等用户反馈再加。
- **不动 PanelChat 大聊天框**。panel 是用户主动专注阅读时打开的，idle fade 反而会干扰。仅 ChatMini（桌面常驻气泡）受影响。
- **不联动 mute / 安静时段**。语义不同：mute 是阻止主动开口；idle fade 是显示层的"放置态"暗化。两者独立。
- **不动 60s 阈值**。经验值，60s 长到不会在用户读消息时偷偷淡掉，短到"放置一会儿"就生效。需要时常量在源码顶可调。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.16s

## 后续候选

TODO.md 现剩：

- 聊天 user 消息编辑/重发：双击历史 user bubble 进 inline 编辑，Enter 后丢弃后续 messages 重新生成。
