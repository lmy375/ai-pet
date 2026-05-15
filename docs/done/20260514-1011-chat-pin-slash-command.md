# PanelChat `/pin` 命令 — 钉住 / 取消钉住当前会话

## 背景

会话 dropdown 行有 📌 钉/取消钉按钮，但只能从 dropdown 里点。聊到一半想"这条以后我会经常回来"得切下拉、找当前会话、点钉。`/pin` 让一行命令搞定。

## 改动

### `src/components/panel/slashCommands.ts`

- `{ name: "pin", description: "钉住 / 取消钉住当前会话（toggle）", parametric: false }`
- `SlashAction` 加 `{ kind: "pin" }`
- `parseSlashCommand` `case "pin"`：无参，返回 `{ kind: "pin" }`

### `src/components/panel/PanelChat.tsx`

`executeSlash` `case "pin"`：

```ts
const cur = sessionList.find((s) => s.id === sessionId);
const wasPinned = !!cur?.pinned;
await invoke("set_session_pinned", { id: sessionId, pinned: !wasPinned });
const idx = await invoke<SessionIndex>("list_sessions");
setSessionList(idx.sessions);
pushLocalAssistantNote(wasPinned ? "📌 已取消钉住本会话" : "📌 已钉住本会话");
```

走与 `handleTogglePinned` 同 backend 路径，sessionList 由 list_sessions 拉真值避免 race。

## 不做

- 不直接调 handleTogglePinned 复用：那是个独立 helper，slash 直接 invoke + refresh 是等价 / 更短
- 不分 "/pin" 与 "/unpin"：toggle 单命令 + 反馈文案表达当前结果，比两个命令简洁

## 验收

- `npx tsc --noEmit` ✅
- 当前会话未钉时 `/pin` → 反馈 "📌 已钉住本会话"，session list 这条排到最上
- 再次 `/pin` → "📌 已取消钉住本会话"

## 完成

- [x] slashCommands.ts: 注册 + parser case
- [x] PanelChat.tsx: executeSlash case
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
