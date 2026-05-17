# ChatMini bubble 右键「↺ 重发本条」(iter #426)

## Background

owner 想 reroll user message（"上次 pet 回复不够好，再试一次同样的
问题"）当前要：
1. ChatMini 右键消息 → 📋 复制
2. ⌘+L focus ChatPanel textarea
3. ⌘+V 粘贴
4. ⌘+Enter send

四步。本 iter 加 ChatMini ctx menu「↺ 重发本条」item：一键
dispatch event 让 ChatPanel 直接 onSend(text) — 跳过 textarea state
中转 + 不污染当前输入。

complement 已存 ChatMini ctx menu「💭 针对这条再问」(prefill only，
不自动 send，让 owner 修问题再发) — 本入口是「原文 reroll」。

## Changes

### `src/components/ChatMini.tsx`（ctx menu，紧贴 💭 之后）

```tsx
{!isAssistant && hasText && (
  <button
    onClick={() => {
      setCtxMenu(null);
      window.dispatchEvent(
        new CustomEvent("pet-mini-resend-message", { detail: text }),
      );
    }}
  >
    ↺ 重发本条
  </button>
)}
```

设计：
- **gate by `!isAssistant`**：assistant reply 重发无语义（owner 不能
  让 pet「再说一次自己刚说的话」）；仅 user message 显
- **hasText guard**：纯图 bubble 无文本可重发
- **dispatchEvent 跨 component 通讯**：与既有
  `pet-mini-respond-to` / `pet-mini-rewrite-selection` 同 channel
  pattern（ChatMini 与 ChatPanel 在同 window；event 走 window 即可）

### `src/components/ChatPanel.tsx`（listener，紧贴 onRewriteSel 之前）

```ts
useEffect(() => {
  const onResend = (e: Event) => {
    const ce = e as CustomEvent<string>;
    const text = ce.detail;
    if (typeof text !== "string" || !text.trim()) return;
    if (isLoading) return;
    onSend(text.trim(), undefined);
  };
  window.addEventListener("pet-mini-resend-message", onResend);
  return () => window.removeEventListener("pet-mini-resend-message", onResend);
}, [onSend, isLoading]);
```

设计要点：
- **直接 onSend 而非通过 textarea**：跳过 setInput / submit() —
  reroll 应该「立即触发」不污染 owner 当前正在敲的草稿（如果有的话）
- **isLoading 守门**：streaming 中拒 — 防并发 LLM 调用
- **不附 images**：原 send 时 stage 已消费；re-send 无新 stage 该
  无图（与既有「💾 转 task」/「📝 记到 note」等其它单 text 入口
  同语义）
- **不 setSentHistory**：text 在原次 send 时已入 history，dedup
  逻辑会让重复 push 无效 — 但也不强制 push 避免不必要重排
- **不调多模态守门**：原次 send 已通过守门；reroll 同请求语义上
  与之等价不需要二次确认（且 onSend 内部也会再次守门）

## Key design decisions

- **新 event 而非复用 pet-mini-respond-to**：那个语义是「带 prefix
  prefill」让 owner 编辑后再 send；本 event 是「直接 send 原文」—
  两动作不同
- **不为 assistant 提供「↺」**：reroll = 重复 prompt 让 LLM 再
  生成；只对 prompt（user message）有意义。assistant 显「↺」会
  混淆为「让 pet 重复自己的话」，没用
- **不为单 event 引 unit test**：纯 dispatchEvent + onSend；event
  pattern 既有覆盖；手测足够（右键 user bubble → 看 ↺ 项出 →
  click → 看 pet 立即开始 reroll reply；右键 assistant bubble →
  看 ↺ 不显）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动 — 复用 ChatPanel.onSend 既有 channel
