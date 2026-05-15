# PanelChat 输入框 token 估算 chip

## 背景

TODO（auto-proposed 之前）：

> PanelChat 输入框 token 计数 chip：实时显当前 input 估算 token 数（÷ 2 字数法）让用户有"我打了多长" 直觉。

跑付费模型（GPT-4 / Claude）的用户对"这条消息要烧多少 token"敏感；目前 input 区只显字符数（在某个隐藏处），没有 token 直观估算。chat 输入框加 chip 让"我打了多长"成为一眼可见的信号。

## 改动（frontend only）

### `src/components/panel/PanelChat.tsx`

**1. 纯函数 `estimateInputTokens(s)`（顶层 export，便于未来 vitest pin）**

```ts
export function estimateInputTokens(s: string): number {
  if (!s) return 0;
  let cjk = 0;
  let other = 0;
  for (const ch of s) {
    const code = ch.codePointAt(0) ?? 0;
    // CJK Unified Ideographs (4E00–9FFF) + 假名 (3040–30FF) + Hangul (AC00–D7AF)
    const isCJK =
      (code >= 0x4e00 && code <= 0x9fff) ||
      (code >= 0x3040 && code <= 0x30ff) ||
      (code >= 0xac00 && code <= 0xd7af);
    if (isCJK) cjk++;
    else if (!/\s/.test(ch)) other++;
  }
  return Math.ceil(cjk + other / 4);
}
```

**为什么这个公式**：
- CJK 字符 ~1 token 各家 tokenizer 都差不多（汉字 BPE 通常落在 1-2 之间，~1 是保守低估）。
- 非 CJK 非空白 ~1 token/4 字 = 经典 GPT BPE 经验值。
- 空白不算 —— 多数 tokenizer 把"前导空格 + word"合一，单纯空格几乎不增 token。
- ±25% 误差不影响"30 vs 3000"的决策；想精确得后端按真实 tokenizer 算，太重。

只算 input —— chip 是"当下这条"的直觉感知，不是实际 LLM context 计费器（system + history 都要算才精确，但那不是用户每键想看的数字）。

**2. chip render**

紧贴现有 `historyCursor !== null` 提示渲染，分别占 input bar 顶部的左右两端，互不挡：

```tsx
{input.length > 0 && (() => {
  const tokens = estimateInputTokens(input);
  return (
    <div
      style={{
        position: "absolute",
        top: -22, left: 16,
        fontSize: 10, padding: "2px 8px",
        background: "var(--pet-color-card)",
        border: "1px solid var(--pet-color-border)",
        borderRadius: 4,
        color: "var(--pet-color-muted)",
        whiteSpace: "nowrap",
        fontFamily: "'SF Mono', 'Menlo', monospace",
      }}
      title={`粗略估算输入的 token 数：CJK 1 token/字 + 其它 ~4 字/token。准确值因模型而异...\n\n当前 ${input.length} 字 → ~${tokens} tok`}
    >
      ~{tokens} tok
    </div>
  );
})()}
```

视觉与既有"↑ 历史 N / M" chip 完全一致（同 font / border / color），让 input bar 顶部的两类提示 chip 风格统一。pointerEvents auto（与 history 的 none 不同）让 hover tooltip 真能弹出（title attr 需要 cursor 不穿透）。

## 不做

- **不算 session-level 总 token**：scope creep。聊很久后再看"整段花了多少"是周期性查询，不该每键算一次。如需可作为独立的 PanelDebug 卡片或 status footer。
- **不接真实 tokenizer**：tiktoken-js / @anthropic-ai/tokenizer 都要 ~200KB+ bundle 体积，本 chip 不值这个 trade-off。
- **不动 ChatPanel（桌面 mini chat 输入框）**：手机大小的输入框塞 chip 会挤；桌面用户感知 token 通常发生在长 prompt 场景（PanelChat 大编辑器）。
- **不写测试**：前端无 vitest；helper 是 15 行的纯字符遍历，含 export 让将来一接入 vitest 就能直接 pin。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.15s
- 改动 ~50 行（helper 25 + chip 25），既有 input 行为 / history 提示 / 字数显示等都不动。

## 后续

- 接入精确 tokenizer 作为可选模式（用户在 settings 里勾"精确 token 计数"，按需 lazy-load `tiktoken`）。
- session-level cumulative tokens：在 settings DB stats 旁加一项"当前 session 估算 LLM context"，与本 chip 协同。
- 超阈值（如 > 4000 tok）chip 变橙色警示"快到上下文边界了"。
