# pet ctx menu「🔄 重启 LLM 连接」（iter #396）

## Background

owner 调试 LLM 卡死场景时希望一键 unstick — 类似既有 reconnect_mcp
（mcp manager shutdown + 重启）的入口但针对 chat backend。

实际架构差异：
- mcp 有持久 connection / spawn 子进程，shutdown + 重启有意义
- chat 走 `reqwest::Client::new()` per-call，无持久 connection
  无法"重连"

所以"reset chat HTTP client"实际语义是 unstick UI in-flight state —
ChatMini 已有 ✕ 取消按钮走 useChat.cancel() soft-cancel，但藏在按
钮里不易发现。本 iter 加 pet ctx menu 入口让 owner 在"pet 卡住"
debug 心智下一键找到。

## Changes

### `src/App.tsx`

#### petCtxMenu 加「🔄 重启 LLM 连接」按钮（紧贴 🔄 重启窗口 之前）

```tsx
<button
  onClick={() => {
    setPetCtxMenu(null);
    cancel();
    appendAssistant(
      "🔄 已重置 LLM 连接 — UI in-flight 状态已清，下次发消息走全新 reqwest client。（注：后端 stream 若已在跑仍会消耗 token；本操作仅救 UI 卡死场景。）",
    );
  }}
  title="一键 unstick UI — soft-cancel 任何 in-flight chat state..."
>
  🔄 重启 LLM 连接
</button>
```

行为：
1. 关闭 ctx menu
2. 调用 `cancel()`（既有 useChat hook）soft-cancel — 翻 cancelledRef
   + finalize 累积的 streaming 文本
3. appendAssistant 推 ack message 到 ChatMini 让 owner 看到生效 +
   说明限制（后端 stream 仍可能跑）

#### H 高度 +30（470 → 500）

加一行按钮 + 余量 → 防菜单越界。

## Key design decisions

- **不加后端 Tauri 命令**：chat 已 per-call new client，无持久 conn
  需 reset。"reset"的实际效果就是 frontend soft-cancel。加一个 stub
  后端 command 仅为命名一致 — 无功能 → 直接复用 cancel() 更诚实。
- **appendAssistant 推 ack 含 limitation 文案**：owner 看到 "UI 已
  清 + 后端 stream 仍可能跑" 知道精确语义，避免错以为"token 已停"。
  与 reconnect_mcp 真重连不同。
- **title attribute 详释架构差异**：chat 走 per-call vs mcp 持久
  conn — 让 owner hover 即明白本菜单的局限性。
- **位置紧贴 🔄 重启窗口**：两 🔄 都是 "unstick" 调试入口（chat /
  window 层）。同图标 + 同分段（最后两条）让心智成组。
- **不引入硬 cancel（cancellation token）**：reqwest stream 真 abort
  需 spawn task + select 抓 abort signal，scope 远超 iter。soft cancel
  已覆盖 owner "UI 卡住" 90% 场景；token 计费区间 LLM 后端通常分钟
  级，owner 等几分钟也能自然恢复。
- **不为单 fn 引 unit test runner**：行为是 IO + cancel() + appendAssistant
  callback；build pass + 手测足够（点 menu 看 ChatMini 是否清 isLoading
  + 显 ack 消息）。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.26s)
- 后端无改动 — soft-cancel 已是既有 useChat 路径
