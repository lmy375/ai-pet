# PanelChat `/mood` slash 命令

## 背景

上轮加了 TG `/mood`。桌面 PanelChat 还没有，但有时候用户在面板聊天 / 工作时，pet 窗被遮挡看不到 MoodWidget —— `/mood` 一行命令让会话内立刻知道宠物心情。

## 改动

### `src/components/panel/slashCommands.ts`

- `SLASH_COMMANDS` 在 `/today` 之后插 `{ name: "mood", description: "查看宠物当前心情", parametric: false }`
- `SlashAction` union 加 `{ kind: "mood" }`
- `parseSlashCommand` 加 `case "mood": return { kind: "mood" }`

### `src/components/panel/PanelChat.tsx`

`executeSlash` 加 `case "mood"`：

```ts
try {
  const m = await invoke<{ text: string; motion: string | null; raw: string }>("get_current_mood");
  // 后端 read_current_mood 返回 None 时 CurrentMood{ text:"", motion:None, raw:"" }
  // —— 用 raw === "" 区分"没记过" vs "记了空字符串"
  if (m.raw === "") {
    pushLocalAssistantNote("🐾 宠物还没记心情；一会儿主动开口时会写一笔。");
    break;
  }
  const textLine = m.text.trim() === "" ? "🐾 心情：（无文字）" : `🐾 心情：${m.text.trim()}`;
  const lines: string[] = [textLine];
  if (m.motion && m.motion.trim()) lines.push(`  动作组：${m.motion.trim()}`);
  pushLocalAssistantNote(lines.join("\n"));
} catch (e) {
  pushLocalAssistantNote(`/mood 失败：${e}`);
}
```

文案与 TG `format_mood_reply` 对齐（同三态：无记录 / 含 motion / 不含 motion / 空文字）。

## 不做

- 不把文案抽公共 helper：跨语言（Rust / TS）；行数太少（~10 行）不值得跨 IPC 共享
- 不显 mood history：那是桌面 MoodWidget hover 的活；slash 命令是当下快照入口
- 不持久化（用户最常想"我现在让 pet 心情记什么"是 LLM 派生的事，不是 user 设定）

## 验收

- `npx tsc --noEmit` ✅
- 桌面聊天 `/mood` → 显当前心情（subdued bubble，与其它 slash 反馈视觉一致）
- 没心情记录 → 友好提示
- 含 motion → 显两行

## 完成

- [x] slashCommands.ts: 注册 + parser
- [x] PanelChat.tsx: executeSlash case
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
