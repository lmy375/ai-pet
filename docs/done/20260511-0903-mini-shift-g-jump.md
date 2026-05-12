# ChatMini Shift+G 跳到底

## 需求

桌面 mini chat 在历史长时滚动比较费 —— 鼠标拖滚动条 / 滚轮转半天 / 点右下角
↓ 浮标都比 vim 风格 `Shift+G` 多一步。给键盘党加个一键到底快捷键。

## 设计

复用既有 followTailRef + scrollTop = scrollHeight 跳底路径（与浮标按钮 click
handler 完全相同）。只 visible 期间挂监听，避免 panel 切走后桌面 mini chat
被卸载但 listener 还在抢键盘。

守护：input / textarea / contenteditable focused 时不响应 —— 用户在 ChatPanel
里 typing 不应该被 G 抢；ChatPanel textarea 此时是 activeElement，跳过。

ctrl / meta / alt 修饰键时也跳过，避免冲突（Ctrl+G 是常见浏览器"查找下一处"
等）。

## 实现

`src/components/ChatMini.tsx`：

```ts
useEffect(() => {
  if (!visible) return;
  const handler = (e: KeyboardEvent) => {
    if (e.key !== "G") return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const ae = document.activeElement as HTMLElement | null;
    if (ae && (ae.tagName === "INPUT" || ae.tagName === "TEXTAREA" || ae.isContentEditable)) return;
    const el = scrollRef.current;
    if (!el) return;
    e.preventDefault();
    el.scrollTop = el.scrollHeight;
    followTailRef.current = true;
    setNotAtBottom(false);
  };
  window.addEventListener("keydown", handler);
  return () => window.removeEventListener("keydown", handler);
}, [visible]);
```

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 桌面焦点不在 textarea → Shift+G → mini chat 滚到底 + 浮标 ↓ 消失
  - 焦点在 ChatPanel textarea typing → Shift+G typing 出 `G` 字符正常（不被抢）
  - 选了某段历史文本 → Shift+G 仍跳（getSelection 守护放在 ⌘+C 那条，这条不需要）
  - panel 切走时 visible=false → listener unmount，不抢键盘

## 不在本轮范围

- 没做 `gg` 跳顶（vim 还有这个）—— mini chat 主流方向是看新消息，跳顶需求更
  弱。如果有用户提需求再加（要 g-prefix 双键状态机）
- 没做"G 跳到上次阅读位置" —— 没维护 lastReadIndex 持久化，扩 useChat 即可
  但本轮不扩
