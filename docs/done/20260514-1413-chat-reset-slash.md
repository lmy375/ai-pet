# PanelChat `/reset` slash command — 软清空 LLM 上下文

## 背景

TODO 最后一项（auto-proposed 几轮之前）：

> 聊天 `/reset` slash 命令：与 TG `/reset` 对偶，桌面端也能一键清 LLM 上下文（保留 session）。

`/clear` 已存在：清空当前 session 的 items + messages = "硬清空"，session 文件保留但内容全无（用户回看像新建会话）。

TG `/reset` 的语义不同：只清掉 LLM 看的 messages（保留 role=="system"），TG 本身没"items"概念所以效果等于"context 重置"。

桌面有 items 这个独立层，所以 `/reset` 有空间做"软清空" —— 仅截断 messagesRef.current 到 system-only，items / 可见 bubble 一字不动。用例：

- 长 backlog session（30 条历史）想跟宠物开新话题，怕 LLM 被上下文带偏；又不想丢可见历史的 reference value。
- 调试 prompt 时想验证"宠物在干净 context 下会怎么回 X 问题"，但当前 session 已积累其它讨论。

## 改动（frontend only）

### `src/components/panel/slashCommands.ts`

```ts
{ name: "reset", description: "清掉 LLM 上下文但保留可见历史（与 TG /reset 对偶）", parametric: false }
```

SlashAction 加 `{ kind: "reset" }`；parser switch 加 `case "reset"`。位置紧贴 `clear` —— 两者语义对偶，文档与肌肉记忆并排。

### `src/components/panel/PanelChat.tsx`

`executeSlash` switch 新增 `case "reset"`，紧跟 `case "clear"` 之后：

```ts
case "reset": {
  if (isLoading) {
    pushLocalAssistantNote("⚠️ 正在流式回复中；先等完成或 Esc 取消，再 /reset。");
    break;
  }
  const sysOnly = messagesRef.current.filter((m) => m?.role === "system");
  if (sysOnly.length === messagesRef.current.length) {
    pushLocalAssistantNote("🧠 LLM 上下文本就是干净的（只剩 system 人设）。");
    break;
  }
  const droppedCount = messagesRef.current.length - sysOnly.length;
  messagesRef.current = sysOnly;
  // setItems 不动 —— 可见历史保留
  await invoke("save_session", {
    session: {
      id: sessionId, title: sessionTitle, created_at: "",
      updated_at: new Date().toISOString(),
      messages: sysOnly, items,
    },
  }).catch((e) => console.error("Failed to save reset session:", e));
  pushLocalAssistantNote(
    `🧠 已清掉 ${droppedCount} 条 LLM 上下文（保留可见历史 + system 人设）；下一条消息就是干净的 turn 1。`,
  );
  break;
}
```

**与 `/clear` 的关键差异**：

| 字段 | `/clear` | `/reset` |
|---|---|---|
| `messagesRef.current` | `[sysMsg]` 重建 | `.filter(r => r === "system")` 截留 |
| `items` | `setItems([])` | 不动 |
| `currentResponse / currentToolCalls` | 清空 | 不动（应已空） |
| 二次确认 | 5s armed | 单击生效 |
| 流式中 | 不特别拒绝（armed 兜门） | 显式拒绝（避免截 invoke 引用） |
| 反馈 | 无（视觉直接见空） | "🧠 已清掉 N 条" 文字 |

**为什么不要二次确认**：`/clear` armed 因为视觉历史会立即消失（不可恢复），用户容易误手。`/reset` 仅影响 LLM 看的 messages，可见 items 完好；要"恢复"上下文用户只需把背景再说一遍。损失低 → 无须 armed。

**为什么流式中拒绝**：`invoke("chat", { messages: messagesRef.current, ... })` 把 messagesRef 引用直接传给后端 stream pipeline；半途砍掉这个引用行为不可预期（最坏：后端读到 partially-mutated 数组）。先 Esc 取消（清理 isLoading + currentResponse）再 /reset 是规范路径。

**Sentinel 反馈**：dropped 计数让用户感知"砍了多少"。如果 messagesRef 已经是 system-only（罕见但可能 — 刚 /clear 完又 /reset），给一句温和兜底而非沉默无反馈。

## 不做

- **不动 ChatPanel（桌面 mini chat）**。ChatPanel 是 input + 显示，不持有独立 messages 状态（共享 useChat 的 messagesRef）。要在桌面 chat 输入框敲 `/reset` 也走同一逻辑，但 ChatMini 不解析 slash 命令（slash 仅 PanelChat 主聊天框支持）。如要桌面也敲 /reset 工作，得改 ChatPanel 输入路由 —— 独立改动。
- **不更新 SOUL.md / persona_summary**。`/reset` 的语义是清 conversation context，不是清宠物自我画像。后者要走 /clearstats 或 Persona panel 的"清画像"按钮（如有）。
- **不写测试**。前端无 vitest；逻辑是 filter + invoke save_session，与 /clear 同模式（已验证）。
- **不在 keyboard help overlay 文档化**。/help slash 列表里已自动列出（slashCommands.ts 是单一来源）；keyboard help 是按键速查，不是命令清单。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.17s
- 改动 ~45 行（slashCommands 3 + handler 35 + README 1）；既有 /clear / /tasks / /sleep 等 slash 路径全部不动。

## 后续

- ChatPanel 桌面输入框接 /reset（与桌面已有 /done / /cancel / /retry / /stats 等同模式接入）。
- 加一句 PanelDebug 卡片显"当前 session 的 LLM context 长度（消息数 + 估算 tokens）"，与本命令配合 —— 看到太长时 /reset。
- `/reset --to N` 形态保留最近 N 条而非全清（用户场景："只想丢早期闲聊、保最近 5 条"）。

## TODO 状态

empty —— 下次启动 TODO 流程会进入 auto-propose 分支提新需求。
