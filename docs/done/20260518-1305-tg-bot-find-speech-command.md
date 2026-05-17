# TG bot `/find_speech <keyword>` 命令（iter #474）

## Background

TG 搜索族已含 `/find`（任务标题 / 描述）+ `/find_in_detail`（detail.md
内容）。但缺一条：**搜 pet 说过的话**。owner 想「pet 之前提过 X 吗 /
上次怎么说这件事」时只能切到桌面 PanelDebug 滚 speech_history list 找。

本 iter 加 `/find_speech <keyword>` — 扫 speech_history.log 全文 +
case-insensitive 子串过滤 + 抽 ts + 60 字 snippet。与 `/last_speech`
（最近 1 条）对偶 — 那个单点 audit，本命令跨时段搜索 audit。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::FindSpeech { keyword: String }` 变体

紧贴 `FindInDetail`（同搜索族）。

#### 2. 解析

```rust
"find_speech" => Some(TgCommand::FindSpeech { keyword: title }),
```

所有 trailing 作 keyword（含空格保留），与 /find / /find_in_detail 同
模板。

#### 3. `format_find_speech_reply` pure 函数

```rust
pub fn format_find_speech_reply(hits: &[(String, String)], keyword: &str) -> String;
```

- 入参 `hits` 是 `(ts_local_label, snippet)` tuples — handler 已把
  RFC3339 ts 转 `MM-DD HH:MM` 本地串 + 抽 60 字 snippet
- 4 态：
  - 空 keyword → usage hint
  - 非空 keyword + 空 hits → 「🗣 speech_history 内没有命中『kw』的话」
  - 有 hits → 「🗣 speech 命中『kw』N 条：\n· MM-DD HH:MM · …<snippet>…」每行
  - overflow（> 8）→ 「…还有 K 条命中（关键词太宽？试更精确的词）」

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `Tag` 之前：

```rust
TgCommand::FindSpeech { keyword } => {
    if keyword.trim().is_empty() {
        format_find_speech_reply(&[], &keyword)
    } else {
        let content = read_history_content().await;
        let kw_lower = kw.to_lowercase();
        let mut hits = Vec::new();
        for line in content.lines().rev() {  // reverse — 最新 hit 在前
            if line.is_empty() { continue; }
            let Some((ts_str, _)) = line.split_once(' ') else { continue; };
            let text = strip_timestamp(line);
            if !text.to_lowercase().contains(&kw_lower) { continue; }
            let ts_label = chrono::DateTime::parse_from_rfc3339(ts_str)
                .map(|t| t.with_timezone(&chrono::Local).format("%m-%d %H:%M").to_string())
                .unwrap_or_else(|_| ts_str.to_string());
            let Some(snippet) = extract_find_in_detail_snippet(text, &kw) else { continue; };
            hits.push((ts_label, snippet));
        }
        format_find_speech_reply(&hits, &keyword)
    }
}
```

- **handler 处理 IO + filter**：reuses `read_history_content().await`
  (production-validated) + `strip_timestamp(line)` 拆 ts/text
- **reverse iteration**：最新 hit 在前 — owner 通常关心近期 utterance
- **`extract_find_in_detail_snippet` 复用**：iter #455 helper — 抽 60 字
  context window + flatten whitespace；同 source-of-truth 让两 search 命
  令 snippet 视觉一致
- **ts 转本地 `MM-DD HH:MM`**：让 TG reply 紧凑可读；parse 失败兜底返
  原 ts 不抛错

### Registry + ALL_HELP_TOPICS + help-for-topic + table line + 2x drift defense

- 双 lang registry 各加一条（紧贴 find_in_detail）
- ALL_HELP_TOPICS 紧贴 "find_in_detail"
- format_help_for_topic 加详细文案 + /find_in_detail / /last_speech 交叉
  引用
- format_help_text 全表加 `/find_speech <keyword>` 一行
- 两处 drift-defense 测试列表加 "find_speech"

### 5 单元测试

- parse（keyword / 含空格）× 1
- format（empty / no-hits / hits with ts+snippet / cap-8-overflow）× 4

## Key design decisions

- **snippet 复用 `extract_find_in_detail_snippet`**：两 search 命令的
  snippet 协议（30 字 left + 30 字 right + whitespace flatten + char-
  boundary safe）完全一致 — 一处算法两处用减 drift
- **handler reverse iteration**：speech_history.log 是 append-only
  oldest-first；reverse 让 hit 列 newest-first 符合 owner 期待
- **ts 转本地 `MM-DD HH:MM` 而非保 ISO**：TG 行宽紧凑；owner 看「5/17
  14:30」秒懂；RFC3339 全串占 25+ 字符
- **cap 8（与 /find_in_detail 同）**：snippet 每行 ~80 字 + ts；8 条已
  撑满 TG 单消息 ~4KB 经验
- **handler IO + pure formatter 分离**：formatter testable（不 mock
  filesystem）；handler 仅 stitching — 错误兜底 in handler
- **不写 unit test on async handler**：handler 是 read + filter +
  stitch，simple wiring；formatter + extract_find_in_detail_snippet 已
  cover 主算法 corner cases。GOAL.md "meaningful tests only" 规则下
  不引装饰性 handler test

## Verification

- `cargo build --lib` — clean
- `cargo test --lib telegram::commands::tests::find_speech` — 5/5 通过
- `cargo test --lib`（全表）— 1548/1548 通过（+5 from 1543）
