# PanelChat 复制按钮 ⇧/Shift 加 [session title · 时间戳] 前缀

## 背景

复制宠物 / 用户消息粘到外部 IM / 文档时常想带上"哪个会话、什么时候发的"的上下文。
当前复制按钮：
- 普通点击：去掉 `「」` ref 装饰
- ⌥/Alt 点击：原样 markdown

缺一个"带元数据"的模式（归档 / share）。

## 改动

- `panelChatBits.tsx`：`CopyableMessage.onCopy` 签名加第四参数 `withMeta: boolean`，按钮 onClick 传 `e.shiftKey`。tooltip 文案补充 ⇧ 说明。
- `PanelChat.tsx`：`handleCopy` 接 `withMeta`；为 true 时在 payload 顶部拼一行 `[${sessionTitle} · YYYY-MM-DD HH:MM]\n` 前缀。⇧ 与 ⌥ 可叠加。
- 新增本地工具 `formatLocalStamp(date)` — 本地时区 minute 粒度。

## 不做

- 不引入 per-message timestamp（ChatItem 没存，要扩 schema + 后端 / persistence，超出本次范围）。timestamp 用复制瞬间的本地 now —— 对"归档"场景反而更直观（"我什么时候拍下这条对话快照"）。
- 不写测试 —— 复制是 UI 串联，formatLocalStamp 是 trivial pad，覆盖意义不大。

## 验收

- 普通点击 / ⌥ 点击行为不变。
- ⇧ 点击 → 剪贴板首行 `[新会话 · 2026-05-12 11:26]`。
- ⇧+⌥ → 首行 meta + 原始 markdown（含 `「」`）。

## 完成

- [x] panelChatBits.tsx
- [x] PanelChat.tsx
- [x] TODO.md 移除该行
- [x] 移入 docs/done/
