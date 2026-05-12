# 聊天气泡视觉抛光（UI 美化 迭代 5）

## 背景

聊天页是 panel 主入口，气泡是核心信息载体。旧 `bubbleStyle`：
- 浅色主题阴影 hardcoded `rgba(15, 23, 42, 0.06)` — dark 主题下完全不可见
- padding 略紧（10/14）
- lineHeight 1.6 略局促
- 无 hover 反馈

## 改动

### `panelChatBits.tsx::bubbleStyle`

- `padding` 10/14 → 11/16（多 1px 上下 + 2px 左右呼吸）
- `lineHeight` 1.6 → 1.65
- assistant `boxShadow` → `var(--pet-shadow-sm)`（迭代 1 token，自动跟随主题）
- user `boxShadow`：accent 35% → 32%，强度调和、`offset-y` 1→2 / blur 4→8，更"飘"
- 加 `transition: box-shadow / transform / border-color`（hover 行内 inline 也走过渡）

### bubble 加 `pet-chat-bubble` className + `data-role` 属性

让外部 CSS 能针对气泡角色单独命中（旧 inline 无 hook 可挂）。

### `PanelChat.tsx` 全局 style 块新增 hover-tier shadow

- `.pet-chat-row:hover .pet-chat-bubble[data-role="user"]` → 0 4px 14px accent 40% alpha
- `.pet-chat-row:hover .pet-chat-bubble[data-role="assistant"]` → `var(--pet-shadow-md)`

不动 transform —— 气泡间距小，translate 易抖动，shadow 渐变更稳。

## 不做

- 不加渐变背景给 user bubble —— 当前 flat accent 与 panel 整体语言一致，渐变会割裂。
- 不动 corner radius asymmetry（"speech bubble"形已建立）。
- 不写测试 —— 纯视觉。

## 验收

- 浅色主题：bubble 默认轻阴影；hover 整行 → 气泡阴影增强（user 偏蓝、assistant 中性）。
- 深色主题：阴影通过 `--pet-shadow-sm/md` 自动加深、可见。
- `npx tsc --noEmit` 通过。
- 复制按钮 / reaction 按钮的现有 hover-only 显隐路径不受影响。

## 完成

- [x] bubbleStyle padding / lineHeight / shadow / transition 更新
- [x] bubble 加 className + data-role
- [x] PanelChat style 块加 hover-tier shadow
- [x] 移到 docs/done/
