# ChatMini bubble 列表底部「💰 session token tally」status line（iter #561）

## Background

ChatMini 已有两层 token 视觉：

- 🌡️ context 健康 mini bar — 仅 20%-100% threshold 区段显（avoid empty
  session noise）
- 💭 CTA reset chip — 仅超 threshold（4000 tok）时显

但 0%-20% 早期 zone 完全无信号 — owner 没法看「这个 session 已经聊
了多少 token」直到撞 20%。也没 cost 维度，token 数字本身对 owner
（非 LLM 工程师）不够 actionable。

TODO 要求「session token tally status line：bubble 下显累计 in/out
token + 估算 cost」。

## Data substrate gap

backend `estimate_tokens` 是按 4 chars/token 单总数估，**无 input /
output 拆分**：

- `commands::session::estimate_tokens(s: &str) -> u32`
- session_summary 单字段 `tokens: u32`

So 「in/out 拆分」literal 要求需先 backend lift（API 层分别 sum）—
范围外。本 iter 用 cost 估算填补「数字 → actionable signal」缺口，
in/out 拆分留 future iter。

## Change

ChatMini.tsx：

1. 新常量 `MINI_TOKEN_COST_PER_MILLION = 3.0`（USD per 1M tok，
   blended rate — Claude Sonnet $3 input / $15 output 之间偏 input
   的中点；Opus 更贵 / Haiku 更便宜，本常量算 mid-band 估算）
2. bubble 列表底部（tool 状态行下方、pet-mini-chat 关闭前）插
   status 行：

```tsx
{sessionTokens !== undefined && sessionTokens > 0 && (() => {
  const costUsd = (sessionTokens * MINI_TOKEN_COST_PER_MILLION) / 1_000_000;
  const costLabel = costUsd < 0.01
    ? `<$0.01`
    : `≈ $${costUsd.toFixed(costUsd < 1 ? 3 : 2)}`;
  return (
    <div style={{ marginTop: 4, paddingTop: 4, borderTop: "1px dashed …",
                  fontSize: 10, color: muted, fontFamily: mono,
                  display: "flex", alignItems: "center", gap: 6 }}>
      <span>💰</span>
      <span>~{sessionTokens} tok · {costLabel}</span>
    </div>
  );
})()}
```

## Key design decisions

- **任 `sessionTokens > 0` 即显**：填补 0-20% 早期 zone — 顶部
  🌡️ bar 没显但 owner 已聊了几轮，仍能看到 ambient 信号
- **底部位置（not 顶部）**：与既有 🌡️ bar / 💭 CTA chip 互补，不
  同视觉位置避免叠加噪音。owner 视线自然从 bubble 末端往下扫看到
  tally，逻辑顺序「这是你刚聊的；总账如下」
- **dashed border-top 隔断**：visual separator 让 tally 与 bubble
  内容明显分离，强调"meta 信息 not 一条 message"
- **cost format 三档**：< $0.01 → `<$0.01`（避免显 0.000…）；
  < $1 → 3 decimal（$0.024）；≥ $1 → 2 decimal（$1.32）。tabular-nums
  对齐
- **`3 chars/token cost rate` 单常量**：MINI_TOKEN_COST_PER_MILLION
  顶部常量，owner 想精算改这个数即可。3.0 是 Claude Sonnet input rate；
  Opus 用户 / Haiku 用户改成 15 / 0.8 各自 mid-band
- **tooltip caveats**：说明 (1) 含 system + 历史 turns 全 context
  count（不仅最近一轮）；(2) 按 4 chars/token 估；(3) 不区分
  input/output；(4) 仅供 ambient awareness — 精确账单看 API console。
  honest disclosure 避免 owner 把 ChatMini 当账单工具
- **不动既有 🌡️ bar / 💭 CTA**：本 iter 仅加新层 — 既有两层视觉
  逻辑（progress / CTA）已熟，无需 churn

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — 单 status 行加在熟悉位置（与 tool status 行同
  fragment），无 layout race / scroll race。color / font / spacing 与
  既有 muted helper 行一致
- 无新 lib test — UI 渲染纯 React + 已有 `sessionTokens` prop

## Future iters (out of scope)

- **input/output 拆分**：backend `estimate_tokens` 改成 (input_tok,
  output_tok) 双字段；session_summary 同步；前端 status 显
  `📥 ~N in · 📤 ~M out · ≈ $X`。需 backend schema 调整 — 中型 lift
- **真实 API token 接入**：API response 里有精确 `usage.input_tokens`
  / `usage.output_tokens` — 比 4 chars/token 估准。需读 chat_completion
  response 路径写到 session metadata
- **per-message cost chip**：bubble hover 显「本条 ≈ $X」— 让 owner
  看哪一轮特别贵（如长 tool result 输出）。需 per-message token 数
  metadata；in/out 拆分实现后顺手做
- **可配置 rate（user settings）**：owner 切 model 时 rate 切。先观察
  owner 是否真的需要 — 改个常量更轻量
