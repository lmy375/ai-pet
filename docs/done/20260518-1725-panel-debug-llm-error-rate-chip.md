# PanelDebug 加「📊 err N%」chip — LLM 错误率（iter #486）

## Background

PanelDebug toolbar 已含 📊 1h tokens / 📊 R·K·S 今日决策 / ⏰ next
consolidate / 🧹 force consolidate 等 audit chip。但缺一个 LLM-pipeline
健康度信号：**错误率**。owner 在 sprint / 调 prompt 后想看「最近 LLM
call 失败几率」时只能去 PanelDebug 顶部「🧪 LLM tools」面板看完整明
细 — 顶部 chip strip 一眼看率更直观。

本 iter 加 chip — 从既有 `llmOutcomeStats` (spoke / silent / error)
state 派生错误率 N%。

## Spec deviation

TODO 原文："扫近 1h llm.log 算 ok / err 比例" — 但 llm.log 仅在成
功响应后 `write_llm_log()` 被调用（错误路径 `?` 提前 propagate），所
以 log 内**没有 error 条目可数**。

可行的「LLM error count」唯一来源是 `LlmOutcomeCounters` 进程内
AtomicU64 — 全 process 累计（启动以来），不能时间窗口化。

本实现使用 process-wide stats 作为最近似的「错误率 audit」信号；tooltip
明示「不能时间窗口化」+ 提供「重置 chip 重测」hint。

## Changes

### `src/components/panel/PanelDebug.tsx`

紧贴既有「📊 1h tokens」chip 之后插：

```tsx
{(() => {
  const total = llmOutcomeStats.spoke + llmOutcomeStats.silent + llmOutcomeStats.error;
  if (total === 0) return null;
  const errPct = Math.round((llmOutcomeStats.error / total) * 100);
  const hasErr = llmOutcomeStats.error > 0;
  return (
    <span
      style={{
        ...
        color: hasErr ? "var(--pet-tint-amber-fg, ...)" : "var(--pet-color-muted)",
      }}
      title={`LLM 进程错误率（进程启动以来累计）：${llmOutcomeStats.error}/${total} = ${errPct}%
· spoke ${...}（真说话）
· silent ${...}（[silent] / 空回复）
· error ${...}（network / API / parse 失败）

注：llm.log 仅存成功条目，错误率不能时间窗口化 — 本 chip 是进程级累计。建议 < 5%；高于 10% 时检查 API key / 网络 / prompt 是否触发 model refusal。可用 PanelDebug 顶部「🧪 LLM tools」面板「重置」按钮清零重测。`}
    >
      📊 err {errPct}%
    </span>
  );
})()}
```

- 复用既有 `llmOutcomeStats` state（已被 get_debug_snapshot polling 自
  动刷新）— 不引新 IPC
- `total === 0` gate：从未跑过 LLM call 时 chip 隐藏避免「📊 err 0%」
  无信息 chip
- `hasErr` ternary：err > 0 时 amber tint 让 owner 第一眼看到「有错」；
  否则 muted 灰

## Key design decisions

- **复用 llmOutcomeStats 而非新建 backend**：既有 atomic counter 已
  cover 三态（spoke / silent / error）— 单 source-of-truth；新建独立
  llm-error log 复杂度高 + 与既有计数器易 drift
- **process-wide vs 1h**：spec 原文要时间窗口；实现做了说明性 deviation
  （llm.log 不存 error 条目 → 不能时间窗口化）。tooltip 明示局限 + 提
  供重置按钮入口让 owner「按需重测」近段时间错误率
- **amber tint when err > 0**：与既有 dueUrgency soon / silent count 等
  amber tint chip 同 family — owner 一眼识别"值得关注"
- **建议 < 5% 阈值**：典型 LLM 错误率（API rate-limit / 网络偶丢 / parse
  edge case）<5%；高于 10% 一般有可诊断根因（key 错 / model refusal
  / prompt 边界）。tooltip 明示让 owner 知道何时该 audit
- **tooltip 含三态明细 + 重置 hint**：让 chip 视觉简（仅 "err N%"）+
  tooltip 含完整 audit 信息 + 操作建议
- **不写 unit test**：纯数据派生 + JSX 字符串拼接；逻辑 trivial（既有
  llmOutcomeStats backend 已 production 验证）。GOAL.md "meaningful
  tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.44s)
- 后端无改动 — 派生自既有 llmOutcomeStats state
- 手测：PanelDebug toolbar 「📊 1h tokens」之后看到「📊 err N%」chip；
  N=0 时 muted 灰，N>0 时 amber tint；hover tooltip 显完整三态明细 +
  阈值 hint + 重置入口；从未跑 LLM call 时（新机刚启动）chip 不渲染
