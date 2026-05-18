# PanelDebug 「📊 7d LLM sparkline」chip（iter #529）

## Background

PanelDebug chip 行已有 1h / 24h LLM 数字 metric chip（iter #509 / 既有）。
但缺 **visual 趋势** — owner 想看「这周节奏分布如何 / 哪天大量调用 /
哪天 idle」时只能切到 1h chip 等 7 次 + 心算。

本 iter 加 mini 7-bar sparkline 显近 7 天每日 LLM round 数 — 与
PanelTasks 30 天 task sparkline 同视觉风格。

## Changes

### `src-tauri/src/commands/debug.rs`

新 Tauri 命令 `get_llm_calls_per_day(days: u32) -> Vec<u32>`：

```rust
#[tauri::command]
pub fn get_llm_calls_per_day(days: u32) -> Vec<u32> {
    let n = days.clamp(1, 30) as usize;
    // 读 llm.log → parse 每行 done_time → bucket 按本地日历日
    // buckets[0] = 最旧日；buckets[n-1] = 今日
    ...
}
```

- clamp 1..=30 防极端调用
- bucket 按 **本地日历日** — 与 PanelTasks/PanelMemory「今日」/「昨日」
  filter 同 timezone 协议
- 左旧右新顺序 — 与既有 sparkline 视觉约定一致

注册到 `lib.rs` invoke handler 表。

### `src/components/panel/PanelDebug.tsx`

#### 新 state `llmCalls7d` + 30s poll

```tsx
const [llmCalls7d, setLlmCalls7d] = useState<number[] | null>(null);
useEffect(() => {
  let cancelled = false;
  const tick = async () => {
    try {
      const buckets = await invoke<number[]>("get_llm_calls_per_day", { days: 7 });
      if (!cancelled) setLlmCalls7d(buckets);
    } catch (e) { /* non-fatal */ }
  };
  void tick();
  const id = window.setInterval(tick, 30_000);
  return () => { cancelled = true; window.clearInterval(id); };
}, []);
```

30s poll 与 1h / 24h chip 同节奏 — 数据刷新一致避免「1h 显新但 7d 显
旧」诡异。

#### Chip 渲染（紧贴 24h chip 之后）

```tsx
{llmCalls7d && llmCalls7d.length === 7 && (() => {
  const total = llmCalls7d.reduce((a, b) => a + b, 0);
  if (total === 0) return null;
  const max = Math.max(...llmCalls7d, 1);
  const labels = ["6天前", "5天前", "4天前", "3天前", "前天", "昨天", "今天"];
  return (
    <span
      onClick={async () => {
        const line = `7d LLM rounds: ${llmCalls7d.join("/")}（6天前→今天，总 ${total}）`;
        await navigator.clipboard.writeText(line);
      }}
      title={"7 days breakdown..."}
      style={{ dashed chip + flex inline + sparkline bars }}
    >
      📊 7d <sparkline bars /> {total}
    </span>
  );
})()}
```

- 7 bar inline mini-chart（同 PanelTasks 30 天 sparkline 视觉 — 3px
  宽 / max 归一 / tint-blue-fg）
- 总数显在 chip 末让 owner 一眼看「这周共多少 LLM round」
- click 复制 line 含每日数 + 总数

## Key design decisions

- **新 IPC vs 复用 secs 接口**：理论可调 `get_llm_tokens_recent_secs`
  7 次（每次 24h cutoff 算不同时段）— 但需 7 次 fs::read_to_string +
  7 次 line scan = 7x cost。新 IPC 单次扫一遍 log + bucket — 更省。
  且 daily semantic（按本地日历日）clamping 在后端做更准（前端 JS
  Date.toLocaleDateString 各浏览器实现可能微差）
- **clamp 1..=30**：防极端调用 — 30d 范围内 llm.log 单次扫成本可控；
  31d+ 应该走专门的 archive 视图（未来 iter）
- **左旧右新视觉**：与 PanelTasks 30 天 sparkline 同约定 — owner 心智
  「最近在右」一致
- **labels 用「N天前」**：与 ChatMini ts chip 相对时间表达一致 — 中
  文 idiomatic「前天 / 昨天 / 今天」自然
- **`turns > 0` gate**：与 1h / 24h chip 同 — 0 时不渲染避免空状态
- **click 复制 line**：与 24h chip 同 clipboard pattern — 写 weekly
  review / 同事 ping 场景
- **bar 高度 max 归一**：与既有 30 天 sparkline 同算法 — 反映「自身节
  奏」而非跨 chip 比较；高 bar 是「这周高峰」而非「总活跃度」
- **不写 unit test**：纯 React useState + render 条件 + clipboard
  write；backend 新 IPC 逻辑 trivial（与既有 get_llm_tokens_recent_secs
  同 line scan + bucket pattern）。GOAL.md "meaningful tests only" 规则
  下不引装饰性测试

## Verification

- `cargo build`（src-tauri）— clean
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.35s)
- 手测：
  - PanelDebug 顶部 chip 行：1h chip 之后 24h chip → 之后看「📊 7d
    <bars> N」chip
  - hover → tooltip 显 7 天 breakdown + 「click 复制」hint
  - click → 控制台 log 「📊 已复制 7d sparkline：7d LLM rounds: 0/2/5/
    8/12/15/9（6天前→今天，总 51）」+ 剪贴板含 line
  - 长时间 idle 后总数 0 → chip 隐藏（gate 验）

## Future iters (out of scope)

- 「📊 30d」更长视角 sparkline — backend 已支持 clamp ≤ 30；前端 chip
  本可加，但 30 bar 在 chip 行内可能挤；后续考虑独立 popover
- 「按 outcome 分色 sparkline」（spoke / silent / error 不同色叠加）—
  当前 binary count 已能反映节奏；颜色分维度复杂度收益不匹配
- 「点 bar 跳到当日 llm.log」— 当前 click 是整 chip 复制；细粒度交互
  后续 iter
