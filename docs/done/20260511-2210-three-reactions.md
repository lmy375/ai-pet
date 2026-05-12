# PanelChat assistant 三键 reaction

## 需求

桌面 mini chat 已有"最新一条 assistant 👍" 入口，但 panel 里每条历史回复
都看完整内容，用户想对具体某条标"👍 赞同 / 👎 不满意 / 🤔 没看懂"。三键
信号写到既有 feedback_history.log，反过来喂 proactive prompt 的 register
adapter。

## 实现

### 后端 `src-tauri/src/feedback_history.rs`

- `FeedbackKind` enum 加第 5 个 variant `Puzzled`
- `as_str / parse_line` 增 "puzzled" 映射
- `format_feedback_aggregate_hint`：parts 顺序 replied / liked / **puzzled** /
  ignored / dismissed；puzzled > 0 才显，与 liked / dismissed 同模式
- `format_feedback_hint` 加 Puzzled 分支："上次你说「X」用户表示困惑 —
  换角度澄清同件事 / 短具体话 / 避免抽象"
- 新 commands：
  - `record_bubble_puzzled(excerpt)` — 写 Puzzled
  - `record_message_disliked(excerpt)` — 写 Dismissed（与桌面气泡
    record_bubble_dismissed 对偶，但语义在 panel 上下文）
- **NOT 改 negative_signal_ratio / count_trailing_negative**：Puzzled 是
  "没听懂"中性信号，不算负面。让 cooldown adapter 维持当前阈值不被混淆
- lib.rs 注册两条新命令

### 前端 `src/components/panel/panelChatBits.tsx`

- 新导出 type `AssistantReaction = "liked" | "disliked" | "puzzled"`
- `CopyableMessage` props 加 `reaction?: AssistantReaction | null` +
  `onReact?: (idx, kind, content) => void`
- assistant 行渲染 reactionRow：3 个按钮（👍/🤔/👎，顺序：正→中→负 让
  视觉对偶），每个用既有 `.pet-copy-btn` 类（共享 row-hover 显隐 + 自身
  hover 强化），selected 强制 opacity=1 + 标志色 + tint 底（绿/黄/红）
- 布局：assistant 行从右到左 `bubble [reactionRow] [copy button]`

### 前端 `src/components/panel/PanelChat.tsx`

- 新 state `reactionsByIdx: Record<number, AssistantReaction>` —
  session-only（切 session 清空），idx → kind
- `handleReact(idx, kind, content)`：
  - 同 kind 重复点 → 切换 off（删 map 项），不再 invoke（避免重复入库）
  - 不同 kind → 覆盖 map 项 + invoke 对应命令；fire-and-forget
  - excerpt 截到 200 字符（feedback_history 内部又会按 40 char 再截，但前
    端截一刀避免 IPC 传超长 string）
- loadSession 切会话时 setReactionsByIdx({}) 清状态
- assistant 路径的 CopyableMessage 传 `reaction={reactionsByIdx[i] ?? null}`
  和 `onReact={handleReact}`

## 验证

- `cargo check` clean（保留既有 unrelated warnings）
- `npx tsc --noEmit` clean
- 行为：
  - panel chat 把鼠标 hover 到任一历史 assistant 行 → 见 👍 🤔 👎 三按钮
    弱可见
  - 点 👍 → 按钮变绿 + 加绿底 + 加粗 + 持续可见；feedback_history.log
    出现一行 `... liked | <excerpt>`
  - 同条再点 👍 → 切换 off，按钮回灰，不再写日志
  - 点 🤔 → 按钮变黄 + 黄底；新写一条 puzzled
  - 切到另一 session → 当前 session 内的反馈高亮全清，feedback_history
    里的条目仍在
  - 设置页 panel 的 feedback timeline（既有）能显新的 puzzled / dismissed 类
    型（serde lowercase 自动序列化）

## 设计选择

- Puzzled 不计 negative：保持 cooldown adapter 阈值稳定。Puzzled 是"说不
  清"信号，对应的应是"换说法"而非"少说话"
- 前端不持久化反馈高亮跨 session：feedback_history.log 是权威落盘；UI 高
  亮只为"我这次浏览时点的"反馈，否则切回去看老对话满屏标记，反而混乱
- 复用 `.pet-copy-btn` class：少加一条 CSS 规则；hover 显隐 / accent border
  在 reaction 按钮上也合理

## 不在本轮范围

- 没改 ChatMini 桌面气泡：那里已有 👍 单键且语义清晰；扩三键会让 mini 视
  觉变拥挤
- 没在 panel feedback timeline 里加新 puzzled 的 chip 配色（自动 fallback
  到 muted）：后续若用户反馈"想区分颜色"再单独做
- 没改 negative_signal_ratio 把 Puzzled 计入：保守不动，避免 cooldown
  调整链路上其它依赖断裂

## TODO 池剩余

- /image 历史 prompt 菜单显缩略图
