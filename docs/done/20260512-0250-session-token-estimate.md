# PanelChat session token 估算角标

## 需求

随着 chat 历史变长，prompt 占 context window 越多。LLM 对长 prompt 表现下
降；超出 window 会被截断早期消息。session bar 上加一个小角标显当前 session
估 token 数 + 超阈值时变色警示。

## 实现

`src/components/panel/PanelChat.tsx`：

- 新 `TOKEN_WARN = 8000` / `TOKEN_CRIT = 24000` 常量
- `sessionTokensEstimate` useMemo：累加 `items[].content.length`（仅 string
  content；多模态数组形态略过，本轮粗估优先）+ `currentResponse.length`，
  `/ 4` round 得 token 估
  - `/4` 是 OpenAI tokenizer 平均比的常用粗估；中文偏低估，英文准
  - 与 cl100k_base / o200k_base 实测在 ~20% 误差内
- session bar 内 title 旁加 `<span>` 角标，仅 `>= 100 token` 时显（新会话
  起步无意义）：
  - < TOKEN_WARN：灰底 muted 字 + 默认边
  - WARN ≤ x < CRIT：yellow tint 底 + 黄边
  - >= CRIT：red tint 底 + 红边 + 提示"可能超 context window"
- 文案：< 1000 显纯数字 + " tok"；≥ 1000 显 "X.Yk tok"
- hover tooltip 三档分别详细解释（含建议"开新会话"提示）
- onClick stopPropagation —— 防点 badge 时误触发"展开 dropdown"

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 新建空会话：无 badge
  - 发几条短消息（< 100 token）：badge 不出现
  - 累计 ~500 token：灰底 "~500 tok" 出现
  - 累计 8000+：黄底 + 黄字
  - 累计 24000+：红底 + 红字 + tooltip 警示
  - streaming 中 currentResponse 字符也算入 → badge 实时增长
  - 切到其它 session → badge 数字立即跟随该 session items 重算
  - 点 badge 不展开 dropdown（stopPropagation 拦截）

## 不在本轮范围

- 没用真正 tokenizer（gpt-tokenizer / tiktoken-js 等）：依赖体积 100+KB，
  对实时 useMemo 也偏慢；粗估指标已足够"长度感知"
- 没自动 trim 历史：是用户决定何时开新会话；自动 truncate 会让 LLM 看不
  到关键早期上下文
- 没把多模态图片 token 估算进去：images 的 token cost 由模型定（gpt-4o
  ~85-170 tokens/image），先聚焦文本估算；图片估算可作单独需求

## TODO 池剩余

- PanelPersona mood 强度 mini bar
- ChatMini 拖图到桌面气泡多模态
- PanelTasks task title 双击 inline 编辑
- PanelDebug timeline tab 切换
