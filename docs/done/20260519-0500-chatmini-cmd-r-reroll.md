# ChatMini 加「⌘R 重发最后一条 user」keyboard shortcut（iter #534）

## Background

ChatMini bubble 右键 ctx menu 已有 `↺ 重发本条` item（line 3392） — 在
具体 user bubble 右键 → 选「重发」。但最常见的场景是「上一句不满意，
让 pet 再 reply 一次」— 不需要先找具体 bubble。键盘 ⌘R reroll 让一键
完成。

但 ⌘R 是 browser / Tauri webview 默认 reload 整页 — 直接触发会丢
session state。需 preventDefault 拦截 + capture phase 拦在其它 handler
之前。

## Changes

### `src/components/ChatMini.tsx`

新 useEffect window keydown listener（紧贴 ⌘C copy-recent-1 监听之后）：

```tsx
const messagesRef = useRef(messages);
useEffect(() => {
  messagesRef.current = messages;
}, [messages]);

useEffect(() => {
  if (!visible) return;
  const handler = (e: KeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    if (e.shiftKey || e.altKey) return;
    if (e.key.toLowerCase() !== "r") return;
    // textarea / input / contentEditable focus → 放过（让 owner 在
    // ChatPanel input 写 prompt 时 ⌘R 仍能 reload — 防御过度反而扰）
    const ae = document.activeElement;
    if (ae instanceof HTMLInputElement || ae instanceof HTMLTextAreaElement
      || (ae instanceof HTMLElement && ae.isContentEditable)) {
      return;
    }
    // 找最后一条 user message（从末向前扫）
    const msgs = messagesRef.current;
    let lastUserText = "";
    for (let i = msgs.length - 1; i >= 0; i--) {
      const m = msgs[i];
      if (m.role !== "user") continue;
      const t = extractText(m.content).trim();
      if (t.length === 0) continue;
      lastUserText = t;
      break;
    }
    if (!lastUserText) return;
    // 拦截 reload + 触发 resend
    e.preventDefault();
    e.stopImmediatePropagation();
    window.dispatchEvent(
      new CustomEvent("pet-mini-resend-message", { detail: lastUserText }),
    );
  };
  // capture phase 让本 handler 先于其它 listener / 默认 reload 跑
  window.addEventListener("keydown", handler, { capture: true });
  return () => window.removeEventListener("keydown", handler, { capture: true });
}, [visible]);
```

复用既有 `pet-mini-resend-message` CustomEvent — 与 ctx menu 「↺ 重发
本条」同 ChatPanel listener / 同 onSend 路径。

## Key design decisions

- **复用 `pet-mini-resend-message` 事件 channel**：与 ctx menu 「↺ 重
  发本条」同后端（ChatPanel listener → 跳过 textarea 直接 onSend）。
  resend / reroll 两路径行为一致 — owner 心智复用
- **`messagesRef` 防 stale closure**：keydown handler 依赖只是 `visible`
  防 mount/unmount 反复绑；messages 通过 ref 拿 latest 避免每次
  messages 变都 re-bind listener
- **capture phase + stopImmediatePropagation**：⌘R 是 OS 级 reload；
  capture phase 让本 handler 先跑 + stop 不让 default + 其它 listener
  劫持
- **textarea / input focus 放过**：ChatPanel input 写 prompt 时 ⌘R 还
  是想 reload（owner 习惯）— 防御过度反而扰。owner 不会在 input 内
  reroll；空 chat scroll / pet body 才是本 shortcut 主战场
- **找 last user message 而非整 last**：owner 想 "重发上句"，不是
  "让 pet 再说一次"（assistant resend 无语义）。从末向前扫第一条 user
  message
- **`lastUserText === ""` 不拦**：若 chat 内无 user message（极端：刚
  起 session 啥都没发）— 让 reload 自然走（无东西可 reroll，reload 反
  而正常）
- **不写 unit test**：纯 DOM event handler + ref 读 props + dispatch
  CustomEvent；逻辑 trivial（既有 ⌘C copy-recent-1 / ctx menu resend
  同 pattern production 验证）。GOAL.md "meaningful tests only" 规则下
  不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.33s)
- 后端无改动 — 复用既有 resend event channel
- 手测：
  - ChatMini visible + chat 有 user message → ⌘R → pet 用上次 prompt
    重新生成 reply（不 reload）
  - ChatMini visible + 无 user message → ⌘R → 默认 reload 走（与 owner
    预期一致）
  - 焦点在 ChatPanel input → ⌘R → reload 走（textarea gate 验）
  - ChatMini 不 visible（pet hidden）→ ⌘R → reload 走（visible gate 验）

## Future iters (out of scope)

- 「⌘⇧R force-reroll」忽略 hot-cache 重新调 LLM — 当前 pet 内部已 fresh
  调（每次 onSend 都重 LLM call）
- 「⌘R 多次按 = 选更早的 user message」cycle — 当前仅取最后一条；
  owner 想重发更早走 ctx menu 右键 「↺ 重发本条」
- 「⌘R confirm modal」防误触 — owner 习惯了 keyboard fast path 不希望
  confirm；如真误触可走 PanelTasks `↺` 既有撤销路径
