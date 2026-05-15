# PanelChat `/new [title]` 命令 — 一键新建会话（可选标题）

## 背景

上轮加了 `/title <new>` 改当前会话名。配对动作"新建一个 X 主题会话"现在还得点 + 按钮然后 / title。一行 `/new <title>` 让"开个新话题"也是单步。

## 改动

### `src/components/panel/slashCommands.ts`

- 加 `{ name: "new", description: "新建会话：/new [初始标题]（留空走默认「新会话」）", parametric: true }`
- `SlashAction` 加 `{ kind: "new"; query: string }`
- `parseSlashCommand` `case "new"`：arg 可空（等价点 + 按钮），非空作初始 title

### `src/components/panel/PanelChat.tsx`

`executeSlash` `case "new"`：

```ts
const session = await invoke<Session>("create_session");
const newTitle = action.query.trim();
setSessionId / setSessionTitle / setItems / messagesRef / setShowSessionList…
if (newTitle) {
  const fresh = await invoke<Session>("load_session", { id: session.id });
  fresh.title = newTitle;
  await invoke("save_session", { session: fresh });
}
const idx = await invoke<SessionIndex>("list_sessions");
setSessionList(idx.sessions);
pushLocalAssistantNote(newTitle ? `✨ 已新建会话「${newTitle}」` : "✨ 已新建会话");
```

非空 title 时立刻 save_session 把 title 改掉，**防 auto-title-from-first-message 路径覆盖** —— 那条只在 `sessionTitle === "新会话"` 时触发，立刻改非默认值即可。

## 不做

- 不抽 handleNewSession 复用：handleNewSession 内联了 4-5 个 setState，复用要么传 callback 要么解构出来，scope 反而比就地 inline 大
- 不与 /title 合并（如 `/new /title X`）：两个独立动作各自一行更清晰
- 不写单测：纯前端，项目无 vitest

## 验收

- `npx tsc --noEmit` ✅
- 聊天 `/new` → 新建 "新会话" + 切过去 + assistant 反馈 "✨ 已新建会话"
- 聊天 `/new 周末记账` → 新建 + 标题立即是 "周末记账"，session list 里能看到，后续发消息也不会被 auto-title 替成 first 20 字

## 完成

- [x] slashCommands.ts: 注册 + parser case
- [x] PanelChat.tsx: executeSlash case
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
