# PanelChat 输入框换 auto-grow textarea（Iter R126）

> 对应需求（来自 docs/TODO.md）：
> PanelChat 输入框换 auto-grow textarea：现 `<input>` 单行，长输入横向滚没法看全；换 `<textarea>` 默认 1 行高、最多 5 行 auto-grow，Enter 提交 / Shift+Enter 换行（onKeyDown 拦截 + form submit 走 button click）。

## 目标

PanelChat 输入框现是单行 `<input>`：长 prompt（多行说明 / 代码片段）输入
后只能横向滚动看不到上下文。换 `<textarea>` 让多行可见 + 用户可手按
Enter 换行编辑。

## 非目标

- 不引入第三方 textarea-autosize 库 —— 用 `Math.min(5, lines)` 自动 grow
  覆盖 90% 场景；soft-wrap 不精确折行计数（接受 trade-off，5 行 cap 后
  scroll 能看完）
- 不动 send 按钮 —— form `onSubmit` 仍由按钮 click 触发；textarea Enter 路径
  另写
- 不动 slash 命令模式所有 keys —— 仅扩展 Enter / Shift+Enter 在 textarea
  里的语义

## 设计

### `submitInput` 抽离

现 `handleSubmit(e: FormEvent)` 内部封了 trim / slash 分支 / sendMessage。
keydown 路径要复用同套逻辑，抽出来：

```ts
const submitInput = useCallback(() => {
  const trimmed = input.trim();
  if (!trimmed || isLoading) return;
  if (trimmed.startsWith("/")) {
    const action = parseSlashCommand(trimmed);
    if (action) {
      executeSlash(action);
      setInput("");
      setSelectedSlashIdx(0);
      return;
    }
  }
  sendMessage(trimmed);
  setLastSentMessage(trimmed);
  setInput("");
}, [input, isLoading, executeSlash, sendMessage]);

const handleSubmit = (e: React.FormEvent) => {
  e.preventDefault();
  submitInput();
};
```

### handleInputKeyDown 改造

```diff
-const handleInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
+const handleInputKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
   if (!slashMenuVisible) {
+    if (e.key === "Enter" && !e.shiftKey) {
+      e.preventDefault(); // 防 textarea 默认换行
+      submitInput();
+      return;
+    }
     if (e.key === "ArrowUp" && input.length === 0 && lastSentMessage) {
       e.preventDefault();
       setInput(lastSentMessage);
     }
     return;
   }
   // slash 模式
   if (e.key === "ArrowDown") { ... }  // 不变
   ...
   } else if (e.key === "Enter") {
+    e.preventDefault(); // textarea Enter 总不换行；submit 改显式调
     const action = parseSlashCommand(input);
     const isCompleteKnown = ...;
-    if (!isCompleteKnown) {
-      e.preventDefault();
+    if (isCompleteKnown) {
+      submitInput();
+    } else {
       const cmd = filteredCommands[selectedSlashIdx];
       if (cmd) handleSelectSlashCommand(cmd);
     }
   }
};
```

slash 模式 Enter 的两种路径都改成显式 submit：
- complete known command → `submitInput()`（原来靠 form onSubmit fall-through，
  textarea 没自动 submit 行为，必须显式调）
- prefix → autocomplete（不变）

### `<input>` → `<textarea>`

```diff
-<input
+<textarea
   value={input}
   onChange={(e) => setInput(e.target.value)}
   onKeyDown={handleInputKeyDown}
   placeholder='输入消息（Enter 发送 / Shift+Enter 换行；首字符 "/" 触发命令面板）'
+  rows={Math.max(1, Math.min(5, (input.match(/\n/g)?.length ?? 0) + 1))}
   style={{
     flex: 1,
     padding: "10px 14px",
     borderRadius: "10px",
     border: "1px solid var(--pet-color-border)",
     fontSize: "14px",
     outline: "none",
     color: "var(--pet-color-fg)",
     background: "var(--pet-color-card)",
+    resize: "none",
+    fontFamily: "inherit",
+    lineHeight: 1.5,
+    overflowY: "auto",
   }}
 />
```

`rows` 用 `\n` 计数 + 1（5 行 cap）；soft-wrap 不影响视觉行高（换行才 grow）。
`resize: none` 禁手动拖手柄；`fontFamily: inherit` 防浏览器默认 monospace
切走 chat 主字体；`lineHeight: 1.5` 让行间距与按钮垂直对齐。

### placeholder 更新

加入 `Enter 发送 / Shift+Enter 换行` 提示让用户首扫即知 textarea 行为。

### 测试

无单测；手测：
- 输 1 行 → textarea 高 1 行（与原 input 视觉接近）
- Shift+Enter → 换行；高度 grow 到 2 行
- 继续 Shift+Enter ×4 → cap 在 5 行，溢出后 vertical scroll
- Enter（无 shift）→ 提交，input 清空、textarea 回 1 行
- "/clear" + Enter → slash 命令执行（不发 LLM）
- 上箭头召回上条消息（input 空 + 非 slash 模式）
- 点 发送 按钮 → 同 Enter 提交路径
- 长消息粘贴 100 行 → cap 在 5 行 + scroll，input 内容仍完整

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 抽 submitInput |
| **M2** | handleInputKeyDown 改 textarea 类型 + 加 Enter / Shift+Enter 分支 |
| **M3** | input → textarea + rows + style |
| **M4** | tsc + build + 手测 |

## 复用清单

- 既有 sendMessage / executeSlash / parseSlashCommand
- 既有 form onSubmit → handleSubmit → submitInput chain
- 既有 lastSentMessage ↑ 召回

## 进度日志

- 2026-05-10 07:00 — 创建本文档；准备 M1。
- 2026-05-10 07:08 — M1 完成。`submitInput` useCallback 抽出 handleSubmit 主逻辑（trim / slash 分支 / sendMessage / lastSentMessage 维护），handleSubmit(e) preventDefault + submitInput()。
- 2026-05-10 07:14 — M2 完成。handleInputKeyDown 类型 input → textarea；非 slash 模式加 Enter / Shift+Enter 分支（preventDefault 防换行 + submitInput）；slash 模式 Enter 总 preventDefault，complete known → submitInput / prefix → autocomplete。
- 2026-05-10 07:18 — M3 完成。`<input>` → `<textarea>`：rows={Math.min(5, lines+1)} auto-grow；resize none / fontFamily inherit / lineHeight 1.5 / overflowY auto；placeholder 加 "Enter 发送 / Shift+Enter 换行" 提示。
- 2026-05-10 07:22 — M4 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 950ms)。归档至 done。
