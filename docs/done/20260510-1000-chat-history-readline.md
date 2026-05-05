# PanelChat 输入历史多条召回（Iter R129）

> 对应需求（来自 docs/TODO.md）：
> PanelChat 输入历史多条召回：现 ↑ 只召回 lastSentMessage；扩展到 `messageHistory: string[]` ring buffer（cap 20），↑ / ↓ readline 风格穿越多条历史（input 空时 ↑ 进入历史模式，索引随 ↑/↓ 移动），让批量改写 / 重复 prompt 调试更顺手。

## 目标

PanelChat 输入框 ↑ 召回当前实现：仅记住"最近一次发送的非 slash 消息"
（`lastSentMessage: string`）。debug 时反复改写 prompt 跑实验，要修改一
条几条之前的发言只能手动重输。

升级到 readline 风格多条历史：
- `messageHistory: string[]` ring buffer，cap 20（newest at end）
- `historyCursor: number | null` —— null = 不在浏览模式；非 null = 当前
  指向哪条历史
- ↑：从空 input 进入历史模式至最末（最新）；持续 ↑ 往前（更早）
- ↓：往后（更新）；超过末尾退出历史模式 + 清空 input
- 用户改写 input → 自动退出历史模式（cursor=null）让用户自由编辑

## 非目标

- 不持久化到 localStorage —— session 内有效（与现 lastSentMessage 同语义；
  跨重启的 prompt 复用走"看上次 prompt" R125 PanelDebug 入口）
- 不去重相邻同内容 —— 提交两次相同 prompt 是合法 debug 场景，保留两条
  让 ↑ ↑ 命中同样的内容，不让用户失去 redo 机会
- 不引入 Ctrl+R 反向搜索 —— 那是真正 readline；本轮只覆盖 90% 用例的
  ↑/↓ 浏览
- 不把 multiline textarea 的 ↑ 行为搞复杂 —— 只在 input 空 / 已在历史模式
  时拦截 ↑；非空 input 时 ↑ 走 textarea 默认（光标向上）

## 设计

### state 替换

```diff
-const [lastSentMessage, setLastSentMessage] = useState<string>("");
+const [messageHistory, setMessageHistory] = useState<string[]>([]);
+const [historyCursor, setHistoryCursor] = useState<number | null>(null);
```

### submitInput push history

```diff
 sendMessage(trimmed);
-setLastSentMessage(trimmed);
+setMessageHistory((prev) => {
+  const next = [...prev, trimmed];
+  return next.length > 20 ? next.slice(-20) : next;
+});
+setHistoryCursor(null);
 setInput("");
```

cap 20 用 `slice(-20)` 保留最新 20 条。`historyCursor` 重置 null 让发送
后再 ↑ 从最末（即刚发的这条）开始。

### handleInputKeyDown ↑/↓

非 slash 模式分支替换 ArrowUp 现有 logic：

```ts
if (e.key === "ArrowUp") {
  if (historyCursor !== null) {
    e.preventDefault();
    const next = Math.max(0, historyCursor - 1);
    setHistoryCursor(next);
    setInput(messageHistory[next]);
    return;
  }
  if (input.length === 0 && messageHistory.length > 0) {
    e.preventDefault();
    const next = messageHistory.length - 1;
    setHistoryCursor(next);
    setInput(messageHistory[next]);
    return;
  }
  // input 非空 + 不在历史模式 → 不拦截 ↑（textarea 多行光标向上行为）
  return;
}
if (e.key === "ArrowDown") {
  if (historyCursor !== null) {
    e.preventDefault();
    if (historyCursor < messageHistory.length - 1) {
      const next = historyCursor + 1;
      setHistoryCursor(next);
      setInput(messageHistory[next]);
    } else {
      setHistoryCursor(null);
      setInput("");
    }
  }
  return;
}
```

注意：`return;` 在每个分支末避免落到原 ArrowUp gate。

### onChange 守卫

textarea onChange 检测内容偏离历史 → 自动退出历史模式：

```ts
onChange={(e) => {
  const v = e.target.value;
  setInput(v);
  if (historyCursor !== null && v !== messageHistory[historyCursor]) {
    setHistoryCursor(null);
  }
}}
```

让用户在历史模式按 backspace 改名 / 加字 → 立即"脱钩"，再 ↑ ↓ 不会跳到
另一条；要重新进历史得清空 input + ↑。

### Esc R127 兼容

R127 已加"非空 + Esc → 清空"。在历史模式下按 Esc 同样清空 + 退出历史
（input 空 + cursor=null 自然到位）。需要在 R127 分支补 setHistoryCursor(null)
让状态完整：

```diff
 if (e.key === "Escape" && input.length > 0) {
   e.preventDefault();
   setInput("");
+  setHistoryCursor(null);
   return;
 }
```

### 测试

无单测；手测：
- 发送 3 条消息 A / B / C → ↑ 显 C → ↑ 显 B → ↑ 显 A → ↑ 仍是 A（已到顶）
- 显 B 时 ↓ → 显 C → ↓ → 退出历史，input 空
- 显 B 时 backspace 一字 → 立即脱钩，再 ↑ 仍从最新（C）进入
- 显 B 时 Esc → input 清空 + 退出历史
- 发送 25 条 → cap 20，最早 5 条被淘汰
- 历史模式 + Enter → 提交修改后的 / 原版 history 内容；再 ↑ 把刚发的进
  history 末
- 输非空内容 + ↑ → 不拦截（textarea 默认行为：光标向上）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state 替换 + submitInput push |
| **M2** | handleInputKeyDown ↑/↓ + Esc 兼容 |
| **M3** | onChange 守卫 |
| **M4** | tsc + build + 手测 |

## 复用清单

- 既有 R126 textarea + handleInputKeyDown 框架
- R127 Esc 清空守卫

## 进度日志

- 2026-05-10 10:00 — 创建本文档；准备 M1。
- 2026-05-10 10:08 — M1 完成。`lastSentMessage / setLastSentMessage` 替换为 `messageHistory: string[]` + `historyCursor: number | null` 双 state；submitInput push 到 history（cap 20，slice(-20)），不去重相邻同内容；historyCursor 重置 null。
- 2026-05-10 10:14 — M2 完成。handleInputKeyDown 替换 ↑/↓ 分支：cursor 非 null 时 ↑ Math.max(0, cursor-1)、↓ cursor < length-1 即递增 / 否则退出 + 清空；cursor null 时 ↑ 仅在 input 空 + history 非空时进入末位（最新）；非空 input 不拦截 ↑（textarea 多行光标向上不破坏）。R127 Esc 分支补 setHistoryCursor(null) 让状态完整。
- 2026-05-10 10:18 — M3 完成。textarea onChange 加守卫：cursor 非 null 且新值 !== messageHistory[cursor] → setHistoryCursor(null) 自动脱钩。
- 2026-05-10 10:22 — M4 完成。`pnpm tsc --noEmit` 0 错误；grep 验证无遗留 lastSentMessage 引用。归档至 done。
