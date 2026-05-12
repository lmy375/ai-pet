# PanelChat compose 草稿持久化

## 需求

textarea 内未发送的字符串只在 React state 里，切 session / 关 panel / 重启
app 都会丢。用户在 A 会话敲了一段思考再切去 B 看一下再回来 → 内容没了。
每个 sessionId 独立存 localStorage，3s debounce 写盘 + session 切换时立
即 flush 当前内容到 prev session 的 key。

## 实现

`src/components/panel/PanelChat.tsx`：

- 新 helper `writeDraft(id, text)`：empty 走 removeItem 不积陈旧；非空 setItem；
  try/catch 包 IO 防私密浏览 / 配额满
- 新 `inputRef: useRef<string>` + 跟随 input 的 useEffect 同步 — 给"切
  session 时拿当前 input 当 prev session 的 draft"用，避开 state 异步
- 新 `prevSessionIdRef: useRef<string>` — 切换时知道之前在哪 session
- 新 sessionId-effect：
  - prev != cur 时 `writeDraft(prevId, inputRef.current)` flush（覆盖 3s
    debounce 还没触发的情况）
  - update prevSessionIdRef
  - 读 `pet-chat-draft-${sessionId}` 填入 textarea；缺失 → setInput("")
- 新 input/sessionId-effect：3s debounce 写盘；timer cleanup 在 unmount /
  下次 deps 变化时清

## 边角

- send / 清空 input → 3s 内最后一次 setInput("") → debounce 写 "" →
  writeDraft removeItem → storage 干净
- 切 session 立即 flush 而非等 3s debounce：防"敲 1s 立刻切走"丢稿
- 关 panel webview 不触发 cleanup（webview 销毁不走 React lifecycle）；
  最坏丢 3s 内未 debounce 写完的内容。可接受 — 3s 内的草稿损失体感小
- inputRef 同步通过专门的 useEffect[input] 而非函数式访问，保证 ref 始终
  对应当前 commit 后的值

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 在 A 会话敲"hello world"等 4s → localStorage 出现 `pet-chat-draft-A`
  - 切到 B 会话 → textarea 显 B 的 draft（如 B 没草稿则空）
  - 切回 A → textarea 自动填回"hello world"
  - A 敲一半（< 3s）立刻切 B → 仍能保留 A 的内容（切换 effect flush）
  - 发送消息后 textarea 清空 → 3s 后 storage 中的 draft key 也被删
  - 关 panel 重开 → 草稿还在
  - 私密浏览模式 → 草稿仅 session 内有效；console 不报错

## 不在本轮范围

- 没限制 draft 体积：localStorage 通常 5MB；单条 chat draft < 几 KB 无忧
- 没做 draft 历史（多版本撤销）：单 draft 已能解 90% 用例；多版本是 IDE
  级特性
- 没 sync across windows：panel 是单 webview，无 multi-window 同 sessionId
  冲突

## TODO 池剩余

- ChatMini 桌面气泡 markdown 块级语法
- PanelTasks 归档独立 tab
- PanelMemory hover 显 detail.md preview
- PanelDebug 工具调用历史按 tool name group
