# ChatPanel input 长 input chip（iter #507）

## Background

ChatPanel textarea 默认 `overflow: hidden` + `rows={1}` — 输长 prompt
时仅显头几行，owner 失去 textarea 体积感知。容易盲打越界、误以为内
容比实际少。

本 iter 加 **📏 字符徽章** — input.length ≥ 500 时浮 bottom-right
显「Nch」灰字；≥ 2000 时切红 tint 告警 + 加粗 + warning tooltip。短
input 不显避免视觉噪音。

## Changes

### `src/components/ChatPanel.tsx`

紧贴 💡 history button 之后插入 absolute-positioned badge：

```tsx
{input.length >= 500 && (
  <div
    title={`当前 input ${input.length} 字 — 长 prompt 注意：textarea 默认收起 ...`}
    style={{
      position: "absolute",
      bottom: 6,
      right: 12,
      fontSize: 10,
      color: input.length >= 2000
        ? "var(--pet-tint-red-fg)"
        : "var(--pet-color-muted)",
      fontWeight: input.length >= 2000 ? 600 : undefined,
      background: input.length >= 2000
        ? "var(--pet-tint-red-bg)"
        : "var(--pet-color-card)",
      // ...
    }}
  >
    📏 {input.length}ch
  </div>
)}
```

### 两级阈值

- **≥ 500 字**：灰字 muted 「📏 Nch」— hint，owner 知道现在有多少
- **≥ 2000 字**：红 tint + 加粗 — 告警，应考虑拆多条或转 detail.md

## Key design decisions

- **bottom-right 位置**：避开 top-right 既有 chip 集群（📜 right: 64
  / 📋 right: 36 / 💡 right: 8）— 视觉分层 "顶部是动作 / 底部是状态
  metadata"
- **阈值 500 / 2000**：500 是约 1-2 段中文 / 3-4 段英文，typical "稍
  长 prompt" 起点；2000 是 textarea UI 体验明显劣化的拐点（鸡尾酒会
  式 prompt / 长上下文场景）
- **仅显 Nch 不显 max**：本应用无 hard cap 限制（LLM context window
  在后端 truncate）；ch 单位「字符」中英都易理解（vs token 计数需
  tokenizer 知识）
- **`pointerEvents: "auto"` + tooltip**：让 owner hover badge 也能看
  到完整提示，不阻挡 textarea click
- **`userSelect: "none"`**：防 ⌘A 选全部时误选 badge 文本污染复制内容
- **fontFamily mono**：与既有 ts chip / sparkline label 等"小 metadata"
  字体一致 — 数字对齐 visual scan
- **不写 unit test**：纯渲染条件 + 字符串模板；逻辑 trivial。GOAL.md
  "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.30s)
- 后端无改动 — 纯前端 conditional render
- 手测：
  - input 短于 500：无 badge（视觉无变）
  - input ≥ 500 → 「📏 N字符ch」灰字浮 bottom-right
  - input ≥ 2000 → 切红 tint + 加粗 + warning tooltip
  - hover badge → tooltip 显字数 + 长 prompt 建议

## Future iters (out of scope)

- 「token 估算」chip（用 tokenizer 近似）— 当前 ch 已是粗 proxy；引
  tokenizer dep 复杂度收益不匹配
- 长 input 时 textarea 自动展开几行（rows={3}）— UI 变动影响大，单独
  评估
- 「📦 转 detail.md」一键转 chip — 长 input 时浮辅助操作；后续 iter
  评估
