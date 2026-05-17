# ChatMini ChatBubble 右键菜单「📝 用此话设 transient_note 30m」（iter #377）

## Background

iter #363 加 TG `/transient` 命令、iter #364 加 PanelToneStrip ✍️
写入口。但 owner 看到 pet 说的好话时想"把这句话当下轮 context"，
两个入口都要再敲字。本 iter 加 ChatBubble 右键菜单选项 — 直接选
pet 的这条 reply 文本即可，完成 transient_note 三 surface 闭环
（TG 文字写 + Panel ✍️ UI 写 + ChatBubble 复用 pet 原话写）。

## Changes

### `src/components/ChatMini.tsx`

#### 1. 新 prop（~line 67）

```tsx
onSetTransientNote?: (text: string, minutes: number) => void;
```

#### 2. menu item（~line 2257，紧贴 💭 针对这条再问 之后）

```tsx
{isAssistant && hasText && onSetTransientNote && (
  <button onClick={() => { setCtxMenu(null); onSetTransientNote(text, 30); }}>
    📝 用此话设 transient_note 30m
  </button>
)}
```

仅 assistant + hasText + 父级传 callback 时显（与 iter #361 等条件
化按钮同模式）。30 分钟是 "短期会议 / 短暂上下文" 的合理默认（与
PanelToneStrip preset 中等档对齐）。

### `src/App.tsx`

#### 1. handler（~line 985）

```tsx
const handleMiniSetTransientNote = useCallback(
  async (text: string, minutes: number) => {
    await invoke<string>("set_transient_note", { text: body, minutes });
    appendAssistant(`📝 已用此话设 transient_note（${minutes} 分钟有效）：「<preview>」`);
  },
  [appendAssistant],
);
```

复用既有 `set_transient_note` Tauri command（iter #363/#364 同后端）+
`appendAssistant` 反馈机制（与 handleMiniSaveAsNote 模式一致）。

#### 2. 传 prop

```tsx
<ChatMini ... onSetTransientNote={handleMiniSetTransientNote} />
```

## Key design decisions

- **30 分钟硬编码而非 sub-menu**：context menu 第 4 个 item 已经是
  发现的边缘 — 加 5/15/30/60 子选项会过度膨胀。30 分钟覆盖最常见
  "刚说的这句话保留半小时给下轮"场景；想精细化走 PanelToneStrip
  ✍️ chip（iter #364）4 档 preset 或 TG /transient（任意 1..=10080）。
- **prop callback 而非 ChatMini 内 invoke 直调**：与既有
  onSaveAsTask / onSaveAsNote / onLike 同 pattern — ChatMini 是
  pure-presentational，所有 IPC 在 App.tsx。
- **`text` 直接用 raw markdown 而非 plain**：transient_note 是给
  pet 看的"我刚说啥"，markdown 标记可能丢失但 spirit 保留。如果
  未来发现"加粗 / 链接干扰 LLM" 再做 plain transform。
- **appendAssistant 反馈而非 setMessage toast**：保持与
  handleMiniSaveAsTask / handleMiniSaveAsNote 一致的"chat 内 inline
  ack"风格 — owner 知道动作生效且不打扰 main chat 流。
- **不在 user bubble 显此选项**：用户自己说的话已是 chat context，
  写 transient_note 重复；只对 pet reply 有意义（让"刚才那条好建议
  继续影响下轮"）。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.27s)
- 后端无改动 — 复用 iter #363 暴露的 set_transient_note Tauri 命令
