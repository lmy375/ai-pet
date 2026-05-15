# 统一 PanelChat / 桌面输入框的 `↑/↓` 历史栈

## 背景

上一个 commit 给桌面 pet 窗 `ChatPanel.tsx` 加了 shell 风 `↑/↓` 输入历史（cap 20 + localStorage 持久 + dedup-and-move-to-front）。但发现 **PanelChat（面板大聊天框）早就有这个功能**，只是：

1. 只在内存里维护（每次 mount 一个全新空 array） —— 关 panel 再开就丢
2. 顺序约定是 newest-at-end，与新 ChatPanel 的 newest-at-front 不一致
3. push 策略是无 dedup append，与新 ChatPanel 的 dedup+move-to-front 不一致
4. 两个面板的"我说过什么"完全独立 —— 用户在 PanelChat 里发 N 条然后切回 pet 窗按 `↑`，召回的不是刚发的那条

而 mental model 上"我刚发过什么"是用户视角的统一概念，不该有两套独立栈。

## 改动

### 新模块 `src/components/chatHistoryStore.ts`

```ts
export const SENT_HISTORY_CAP = 20;
export function readSentHistory(): string[];
export function pushSentHistory(text: string): string[]; // dedup + move-front + cap + write
```

实现：
- localStorage key `pet-chat-history`（沿用上轮 ChatPanel 已经用的 key —— 兼容已经持久过的用户数据）
- pushSentHistory：trim → dedup（同内容旧位置移除）→ unshift → slice(0, CAP) → 写盘 → 返回新数组
- read 时类型不安全的 JSON / 非字符串项 / 越界 → 静默 fallback 空数组

### `src/components/ChatPanel.tsx`

- 删掉本地 `readSentHistory` / `writeSentHistory` / `HISTORY_KEY` / `HISTORY_CAP`，从新模块导入
- submit 后调 `pushSentHistory(trimmed)` 拿新数组 → `setSentHistory(next)`

行为不变（newest-at-front 已经是这套）。

### `src/components/panel/PanelChat.tsx`

- `messageHistory` 初始化从 `useState<string[]>([])` 改为 `useState(readSentHistory)` —— mount 时立即拉持久态
- submit 处的两个 push 点（多模态分支 + 主分支）改用 `pushSentHistory(trimmed)` 返回值 set 进 state；行为切换到 dedup + move-to-front
- `↑/↓` 遍历方向**翻转**到 newest-at-front：
  - `↑` 进入模式：`cursor = 0`（最新）；mode 中 `cursor = min(cursor+1, length-1)`（往前翻 = 索引增大 = 更旧）
  - `↓` mode 中 `cursor = cursor - 1`；< 0 → 退出 + 清空
- 注释 R129 改写说明新约定

### 行为变化（用户可见）

1. **跨窗口召回**：PanelChat 发的消息能在 pet 窗按 `↑` 召回，反之亦然
2. **重启 app 后召回**：以前 PanelChat 重启后历史归零，现在能保留 20 条
3. **dedup**：连发相同内容只占一格（"resend 同一条多次仍能各自 ↑ 命中"的语义没了 —— 重 dedup 的清晰栈意义 > 这一稀有用法）

## 不做

- 不加跨窗口 live sync（storage event 监听）：两个窗口同时打开同时发消息的场景不常见；下次任一窗 mount 时 read 自然合并到最新视图即可
- 不在 chat session 文件里写历史：history 是 input ergonomics，与会话内容解耦 —— 走 localStorage 即可
- 不让 slash 命令进历史（与现状一致）：`/clear` `/done <title>` 这类控制流不该污染"我说过什么"的语义

## 验收

- `npx tsc --noEmit` ✅
- pet 窗发 "abc" → 切到 PanelChat 敲 `↑` → 拉回 "abc"
- PanelChat 发 "xyz" → 切回 pet 窗敲 `↑` → 拉回 "xyz"
- 重启 app → 仍能 `↑` 召回上一会话发过的 20 条
- 连发 "abc" 两次 → 历史里只有 1 条 "abc"（dedup）

## 完成

- [x] chatHistoryStore.ts 新增
- [x] ChatPanel.tsx 改导入
- [x] PanelChat.tsx 改 init + push + ↑/↓ 方向翻转
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
