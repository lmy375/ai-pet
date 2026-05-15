# PanelChat `/title <new>` 命令 — inline 改当前会话标题

## 背景

会话改名只能通过 session 下拉行的双击-input-Enter 三步。在聊天主流中改名（如"刚聊了 5 句发现这其实是个 deep-debug 话题"）需要切下拉、找到当前 session 行、双击。`/title <new>` 让一行命令解决。

## 改动

### `src/components/panel/slashCommands.ts`

- 加 `{ name: "title", description: "改当前会话标题：/title <新标题>", parametric: true }`
- `SlashAction` 加 `{ kind: "title"; query: string }`
- `parseSlashCommand` `case "title"`：空 arg → unknown；否则 `{ kind: "title", query: arg }`

### `src/components/panel/PanelChat.tsx`

`executeSlash` `case "title"`：

```ts
const newTitle = action.query.trim();
if (!newTitle) { pushLocalAssistantNote("⚠️ 用法：/title <新标题>"); break; }
try {
  const session = await invoke<Session>("load_session", { id: sessionId });
  session.title = newTitle;
  await invoke("save_session", { session });
  const idx = await invoke<SessionIndex>("list_sessions");
  setSessionList(idx.sessions);
  setSessionTitle(newTitle);
  pushLocalAssistantNote(`📝 已改名为「${newTitle}」`);
} catch (e) {
  pushLocalAssistantNote(`/title 失败：${e}`);
}
```

与既有 commitRename 同 IO 流程（load → 改 → save → refresh list → 更新本地 sessionTitle），不走 renamingId state（slash 是 inline 一行命令，不需要 inline editor）。

## 不做

- 不暂存撤销：rename 是即时操作；用户改错重发 `/title <旧名>` 即可
- 不与 auto-title-from-first-message 逻辑冲突：那条只在 `sessionTitle === "新会话"` 时触发；用户 `/title` 后 sessionTitle 已 custom，auto-title 不再覆盖
- 不写测试：纯 frontend，项目无 vitest

## 验收

- `npx tsc --noEmit` ✅
- 聊天敲 `/title 今天的代码评审` → 反馈 "📝 已改名为「今天的代码评审」"
- session 下拉 + 顶部 title 实时同步
- `/title` 空参 → 用法提示

## 完成

- [x] slashCommands.ts: 注册 + parser case
- [x] PanelChat.tsx: executeSlash case
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
