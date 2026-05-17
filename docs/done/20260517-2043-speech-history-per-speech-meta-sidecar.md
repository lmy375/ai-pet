# speech_history per-speech meta sidecar（iter #389）

## Background

iter #384 加 PanelDebug ⏰ 最近开口 chip 时把"为何开口"半边 deferred —
speech_history.log entry 只有 `<ts> <text>` 单行，无触发 meta。要做
per-speech feedback_band / cooldown_factor / mode audit 需扩 schema。

直接改 entry shape 会触 6+ 个既有 reader（parse_recent / strip_timestamp
/ speeches_for_date / detect_repeated_topic / hourly_counts_for_date /
classify_speech_register）— 太破坏。本 iter 采用 **sidecar JSONL**
方案：新 `speech_meta.jsonl` 文件，每行 1 条 meta entry by ts；既
有 readers 零改动；新 reader 按 ts join 即可。

## Changes

### `src-tauri/src/speech_history.rs`

#### 1. 新 `SpeechMeta` 结构 + sidecar 路径 + IO

```rust
pub struct SpeechMeta {
    pub ts: String,           // 与 speech_history.log 的 ts 对齐 join key
    pub band: String,         // feedback_band
    pub factor: f64,          // feedback_factor
    pub mode: String,         // proactive companion_mode
    pub deadline_factor: f64, // 0.5 / 1.0
}

fn meta_path() -> Option<PathBuf>; // ~/.config/pet/speech_meta.jsonl
async fn record_meta(&SpeechMeta) -> io::Result<()>; // append + trim
                                                     // 到 SPEECH_HISTORY_CAP
pub fn parse_meta_index(content: &str) -> HashMap<String, SpeechMeta>;
```

文件大小 cap 200KB，行数 cap SPEECH_HISTORY_CAP（与 speech log 同
保对齐）。

#### 2. 写路径分两条

```rust
pub async fn record_speech(text: &str)            // 旧 API: 仅写 speech
pub async fn record_speech_with_meta(             // 新 API: 写 speech + sidecar meta
    text: &str,
    mut meta: SpeechMeta,
)
```

`record_speech_inner` 重构接受 `Option<(ts, meta)>` — meta 走 sidecar
write，speech 走 speech_history.log（ts 同源避免漂移）。两路径都
best-effort（meta 写失败不阻塞 speech 写）。

#### 3. 读路径

```rust
pub async fn recent_speeches_with_meta(n) -> Vec<RecentSpeechEntry>
#[tauri::command]
pub async fn get_recent_speeches_with_meta(n) -> Vec<RecentSpeechEntry>
```

RecentSpeechEntry = { ts, text, meta: Option<SpeechMeta> }。读 speech
log + meta jsonl 各一次，按 ts 在内存 join。缺 meta 的旧 entry →
`meta: None`，frontend tolerant 显仅 text。

### `src-tauri/src/proactive.rs`

#### 1. 新 helper `record_speech_with_current_meta(text)`

```rust
async fn record_speech_with_current_meta(text: &str) {
    let recent_fb = feedback_history::recent_feedback(20).await;
    let urgent_count = compute_urgent_butler_count();
    let meta = match build_cooldown_breakdown(&recent_fb, urgent_count) {
        Some(b) => SpeechMeta {
            ts: String::new(),
            band: b.feedback_band,
            factor: b.feedback_factor,
            mode: b.mode,
            deadline_factor: b.deadline_factor,
        },
        None => /* insufficient_samples 兜底 */,
    };
    record_speech_with_meta(text, meta).await;
}
```

复用既有 `build_cooldown_breakdown`（与 ToneStrip 当前态 chip 同
算法）确保 meta 与 panel UI 同源。proactive disabled 时 None →
"insufficient_samples" 兜底（罕见：speech 写时 proactive 一定
enabled）。

#### 2. 两 record_speech 调用 → record_speech_with_current_meta

- run_proactive_turn 主路径（line 1792）
- morning briefing 路径（line 2412）

### `src-tauri/src/lib.rs`

注册 `get_recent_speeches_with_meta` Tauri command。

### `src/components/panel/PanelDebug.tsx`

#### 1. state + 30s 轮询

```ts
const [speechMetaByTs, setSpeechMetaByTs] = useState<Record<string, SpeechMetaEntry>>({});
useEffect(() => {
  // 每 30s invoke get_recent_speeches_with_meta(50) → Record<ts, meta>
  // speech 写入低频 — 与 debug snapshot 1s 轮询分开节省 IPC
}, []);
```

#### 2. iter #384 chip tooltip 拼 meta

```ts
const meta = ts ? speechMetaByTs[ts] : undefined;
const metaLine = meta
  ? `\n\n触发上下文：feedback_band=${meta.band} (×${meta.factor.toFixed(1)})${meta.mode ? ` · mode=${meta.mode}` : ""}${meta.deadline_factor < 1.0 ? ` · ⚡ deadline 紧迫 (×${meta.deadline_factor.toFixed(1)})` : ""}`
  : "";
title={`${tShort} (${ageLabel})\n\n${text}${metaLine}\n\n点击切...`}
```

缺 meta 的旧 entry → metaLine 为空字符串，tooltip 仅显 ts + text（与
iter #384 行为兼容）。

### Tests

新 4 个 unit test：
- parse_meta_index 空输入 → 空 map
- parse_meta_index valid JSONL → 多条解析
- parse_meta_index 容错跳 malformed 行
- parse_meta_index 同 ts 后写覆盖（HashMap insert 语义）

backend 总 1360 passed（既有 1356 + 4 新）。

## Key design decisions

- **sidecar JSONL vs 扩 entry shape**：zero-break 既有 readers — 6+
  解析路径不动。trade-off：两文件 1:1 同步靠 ts join，meta 缺失
  fallback 优雅；vs 同 entry 扩 schema 需重写所有 parser。
- **record_speech 保留 + 新 API record_speech_with_meta**：向后兼
  容；caller 没 meta 上下文也能写（未来 reactive chat 等扩展）。
- **build_cooldown_breakdown 复用而非新算 band/factor**：与 ToneStrip
  "当前态" chip 同算法 — owner 看 ToneStrip 与 chip tooltip 数值
  一致不困惑。
- **proactive disabled fallback meta**：build_cooldown_breakdown 返
  None 时（proactive disabled / cooldown=0）写 "insufficient_samples"
  + 1.0 兜底 — speech 写时不该走到此分支，但 defensive 防 None 崩。
- **30s 轮询频率**：speech 是低频事件（cooldown 默认 ≥ 60s）；30s
  足够新鲜。比 debug snapshot 1s 节奏稀疏 10×，节省 IPC。
- **n=50 拉取**：与 SPEECH_HISTORY_CAP 一致，让 PanelDebug 看到全
  历史窗口的 meta。chip 自己只显前 5 条但 meta map 全在内存方便
  未来扩。
- **frontend tolerant 缺 meta**：旧 entries / 写 meta 失败 → tooltip
  优雅 fallback 仅 text，不报错。

## Verification

- `cargo check`（backend）— clean
- `cargo test --lib`（backend）— **1360 passed / 0 failed**（+4 新
  parse_meta_index test）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.23s)
