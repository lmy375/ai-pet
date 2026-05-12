# ChatMini 顶部"复制最近 N 条"按钮

## 需求

⌘+C 已经能拿最新一条 assistant 文本（上一轮做的）。但有时用户想把"过去 5
分钟的一段对话"整个贴到飞书 / 笔记本，单条单条拼太费事。在 ChatMini 顶部加
一个 📋 按钮，弹小菜单选 3/5/10 → 一键拿走最近 N 条 user/assistant 对话。

## 实现

`src/components/ChatMini.tsx`：

- 新 state `copyMenuOpen: boolean`
- 顶部右上角加 📋 按钮（与 ⛶ 同款 `pet-mini-maxbtn` 圆形）
- 按钮位置：`right: onOpenPanel ? 48px : 20px`，让 📋 紧贴 ⛶ 左侧
- click 切 popover；popover 是个小 absolute div，显 "复制最近"标题 + 3/5/10 三个按钮
- `copyRecentN(n)`：
  - filter messages 留 user/assistant
  - slice(-n)
  - 每条带角色 glyph 前缀 `🧑 ...` / `🐾 ...`
  - 双换行分隔，writeText 写剪贴板
  - 复用既有 `copyToast` 顶部反馈 1.5s 自清
- outside-click 关 popover：useEffect 挂 window mousedown + keydown(Esc)
- 防误触：toggle button + popover 各自 stopPropagation(mousedown) 让 outside-click 监听器不抢自身事件

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 桌面：点 📋 → popover 弹 → 选 "5 条" → 顶部药丸 "✓ 已复制最近回复" → 飞书粘贴出 5 段带 🧑/🐾 前缀
  - popover 开时点外面任意位置 → 关
  - Esc → 关
  - 历史不足 N 条 → 拿现有的所有；0 条 → 红色 toast
  - 单条 ⌘+C 路径不变（上轮做的，独立）

## 不在本轮范围

- 不带图片附件：当前 extractText 已剥 markdown image 标记；用户要图就走单条复制路径
- 没在 popover 加"自定义 N"输入框 —— 3/5/10 三档覆盖 90% 场景；要更多去 PanelChat 用 Copy MD
