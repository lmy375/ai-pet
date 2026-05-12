# ChatMini bubble 再回应快捷

## 需求

桌面 mini chat 上往回滚看老对话时，想"接着这条再问"得手动复制 excerpt
打到输入框里，繁。给每条 assistant 气泡加 hover-only 💭 按钮，一击把"关
于「<excerpt>」 "塞到 input 前缀，用户接着敲问题即可。

## 实现

### `src/components/ChatMini.tsx`

- 与既有 copyBtn 同一行 hover-display CSS（`.pet-mini-row-copy` class）
- `respondBtn` 仅在 assistant + 非空 text 时渲染：
  - onClick → 截 excerpt 到 30 字（超长加 …）
  - `window.dispatchEvent(new CustomEvent("pet-mini-respond-to", { detail: excerpt }))`
- 放在 copyBtn 左侧（assistant 行 bubble 右侧）：`{respondBtn}{copyBtn}` 顺序

### `src/components/ChatPanel.tsx`

- 新 useEffect 监听 `pet-mini-respond-to`：
  - `detail` 是 excerpt 字符串
  - `setInput(prev => prev ? "关于「X」 " + prev : "关于「X」 ")` —— 已有
    内容时 prefix 让锚点先入眼；空时仍写锚点 + 空格
  - setTimeout(0) focus textarea + 光标到末尾，用户可立即续写问题

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - hover assistant 气泡 → 💭 按钮显（与 📋 复制按钮同 hover 节奏）
  - 点 💭 → ChatPanel textarea 变 "关于「<这条 assistant 内容前 30 字>」 "
    + 光标在末尾 + 焦点已落
  - 用户敲"细节是？"+ Enter → LLM 看到 "关于「<excerpt>」 细节是？"
  - assistant 行没文本（纯图）→ 不渲染 💭（同 copyBtn 条件）
  - user 行 → 不渲染 💭（仅 assistant）
  - textarea 已有"hello"草稿 → 点 💭 → 变 "关于「X」 hello"（锚点前缀）

## 不在本轮范围

- 没做 panel chat（PanelChat 历史）同款按钮：panel 已有完整复制 / reaction
  入口，且 panel chat 用户多用 inline 引用；后续可加但语义重复
- 没做"自动总结老消息再问"：本轮只做"截 30 字 anchor"轻量路径；LLM 端
  summarization 是单独需求
- 没做"⌘+click → 直接发送"：节省一击但容易误触，先保留"用户主动敲完
  再 Enter"路径

## TODO 池剩余

- PanelTasks "now" 标记 + 桌面 nudge
- PanelDebug 快照对比 diff
