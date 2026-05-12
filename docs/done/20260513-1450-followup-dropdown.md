# PanelChat "↩️ follow-up" 短回应下拉

## 需求

iter #233 给 chat 输入栏加了 "📋 模板"下拉（结构化 prompt 起头）。
但对话中接 assistant 后用户常想快速回 "明白了 / 再细说 / 换个例子"
等短回应 —— 当前都得手敲。补 follow-up 下拉。

## 实现

`src/components/panel/PanelChat.tsx`：

- 模块级新 `CHAT_FOLLOWUP_TEMPLATES` 3 条：
  - 👌 明白了：简短致谢确认
  - 🔍 再细说：要细节 + 关键点占位
  - 🔄 换个例子：要换例子
- input 栏新 select "↩️ 回应…"：
  - 仅 `input.length === 0 && items.length > 0` 时浮（已有对话 + input
    空 才出现）
  - 与既有 "📋 模板..." dropdown 同款 select pattern，并排放在 input
    左侧 toolbar
  - onChange prefill + focus textarea + reset placeholder
- 选完后用户可改占位 [关键点] 再发，或直接 Enter 发出

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 新会话刚开 / items 空 → 不显 ↩️ 下拉，仅 📋 模板
  - 发过一句后 + input 空 → 两个 dropdown 都浮
  - 选 "👌 明白了" → input 填"明白了，谢谢。" + focus
  - 选 "🔍 再细说" → 填占位 prompt，用户改 [关键点] 再发
  - 输入框已敲字 → 两个 dropdown 都藏（避免误清）

## 不在本轮范围

- 没让 follow-up dropdown 直接 send（点选即发）：keep prefill 路径让
  用户能微调；如果未来用户反馈"直接发"更快可加 alt-click 直发
- 没让模板可配（localStorage 自定义）：3 条覆盖最高频；与 #233 templates
  同设计哲学
- 没 group 两个 dropdown 成单一 combo：分开更清晰 — 一个是"起话头"，
  一个是"接话茬"，UX 语义不同
- 没集成 reaction 短回应（👍/👎）：reaction 是表态而非文字回应，独立 axis

## TODO 池剩余

- PanelMemory butler_tasks item "⏰ 下次触发：X 后"
- PanelDebug "📥 全部 stash JSON" 按钮
- PanelTasks priority badge 右键菜单
