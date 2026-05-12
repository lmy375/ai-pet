# /image 在桌面 ChatPanel 也生效

## 需求

桌面 mini chat 里敲 `/image dragon` 当普通用户消息发给 LLM，宠物收到的就是
那串字符 → 无法生图。希望桌面也能直接用 /image。

## 设计

不在 ChatPanel 处理 —— 它是纯输入组件不知道消息历史。在 App.tsx 的
`handleSend` 路由层加 slash 检测：parseSlashCommand 命中 `image` / `imageHelp`
时走 image_generate 路径，append 一条 assistant message 到 useChat；其它
slash（/clear、/tasks 等是 panel-only 概念）下落到 LLM。

useChat 新增 `appendAssistant(content, images?)` 方法让外部能 push 消息，绕过
LLM 调用。

## 实现

### useChat

`src/hooks/useChat.ts`：

- 新 callback `appendAssistant(content, images?)`：构造 ChatMessage with ts
  → setMessages 添加 → itemsRef 同步添加 ChatItem (含 images 字段) →
  saveSession 落盘
- 用 functional setState + 同步赋值新数组的小技巧让 saveSession 拿到 fresh
  references（不依赖 setTimeout，无 race）
- export 在 hook 返回值里

### App.tsx

`src/App.tsx`：

- import `parseSlashCommand`, `formatImageHelpText` 复用 PanelChat 同源解析
  / help 文案
- handleSend 改为 async，先 parse trim 后的 message：
  - `kind: "imageHelp"` → `appendAssistant(formatImageHelpText())`
  - `kind: "image"` → 处理 -r 引用最近 assistant 同 PanelChat 算法；append
    "🎨 正在生成…" 占位 → invoke image_generate → append 结果（成功带 urls，
    失败显错误）
  - 其它 / 非 slash → 原 sendMessage 路径

`-n / -r / -s` 全套 flag 都生效，与 PanelChat 行为对齐。help (`/image -h`)
也走桌面路径。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 桌面敲 `/image dragon` → mini chat 出现"🎨 正在生成…" → ~10-30s 后变
    "🎨 dragon" + 缩略图
  - `/image -n 2 -s 1024x1792 cat` → 部分成功支持，错误条目显在 ⚠ 段
  - `/image -h` → assistant 行显内置 help
  - `/image -r 加色彩` → 取最近 assistant 文本 + "加色彩"作 prompt
  - 没有上文 → "⚠ /image -r：当前会话还没有 assistant 回复可引用"
  - 非 slash 消息（"今天天气真好"） → 走 sendMessage / LLM
  - 未知 slash（"/foo"）→ 走 sendMessage（LLM 自然处理）

## 不在本轮范围

- 没在桌面挂 `/help` / `/clear` 等其它 slash —— 桌面 UX 是单输入框 / 单聊
  天历史；这些命令在 panel 上下文（tab 切换 / 多 session 管理）才有意义
- 没把 image 历史 prompt 召回菜单移植到桌面：菜单要 above-input popover，
  桌面 ChatPanel 紧凑布局不适合；用户想用召回去 panel
- 没改 ImagePromptHistoryMenu 的 Enter 行为 —— 桌面调用不走 menu，PanelChat
  仍用之前的实现

## TODO 池剩余

- ChatMini 桌面气泡可拖动
- 设置页 motion_mapping group datalist
