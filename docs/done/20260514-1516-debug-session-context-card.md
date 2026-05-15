# PanelDebugStats: 当前会话 LLM 上下文规模卡片

## 背景

TODO（auto-proposed 之前几轮）：

> PanelDebug 加"session LLM context 字数 / token 估算"卡片：与 /reset 配合让用户感知"上下文是否该 reset"。

20260514-1413 上了 PanelChat `/reset` 软清空，20260514-1353 上了输入框 token 估算 chip。两者缺一个"看到全局趋势"的卡片 —— 用户怎么知道"我该 /reset 了吗"？通过观察 input chip 算 input × N 不直观；该有一个直接显示 session 累积 token 的数。本卡片就是。

## 改动

### Backend（Rust）

#### `src-tauri/src/commands/session.rs`

**1. 两个 pub helper**

```rust
pub fn estimate_tokens(s: &str) -> u32 {
    // CJK 字符 ~1 token/字（4E00-9FFF + 假名 + 韩文音节）
    // 非 CJK 非空白 ~1 token/4 字
    // saturating_add 防极端边界（虽然 u32 装得下任何实际 session）
}

fn content_value_text(content: &serde_json::Value) -> String {
    // string content → 原值；
    // multipart array → 拼所有 type=="text" 段 (\n 分隔);
    // image_url 等非 text 段忽略 —— token 估算只看人类可读文本
}
```

`estimate_tokens` 用 saturating add + chars() 迭代，O(n) Unicode-safe；范围与前端 `estimateInputTokens` 同三段（Unified Ideographs / Hiragana-Katakana / Hangul），让前端 chip 和后端卡片"基本对得上数字"。

**2. `get_active_session_context_stats` Tauri command**

```rust
#[derive(Serialize)]
pub struct SessionContextStats {
    pub messages: u32, pub chars: u32, pub tokens: u32,
    pub session_id: String, pub session_title: String,
}

#[tauri::command]
pub fn get_active_session_context_stats() -> SessionContextStats {
    let idx = read_index();
    if idx.active_id.is_empty() { /* 空 stats */ }
    let session = load_session(...)?;
    // 走过 session.messages 计 messages / chars / tokens —— 跳过 role=="system"
}
```

**关键设计**：

- **排除 system**：system 消息（SOUL.md 人设 / 工具说明）是 `/reset` 保留的部分；本数字反映"会被 /reset 砍掉的部分"。完美对齐"看到这数大了就 reset"的决策语义。
- **读失败 → 0 兜底**：返 Result::Ok(空 stats) 而非 Err。卡片是辅助决策，IO 异常时静默退到"干净状态显示"比弹 toast 更友好。
- **跨 frontend / Rust 一致 token 估算**：手写两份算法但同源（CJK +1 / non-WS +0.25），让用户在 PanelChat input chip 和 DebugStats 卡片看到的数对得上。

#### 9 个新单测

- `estimate_tokens_empty_is_zero / cjk_one_per_char / ascii_quarter / whitespace_only_zero / pure_cjk` (5 个)
- `content_text_extract_string / extract_multipart / extract_unknown_returns_empty` (3 个)
- `content_text_extract_multipart` 验"两段 text + 中间夹 image_url" 的拼接行为
- 边界：`estimate_tokens("整理 Downloads")` 验混合 CJK + ASCII = 5 tok

#### `src-tauri/src/lib.rs`

invoke_handler 注册新命令紧贴 `create_session`。

### Frontend（TypeScript）

#### `src/components/panel/PanelDebugStats.tsx`

**1. State + fetch**

```ts
interface SessionContextStats { messages, chars, tokens, session_id, session_title }
const SESSION_TOKEN_WARN_THRESHOLD = 4000;

const [sessionCtx, setSessionCtx] = useState<SessionContextStats | null>(null);
const fetchSessionCtx = useCallback(async () => {
  try { setSessionCtx(await invoke("get_active_session_context_stats")); }
  catch (e) { console.error(...); /* 静默 */ }
}, []);
useEffect(() => {
  void fetchSessionCtx();
  const id = setInterval(() => { fetchData(); fetchSessionCtx(); }, POLL_MS);
  return () => clearInterval(id);
}, [...]);
```

**分离 IPC**：与既有 `get_debug_snapshot` 各自 catch，让任一失败不互拖。5s poll 节奏（同既有 PanelDebugStats）。

**2. 卡片 render（紧贴"陪伴天数"之后）**

- empty (messages === 0)：`—` + muted color。
- 正常：大字 `~N tok` + detail line `~N tok · M 字 · K 条`。
- tokens > 4000：大字 + detail 都转 yellow tint + 追加"考虑敲 /reset 清掉以省 token"提示。
- session_title 在 subtitle 显（截 14 字 + "…"）；title 空时退到通用 subtitle。

**4000 阈值**：常见 LLM context 窗口 8k-128k；4000 留一倍空间给后续对话。

## 不做

- **不接真实 tokenizer**：与 PanelChat input chip 同决策 —— bundle 体积 / 单机性能权衡。±25% 误差对"该不该 /reset"的决策足够。
- **不刷历史 session 的卡片**：仅 active session。多 session 用户需要的是"这 session 该 reset 吗"，不是"哪个 session 最大"。
- **不显 tokens / context_window 占比**：context_window 因 model 而异 + 设置可改。绝对 token 数 + 阈值 yellow tint 已表达紧迫度。
- **不让卡片提供"立即 /reset" 按钮**：DebugStats 是只读窗口（与 PanelDebug 同模式）；reset 在 PanelChat 走 slash 命令更明确。
- **不动 PanelDebug strip**（chip-row 已有同等紧凑显示器）—— 这卡片是"统计 tab 长形态"，与 strip 视觉分级。

## 验证

- `cargo test --lib commands::session` ✓ 21 / 21 通过（含 9 个新增）
- `cargo test --lib` ✓ **987 / 987 通过**（978 → 987）
- `cargo check` ✓ 0 error
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.21s

## TODO 状态

- 本轮实现 1 条
- TODO 剩 1 条：ChatMini 跳到 Panel deeplink

## 后续

- session_index 的 active_id 与 PanelChat in-memory `sessionId` 偶尔短暂不同步（用户切 session 瞬间）；本卡片显的可能是上一 session 数字。为前端引入"事件驱动重抓"（PanelChat switchSession 时 emit `session-switched` 让 DebugStats listen 后立即 fetch）让数字跟手。
- 历史 session token 排行 top 5（power user 想知道哪些 session 最大需要清）。
- 当前模型的真实 context 窗口（从 settings.model 推断）显占比 N/W。
