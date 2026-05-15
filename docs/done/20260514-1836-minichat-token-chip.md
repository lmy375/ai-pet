# 桌面 ChatMini 顶部「上下文 token 提示 + /reset」chip

## 背景

TODO 上 auto-proposed 一条："mini chat 顶部「上下文 token 提示 chip」：当 messagesRef 累积 tokens > 4000 时桌面 mini chat 浮一个轻量 chip + 一键 /reset 入口（与 DebugApp 同源信号）。"

DebugApp 统计 tab 已有「当前会话 LLM 上下文」卡片显累计 messages / chars / tokens，并在 > 4000 时变 yellow tint 提示用户考虑 /reset。但 DebugApp 是隐藏窗口，owner 平时看不到 —— context 膨胀真的发生时（聊 50+ 轮 / dense 工具调用 session），owner 没有信号。

桌面 ChatMini 是 owner 日常视野焦点；把同一信号嵌进 ChatMini 顶部，超阈值时浮 chip 让 owner 看到 + 一键 reset 解决，让 token 控制成为 ambient 反馈。

## 改动

### `src/hooks/useChat.ts`

新 `resetContext()` 暴露给 App.tsx —— 桌面 mini chat 与 PanelChat 不同：messages 即 display 即 LLM context（无 split），所以本函数同时清两者并立即 save_session 保重启一致。系统提示词保留。

```ts
const resetContext = useCallback(() => {
  const systemOnly: ChatMessage[] = [{ role: "system", content: prevPrompt.current }];
  setMessages(systemOnly);
  setCurrentResponse("");
  setToolStatus("");
  itemsRef.current = [];
  accumulatedRef.current = "";
  updatedMessagesRef.current = [];
  void saveSession(systemOnly, []);
}, [saveSession]);
```

### `src/App.tsx`

#### usePollingState 拉 session 上下文 tokens

```ts
const { data: sessionTokens } = usePollingState(
  async () => {
    try {
      const stats = await invoke<{ tokens: number }>("get_active_session_context_stats");
      return stats.tokens;
    } catch (e) {
      console.error(e);
      return 0;
    }
  },
  60_000,  // 60s 轮一次：与 user 聊天节奏匹配；更频浪费 IPC，更稀疏漏报
  0,
);
```

复用既有 `get_active_session_context_stats` Tauri 命令（DebugApp 同源）。

#### 传给 ChatMini

```tsx
<ChatMini ... sessionTokens={sessionTokens} onResetContext={resetContext} />
```

### `src/components/ChatMini.tsx`

#### Props 扩展

```ts
sessionTokens?: number;
onResetContext?: () => void;
```

两个都 optional —— 不传则 chip 自然不显（与既有 `nowTasks` / `userGlyph` 等 optional 模式一致）。

#### 阈值常量

```ts
const MINI_TOKEN_WARN_THRESHOLD = 4000;
```

与 DebugApp `SESSION_TOKEN_WARN_THRESHOLD` 同值 —— 单一阈值让两处警示触发条件一致。

#### chip 渲染（scrollRef 容器内、消息列表上方）

```tsx
{sessionTokens !== undefined && sessionTokens > MINI_TOKEN_WARN_THRESHOLD && (
  <div style={{ yellow tint chip ... }}>
    💭 上下文 ~{sessionTokens} tok（已超 4000，建议 /reset）
    <button onClick={...}>{resetArmed ? "再点确认 (3s)" : "/reset"}</button>
  </div>
)}
```

#### armed-confirm 模式

```ts
const [resetArmed, setResetArmed] = useState(false);
useEffect(() => {
  if (!resetArmed) return;
  const id = setTimeout(() => setResetArmed(false), 3000);
  return () => clearTimeout(id);
}, [resetArmed]);
```

第一点击 → armed（变红 "再点确认 (3s)"）→ 3s 内再点 → 执行 reset / 不点 → auto 收回 idle。与桌面 ChatPanel 顶部「清空」按钮、任务面板「清结束」按钮同 UX 模式。

## 关键设计

- **同源 + 同阈值**：DebugApp 卡片 + ChatMini chip 都用 `get_active_session_context_stats` + `4000` 阈值，让两处警示同步触发。owner 一处看到 / 一处采取，体感连贯。
- **chip 嵌在 scrollRef 容器内**：滚动时跟随消息一起滚（不抢顶部固定栏）—— 因为这是"当前 session"的状态信号，而非全局 nav。滚到一半看见 chip = "context 还在膨胀，先处理一下"。
- **armed-confirm 防误触**：reset 不可逆 —— mini chat 历史 + LLM context 一起清。armed 状态变红 + 3s 自清防止"点错 button 立刻丢历史" 的情感损失。系统提示词保留让宠物 persona 不变。
- **yellow tint 配色**：与 DebugApp 卡片 > 4000 时同色族（warning 不是 error；用户没做错事，仅 context 累积到该 reset 程度）。chip 视觉 不抢 既有 mini chat 灰白底 + accent 元素的层级。
- **桌面 reset 同时清 items + messages**：架构上桌面无 LLM/display split（与 PanelChat 不同），半截清理会让 mini chat 与 LLM 状态不一致。完全清是最干净的"开新章" 语义。
- **不动 PanelChat 路径**：PanelChat 有自己的 `/reset` slash + items vs messages split，独立处理。本 iter 仅补桌面入口。
- **localStorage / settings 不持久 chip 阈值**：4000 是与后端 / DebugApp 同源常量，统一调整应在 DebugStats 里改后跟 ChatMini 同步。让用户自调阈值反而引入 "三处不同值" 的 drift 风险。

## 不做

- **不显当前 messages / chars 细分**：chip 只显 tokens —— 用户决策的"该不该 reset" 只看 tokens 就够，细分留给 DebugApp。chip 字符长度小。
- **不在 chip 上加"延后 24h" / "本 session 不再提醒"**：阈值的意义是"该 reset 了"，临时禁用反而让 token 失控积累。用户真烦可以一次 reset 让 chip 自然消失。
- **不写测试**：纯 UI 阈值条件 + onClick callback；vitest 下 setTimeout / setState 异步路径覆盖率有限，视觉验证（输够多消息 → chip 浮现 → 双击重置）足够。
- **不在 PanelChat 也加同一 chip**：PanelChat 已有 `/reset` slash 命令 + DebugApp 卡片；再加 chip 信噪比下降。桌面 mini chat 是 owner 平时唯一长时间盯的视口，唯一需要 ambient chip 的场景。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.29s
- 改动 ~150 行（useChat 25 + App.tsx 25 + ChatMini props 18 + chip 渲染 75 + armed-confirm 8）；既有 useChat 路径、ChatMini 渲染顺序、PanelChat 不变。

## TODO 状态

5 条候选 auto-proposed 已完成 5 条（其中 1 条 stale 移除），余 1 条留池：
- PanelChat 顶部「📌 钉住会话」 chip 计数

## 后续

- chip dismiss "本 chat 不再提醒"：让重 context 工作流（如 code review 大段贴）可以暂时禁 chip。当前阈值固定 = 没有这选项，等需求出现再做。
- 接 `/reset` 按钮上挂 hover preview 显"reset 会丢掉 X 条消息 + 估省 Y tokens" —— 让用户决策时有具体数据。
- 阈值动态：基于实际 LLM context 长度（model.max_tokens 的一定比例）而非固定 4000；不同模型阈值差异大。
