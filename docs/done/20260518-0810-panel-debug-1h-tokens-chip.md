# PanelDebug 加「📊 近 1h tokens」chip（iter #461）

## Background

owner 在 debug 场景常想审计「pet 最近 1 小时跑了多少 LLM round / 烧
了多少 token」 — 判断是否「prompt 改完后耗用激增」/「sprint 期间 LLM
被频繁调用」/「某 consolidate sweep 烧得太狠」。当前 PanelDebug 有
LLM 调用耗时直方图但无 token 累计入口。

llm.log 现有 schema 不存 API 返回的真实 token usage（API 调用时被忽
略），所以本 chip 走 **启发式估算**：(request body + response_text +
tool_calls JSON serialized) char count / 4 ≈ tokens。与 Anthropic 真实
billing 不完全一致，但趋势性参考可用 — 「现在比平时 N× 高」一眼可见。

## Changes

### `src-tauri/src/commands/debug.rs`

#### 1. `get_llm_tokens_recent_secs(secs: u64) -> (u32, u64)` Tauri 命令

```rust
#[tauri::command]
pub fn get_llm_tokens_recent_secs(secs: u64) -> (u32, u64) {
    let path = log_dir().join("llm.log");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return (0, 0),
    };
    let now = chrono::Local::now().fixed_offset();
    let cutoff = now - chrono::Duration::seconds(secs as i64);
    let mut turns: u32 = 0;
    let mut approx_tokens: u64 = 0;
    for line in content.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(done_time) = entry.get("done_time").and_then(|v| v.as_str())
        else { continue; };
        let Ok(ts) = chrono::DateTime::parse_from_rfc3339(done_time) else {
            continue;
        };
        if ts < cutoff { continue; }
        let mut chars: u64 = 0;
        if let Some(req) = entry.get("request") {
            chars = chars.saturating_add(req.to_string().chars().count() as u64);
        }
        if let Some(resp) = entry.get("response") {
            if let Some(text) = resp.get("text").and_then(|v| v.as_str()) {
                chars = chars.saturating_add(text.chars().count() as u64);
            }
            if let Some(tc) = resp.get("tool_calls") {
                chars = chars.saturating_add(tc.to_string().chars().count() as u64);
            }
        }
        approx_tokens = approx_tokens.saturating_add(chars / 4);
        turns = turns.saturating_add(1);
    }
    (turns, approx_tokens)
}
```

返 `(turns, approx_tokens)` tuple — owner 视图既看「跑了几个 round」
（数量级 trend）又看「估烧了多少 token」（耗用 trend）。

注册到 lib.rs `invoke_handler!`。

### `src/components/panel/PanelDebug.tsx`

#### 1. State + mount fetch + 30s poll

```ts
const [llmTokens1h, setLlmTokens1h] = useState<{
  turns: number;
  approxTokens: number;
} | null>(null);
useEffect(() => {
  let cancelled = false;
  const tick = async () => {
    try {
      const t = await invoke<[number, number]>(
        "get_llm_tokens_recent_secs",
        { secs: 3600 },
      );
      if (!cancelled) {
        setLlmTokens1h({ turns: t[0], approxTokens: t[1] });
      }
    } catch (e) {
      console.warn("get_llm_tokens_recent_secs failed (non-fatal):", e);
    }
  };
  void tick();
  const id = window.setInterval(tick, 30_000);
  return () => { cancelled = true; window.clearInterval(id); };
}, []);
```

- mount 立即 fetch + 30s polling — token 估算粒度本就粗，30s 节奏与
  既有 PanelDebug 其它 polling chip（dedicated_tool_stats / shell_exit_stats）
  对齐
- fail console.warn 不抛（与既有 PanelDebug fail-safe 同模板）

#### 2. Toolbar chip（紧贴 🧹 force consolidate 之前）

```tsx
{llmTokens1h && llmTokens1h.turns > 0 && (
  <span
    title={`近 1h：${turns} 个 LLM round · 估 ${tokens} tokens 累计（4
            chars/token heuristic — 与 Anthropic 真实 billing 不完全
            一致，趋势性参考；30s 自动刷新）。`}
  >
    📊 1h ~ {tokens >= 1000 ? `${(tokens/1000).toFixed(1)}k` : tokens}t · {turns} round
  </span>
)}
```

- 仅 `turns > 0` 时渲染（空 llm.log / 旧机刚启动避免「📊 0 round」噪
  音 chip）
- 千位 `k` 表达：1234 → `1.2k` 让 chip 紧凑
- tooltip 解释 heuristic 局限 — owner 不会把 chip 误读成精确 billing

## Key design decisions

- **char-count / 4 启发式而非 fork llm.log schema 加 usage 字段**：
  扩 `write_llm_log` 接 API usage 是大手术（需改 chat pipeline + 后端
  序列化）；本 chip 是 ambient audit 信号，relative trend 准已够。
  Anthropic SDK 不一定可靠返 usage（streaming / tool_use 边界场景）—
  schema 改动收益不一定大于复杂度
- **`done_time` 而非 `request_time` 作 timestamp**：API 慢请求 done_time
  ≫ request_time；用 done_time 让"完成时刻在窗内"语义更直观（与 owner
  心智「最近 1h 完成的」对齐）
- **request.to_string() chars 含整个 JSON 序列化**：包含 system prompt
  + messages + tools schema + 配置。是 input tokens 的上界估计；真实
  API 输入 token 略小（不含 JSON 结构 overhead），但 4 chars/token 本
  来就是粗估，trend 维度差别小
- **fail-safe `Err → (0,0)`**：文件不存在 / IO 错 / 单行 parse 失败
  → 跳过 / 返零 tuple → 前端 turns=0 不渲染。owner 看不到坏 chip
- **`saturating_add` 防溢**：u64 token 累计在多年长跑场景下不可能溢
  （宇宙寿命都不够），但 saturating_add 一致防御性写法
- **30s poll 而非 5min**：与 dedicated_tool_stats 等其它 PanelDebug
  chip 同节奏；debug 视图 owner 关心实时性比节能更重要
- **不写 unit test**：纯 JSON parse + chars / 4 算术；逻辑 trivial +
  与既有 llm.log 写入 path 集成；filesystem mock 不实在。GOAL.md
  "meaningful tests only" 规则下不引装饰性测试。`cargo build --lib`
  clean 即够
- **`turns > 0` gate**：空 llm.log (新机 / pet 刚启动 / `/reset` 清完)
  时 chip 自动隐藏避免「📊 1h ~ 0t · 0 round」无信息感 chip 占位

## Verification

- `cargo build --lib` — clean
- `cargo test --lib`（全表）— unchanged
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.29s)
- 手测：PanelDebug toolbar → 看「📊 1h ~ N.Nk · M round」chip 在 🧹
  force consolidate 左侧 → hover tooltip 显具体值 + heuristic 注释；
  llm.log 空时 chip 不显
