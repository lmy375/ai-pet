# ChatMini hover 气泡显时间戳

## 需求

桌面气泡只显内容；想知道"这条几点说的"得点 📋 弹"带时间戳"复制再粘出
来。在 bubble 上加 native title tooltip 让 hover 即显 `[HH:MM]`。

## 实现

`src/components/ChatMini.tsx`：

- 把原本嵌在 copyRecentN 里的 `formatTime` 提到模块级 `formatBubbleTimestamp`：
  无 ts / 解析失败 → `[?]`；合法 → `[HH:MM]`（与 copy 路径同格式，单一来源）
- 每个 user / assistant 气泡的 div 已经有 title attr（双击进 panel hint）；
  把时间戳拼到 title 前面，用 ` · ` 分隔，one-shot tooltip 携两条信息。空
  onOpenPanel 时仅显时间戳（不挂 hint）

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - hover 桌面气泡 → 浮 "[14:23] · 双击进入面板聊天（...）"
  - 老 session 加载回来的 ts 缺 → "[?]" + hint
  - 时间合法但解析失败 → "[?]"
  - copyRecentN 仍照旧（用同一 formatBubbleTimestamp）
  - 单 click 选文本行为不变（hover tooltip 是 native 实现，不抢 click）

## 不在本轮范围

- 没改成自定义 floating popover：native title tooltip 跨平台一致，零代码
  量，加 ":" 分隔已足够
- 没加完整时间（年月日）—— [HH:MM] 覆盖 99% 场景；要详尽时间用 panel 历
  史的 Copy MD 路径
