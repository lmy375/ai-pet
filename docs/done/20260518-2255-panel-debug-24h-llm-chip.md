# PanelDebug 加「📊 24h LLM」chip（iter #509）

## Background

PanelDebug 顶部 chip 行已有「📊 1h ~Xt · N round」chip（line 2693）—
扫 llm.log 最近 1h 累计估 tokens + round 数，audit「现在 LLM 节奏」。

但缺**daily 视角** — owner 写 sprint review / 给同事报「今天我消耗
了多少」 / audit 今日 LLM 总量时只有 1h 窗口，要瞄出全天累计就要多
次刷新 + 心算。

本 iter 加 sibling chip「📊 24h ~Xt · N round」— 同 Tauri 命令
`get_llm_tokens_recent_secs` 但 secs=86400，给 daily 累计视角。click
复制单行报告到剪贴板。

## Changes

### `src/components/panel/PanelDebug.tsx`

#### 新增 state `llmTokens24h`（line ~1138）

```tsx
const [llmTokens24h, setLlmTokens24h] = useState<{
  turns: number;
  approxTokens: number;
} | null>(null);

useEffect(() => {
  let cancelled = false;
  const tick = async () => {
    try {
      const t = await invoke<[number, number]>(
        "get_llm_tokens_recent_secs",
        { secs: 86_400 },
      );
      if (!cancelled) {
        setLlmTokens24h({ turns: t[0], approxTokens: t[1] });
      }
    } catch (e) { ... }
  };
  void tick();
  const id = window.setInterval(tick, 30_000);
  return () => { cancelled = true; window.clearInterval(id); };
}, []);
```

与 1h chip 完全平行 — 仅 secs 不同。

#### Chip 渲染紧贴 1h chip 之后（line ~2715）

```tsx
{llmTokens24h && llmTokens24h.turns > 0 && (
  <span
    onClick={async () => {
      const tokensLabel = ...;
      const line = `24h: ${llmTokens24h.turns} LLM rounds · ~${tokensLabel} tokens`;
      await navigator.clipboard.writeText(line);
    }}
    style={{ ...same as 1h chip... }}
    title={...含 1h vs 24h 互补说明 + click 复制 hint}
  >
    📊 24h ~Xt · N round
  </span>
)}
```

差异点：
- **click 复制单行报告**：1h chip 是 `cursor: help`（仅 tooltip）；24h
  这条增加 clipboard action — owner 写 sprint review 时一键拿 line
- **tooltip 互补说明**：明确「1h = 现在节奏 / 24h = 今日累计」语义分
  工，避免 owner 困惑「两个 chip 都 token 累计有啥不一样」

## Key design decisions

- **同 IPC `get_llm_tokens_recent_secs` 不引新 Tauri 命令**：后端已
  param 化 secs；前端只需 `secs: 86_400` — 零后端改动
- **30s poll 同 1h**：daily 累计 30s 内变化微小但同步刷新保 chip 间
  数据一致（owner 不会看到「1h: 12 round, 24h: 10 round」短暂不一致）
- **click 复制而非 hover help**：1h 看实时无需 copy 入口；24h 是
  audit / 报告场景，clipboard action 给得起
- **`turns > 0` gate**：与 1h 同 — 空集时不渲染避免视觉噪音
- **位置紧贴 1h**：信息层次「实时（1h）→ 累计（24h）→ 错误率（err%）
  → 决策（R·K·S）」一条 progressive disclosure 行
- **不写 unit test**：纯 IPC poll + render 条件；逻辑 trivial（既有 1h
  chip 同 pattern production 验证）。GOAL.md "meaningful tests only"
  规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.26s)
- 后端无改动 — 复用既有 `get_llm_tokens_recent_secs` IPC
- 手测：
  - PanelDebug 顶部 chip 行：1h chip 旁出现「📊 24h ~Xt · N round」
  - hover → tooltip 显累计数 + 1h vs 24h 互补说明
  - click → 控制台 log「📊 已复制 24h 用量：24h: N rounds · ~Xt tokens」
    + 粘贴板含该 line

## Future iters (out of scope)

- 「7d」/「30d」窗口 chip — 当前 LLM logs 通常按天 rotation；长窗口需
  改后端 scan 策略，单独 iter 评估
- 真 billing 视图（token 单价 × N）— 各模型单价不同；当前 heuristic
  已是趋势性参考
