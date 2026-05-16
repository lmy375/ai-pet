# detail.md 编辑器 toolbar 加 🧠 ask LLM about selection 按钮

## 背景

owner 在 detail.md 编辑长 task 进度笔记 / 计划时，常想"这段值得让 LLM 解释 / 评论 / 给建议"。但当前要：
1. 选中文本 → ⌘C
2. 切到聊天 tab
3. textarea 敲 "关于「..."
4. 粘选段
5. 拼问号

5 步。加 toolbar 🧠 按钮一键：选区 → "关于「<excerpt>」 " 预填 PanelChat textarea + 切到聊天 tab + 焦点 textarea。

## 改动

### `src/PanelApp.tsx` — cross-panel deeplink state

```ts
const [pendingChatPrefill, setPendingChatPrefill] = useState<string | null>(null);
const requestChatPrefillFromSelection = (text: string) => {
  const trimmed = text.trim();
  if (!trimmed) return;
  const excerpt = trimmed.replace(/\s+/g, " ").slice(0, 50);
  const ellipsis = trimmed.length > 50 ? "…" : "";
  setPendingChatPrefill(`关于「${excerpt}${ellipsis}」 `);
  setActiveTab("聊天");
};
```

PanelChat / PanelTasks props 接入：
```tsx
<PanelChat pendingChatPrefill={...} onConsumePendingChatPrefill={...} />
<PanelTasks onAskLLMAbout={requestChatPrefillFromSelection} />
```

### `src/components/panel/PanelTasks.tsx` — toolbar 🧠 按钮

```tsx
{onAskLLMAbout && (() => {
  const selStart = Math.min(detailCursorPos, detailSelectionEnd);
  const selEnd = Math.max(detailCursorPos, detailSelectionEnd);
  const hasSel = selEnd > selStart && ...;
  return (
    <button
      disabled={!hasSel}
      onClick={() => {
        if (!hasSel) return;
        const text = editingDetailContent.slice(selStart, selEnd).trim();
        if (!text) return;
        onAskLLMAbout(text);
        setBulkResultMsg(`🧠 已切到聊天 tab + 预填 "关于「...」"...`);
      }}
      title={hasSel ? "把选区 N 字封装成 ..." : "无选区..."}
      style={{...opacity hasSel ? 1 : 0.4...}}
    >
      🧠
    </button>
  );
})()}
```

### `src/components/panel/PanelChat.tsx` — 消费 effect

```ts
useEffect(() => {
  if (!pendingChatPrefill) return;
  setInput(pendingChatPrefill);
  window.setTimeout(() => {
    const ta = composeTextareaRef.current;
    if (ta) {
      ta.focus();
      const end = ta.value.length;
      try { ta.setSelectionRange(end, end); } catch {}
    }
  }, 0);
  onConsumePendingChatPrefill?.();
}, [pendingChatPrefill, onConsumePendingChatPrefill]);
```

## 关键设计

- **excerpt 50 字 cap + "…" ellipsis**：避免 prefix 过长喧宾夺主；50 字以内全文，> 50 截断。owner 想精准提问可继续补充正文。
- **空白归一**：`replace(/\s+/g, " ")` 让多行选段在 prefix 里成单行 —— 让"关于「abc def」"看起来自然。
- **复用既有 cross-panel state 模式**：与 pendingChatMatch / pendingDueFilter / pendingTaskFocusTitle 同 lift-state 模式 —— PanelApp 持有 state，子组件双向通信。
- **gate on `onAskLLMAbout` prop**：仅 PanelApp 端 wire 时显按钮；其它 caller（未来 PanelMemory 想 reuse PanelTasks 部分组件等）不传该 prop → 按钮不显，避免空 click 无效。
- **focus + setSelectionRange to end**：owner 切到聊天 tab 后 textarea 已聚焦 + caret 在 prefix 末尾，可立刻敲问题。
- **`setBulkResultMsg` 反馈**：与 toolbar 其它复制按钮同 toast 区，UX 一致。
- **setTimeout 0 等下一帧**：React commit 完成后再 focus textarea，确保 PanelChat 已 mount。

## 不做

- **不自动发送消息**：仅 prefill + focus，owner 决定补充什么 + 何时 send。
- **不写测试**：纯 React state + cross-panel callback + setSelectionRange；视觉验证（detail 编辑器选一段 → 点 🧠 → 应切到聊天 tab + textarea 含 "关于「...」 "）足够。
- **不传 detail / task context 进 prompt**：owner 想要 LLM 看完整 detail 走既有 「📤 复制 LLM consume 段」 (iter #203) 路径。本按钮只做"轻量提问"。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.15s
- 改动 ~140 行（PanelApp state 25 + PanelTasks signature 8 + toolbar 按钮 55 + PanelChat props 8 + 消费 effect 25 + 注释）。既有 cross-panel pendingChatMatch / pendingDueFilter / pendingTaskFocusTitle 路径完全不动；选区计算复用 iter #207 detailSelectionEnd state。

## TODO 状态

剩 1 条留池：
- detail.md 编辑器 ⌘K 唤起 task quick-find palette

## 后续

- ⌥+click 🧠 改 "把选段送到 chatMini textarea"（pet 窗，跨 webview deeplink）。
- 加 prompt 模板选择 dropdown："关于「...」帮我看看 / 帮我反驳 / 给优化建议 / 翻译" 5 个 preset。
- selection 含 markdown table / code block 时智能保留格式（当前 \s+ 替换空格会破坏 code）。
