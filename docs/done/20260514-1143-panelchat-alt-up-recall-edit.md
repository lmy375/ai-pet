# PanelChat `⌥↑` 召回最近 user 消息直接进编辑模式

## 背景

TODO：

> ChatPanel 输入框 ↑ 召回最近 user 消息直接进编辑模式：键盘党 ergo 的 IM 风扩展。

20260514-1106 上一轮已经实现"双击 user bubble → inline 编辑 → Enter 重发"路径。鼠标用户已经走得通；纯键盘党还要先 ⌘ + 滚轮 / 上滚找到最近一条 user bubble、再切回鼠标双击 —— 体感断流。IM 应用（iMessage / Telegram / Discord）都支持「↑ in empty input → edit last own message」，一键直达编辑器。

## 改动

### `src/components/panel/PanelChat.tsx`

在 `handleInputKeyDown` 的 ArrowUp 分支前插入 `⌥↑` 拦截：

```ts
if (
  e.key === "ArrowUp" &&
  e.altKey &&
  !e.metaKey &&
  !e.ctrlKey &&
  input.length === 0 &&
  historyCursor === null &&
  !isLoading
) {
  let lastUserIdx = -1;
  for (let i = items.length - 1; i >= 0; i--) {
    if (items[i]?.type === "user") { lastUserIdx = i; break; }
  }
  if (lastUserIdx >= 0) {
    e.preventDefault();
    enterEditMode(lastUserIdx);
    return;
  }
}
```

复用上一轮已经存在的 `enterEditMode(idx)` —— 内含含图 / streaming 拒绝 + 状态切换的全部逻辑。

**为什么 `⌥↑` 而非纯 `↑`**

纯 `↑` 当下是 shell-readline 风 send-history 循环 (`cap 20 · localStorage 持久 · 跨窗口共享`)。一些用户已经形成"empty input + ↑ = 召回上一条发送"的肌肉记忆 —— 直接把它改成 IM 风 enter-edit 会破坏既有体验。

`⌥↑` 是干净加法：

| 触发条件 | 行为 |
|---|---|
| 空 input + `↑` | （不动）send-history 循环到 index 0 |
| 历史模式中 `↑` | （不动）cursor +1 往前翻 |
| 空 input + `⌥↑` | （新增）找最近一条 user item，进 inline 编辑 |
| 在 streaming / 历史模式 / 有 input 字符 | `⌥↑` 让出键位（条件不满足直接 return） |

`⌥` 修饰键的语义是"高级 / 字级 / 别的语义"，与 macOS 文本编辑器的 `⌥←/→` 单词跳转同模式 —— 用户对 `⌥` 有"我要做不一样的事"的直觉。

### `src/components/panel/KeyboardHelpOverlay.tsx`

「聊天输入框」段加两条：

- `⌥↑ / Alt+↑` — PanelChat IM 风召回最近 user bubble 进 inline 编辑
- `双击 user 气泡` — 上一轮已实现但帮助层没收录，本轮补上对偶的"鼠标路径"行

## 不做

- **不替换纯 `↑` 行为**。如上：会破坏既有 history-cycle 肌肉记忆。
- **不支持 `⌥↑ ⌥↑` 翻到更早 user bubble**。当前只跳到最近一条；想编辑更早的用双击。多按拓展会冲突 send-history 路径，复杂度激增收益小。
- **不动 ChatMini（桌面气泡）**。桌面 mini chat 是只读历史显示，没 inline 编辑的载体；要编辑得开 Panel。
- **不加 `⌥↓`**。从编辑态退出已经有 `Esc` / 取消按钮，再加快捷键反而让快捷键集臃肿。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.17s
- 改动 ~15 行 + 帮助层 2 条，影响极局部 —— 既有 ↑↓ history-cycle / Enter 提交 / Esc 清空全部行为不变。

## 后续

- 桌面 ChatPanel 的输入框是否对偶？目前 ChatPanel.tsx 的消息列表是只读 ChatMini，没有"双击 bubble 编辑"载体，所以 `⌥↑` 暂无意义。如果未来 ChatMini 加 inline 编辑能力（涉及流式回复重启），再接 `⌥↑`。
- 编辑历史轨迹（哪几条被改过、原版长什么样）—— 整体编辑特性的潜在拓展，目前 IM 风格"丢弃后续"已经够用。
