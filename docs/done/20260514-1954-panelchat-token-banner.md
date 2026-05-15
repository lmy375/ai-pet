# PanelChat 上下文 token 警示 banner + 一键 /reset

## 背景

TODO 上 auto-proposed 一条："PanelChat 顶部上下文 token 提示 chip：与 ChatMini 同款警示信号在 panel 大窗输入框也露出，长 session 写 panel 同样能感知该 /reset。"

近一轮 ChatMini 加了 token > 4000 时的 yellow chip + reset 按钮。但 PanelChat 是 owner 长篇工作（写长 prompt / 多模态 attach / tool 工作流）的主战场 —— 那里 prompt 膨胀比 ChatMini 更显著，却缺同款信号。owner 只能通过 DebugApp 卡片或 /reset 命令显式判断。

把 ChatMini 的同款 chip 平移到 PanelChat 输入栏顶部，关闭跨界面体验差异。

## 改动

### `src/components/panel/PanelChat.tsx`

#### 抽出 `handleResetLlmContext` 共享 helper

原 `case "reset"` 内联 ~30 行实现，chip 与 slash 两条 path 重复用不便。抽 useCallback：

```ts
const handleResetLlmContext = useCallback(async () => {
  if (isLoading) {
    pushLocalAssistantNote("⚠️ 正在流式回复中；先等完成或 Esc 取消，再 /reset。");
    return;
  }
  const sysOnly = messagesRef.current.filter((m) => m?.role === "system");
  if (sysOnly.length === messagesRef.current.length) {
    pushLocalAssistantNote("🧠 LLM 上下文本就是干净的（只剩 system 人设）。");
    return;
  }
  const droppedCount = messagesRef.current.length - sysOnly.length;
  messagesRef.current = sysOnly;
  try {
    await invoke("save_session", { session: { id: sessionId, title: sessionTitle, ..., messages: sysOnly, items } });
  } catch (e) {
    console.error("Failed to save reset session:", e);
  }
  pushLocalAssistantNote(`🧠 已清掉 ${droppedCount} 条 LLM 上下文（保留可见历史 + system 人设）；下一条消息就是干净的 turn 1。`);
}, [isLoading, items, sessionId, sessionTitle, pushLocalAssistantNote]);
```

`case "reset"` 改为：

```ts
case "reset": {
  await handleResetLlmContext();
  break;
}
```

#### sessionTokens polling

```ts
const [sessionTokens, setSessionTokens] = useState<number>(0);
useEffect(() => {
  let alive = true;
  const fetchOnce = async () => {
    try {
      const stats = await invoke<{ tokens: number }>("get_active_session_context_stats");
      if (alive) setSessionTokens(stats.tokens);
    } catch (e) {
      console.error(e);
    }
  };
  void fetchOnce();
  const id = window.setInterval(fetchOnce, 60_000);
  return () => { alive = false; window.clearInterval(id); };
}, []);
```

60 秒一轮，与 ChatMini 同节奏 + 同源信号。手写 useEffect 而非引 usePollingState —— PanelChat 已有大量内联状态，少一个 hook 依赖 mismatch 边界。

#### chip banner 渲染

在 `<form>` Input bar 之前贴顶：

```tsx
{sessionTokens > 4000 && (
  <div style={{ yellow tint banner ... }}>
    <span>💭 上下文 ~{sessionTokens} tok（已超 4000，建议 /reset ...）</span>
    <button onClick={handleResetLlmContext} disabled={isLoading}>/reset</button>
  </div>
)}
```

## 关键设计

- **PanelChat /reset 软 vs ChatMini reset 硬**：PanelChat 保留可见 items 只清 messagesRef（与 TG `/reset` 对偶语义），所以单击即生效**无 armed 二次确认** —— 损失低（用户能看到 chat 历史 + system 人设留着）。ChatMini 是桌面 pet 窗口 messages == display 无 split，reset 也清 mini chat 可见行，所以需要 armed-confirm。
- **流式中 button disabled**：与 slash 命令的"流式拒绝"同保护 —— `messagesRef` 与 stream 在用同一引用，半截清掉行为不可预期。disabled 视觉上变灰 + cursor: default + opacity 0.5 + tooltip 解释。
- **banner 而非 chip**：贴顶 Input bar 横幅形式（满宽 yellow tint），比 ChatMini 的 chip 更显眼 —— PanelChat 用户在写长 prompt 时眼神在输入区附近，banner 自然落在视野焦点。
- **handleResetLlmContext 抽出后 slash 路径与 chip 路径共享**：未来加更多入口（HUD 顶部按钮 / TG 转发 / 快捷键等）都一致行为。DRY + 单源事实。
- **不写 ChatMini-同款 const**：ChatMini 已有 `MINI_TOKEN_WARN_THRESHOLD = 4000`；PanelChat 内联字面量 4000 跨模块 drift 风险低（两处都和 DebugApp `SESSION_TOKEN_WARN_THRESHOLD` 同值 = 4000）。后续若要统一可抽到 shared constants 文件。
- **不引 usePollingState hook**：PanelChat 文件已 6000+ 行 + 大量内联 effect，加 hook 依赖让 cleanup / race 边界更易模糊。手写 useEffect + `alive` flag 是这文件主流模式。

## 不做

- **不重复实现"清空 mini chat 历史"**：那是 ChatMini 责任（架构上 messages == display）。PanelChat 仅清 LLM context，保留 items。两个 reset 语义不同，banner 文案显式说明"保留可见历史"。
- **不动 ChatMini banner / chip 样式**：ChatMini 是 scrollRef 内嵌 chip（跟着消息滚），PanelChat 是 Input bar 顶部 banner（粘底固定）。两套 layout 适配各自上下文 —— 不强求一致。
- **不写测试**：纯 UI poll + 条件渲染 + 复用既有 reset 路径；既有 `case "reset"` 的逻辑已被多轮测试稳态运行；抽 callback 是纯 refactor，行为完全保留。视觉验证（写长 session 看 banner 浮出 → 点 /reset → 验证 messagesRef 清空）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.18s
- 改动 ~110 行（handleResetLlmContext 提取 40 + sessionTokens poll 25 + banner 45 + case "reset" 简化 -30）；既有 slash 命令 / save_session / pushLocalAssistantNote 路径完全不变。

## TODO 状态

6 条候选 auto-proposed 已完成 1 条，余 5 条留池：
- 任务 detail.md 编辑器顶部「📂 在 Finder 打开」按钮
- 跨会话搜索结果按月份分组
- PanelMemory ai_insights 子项「📋 复制全文」按钮
- 桌面 pet 鼠标右键聚合菜单
- 任务详情 detail.md 内嵌 https 链接预览

## 后续

- 阈值动态：基于 `model.max_tokens` 的一定比例（如 50%）替代固定 4000 —— 不同模型阈值差异大。
- banner click 显"reset 会丢 N 条 LLM 消息 / 估省 M tokens" 数据预览 → 让 owner 决策有量化依据。
- 跨界面 reset 统一：PanelChat reset + ChatMini reset 同时调用 useChat.resetContext，让两个 webview state 不再独立。需 cross-webview event bus，复杂度大，等真有 sync 需求再做。
