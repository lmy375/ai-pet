# TG bot `/find_in_detail <keyword>` 命令（iter #455）

## Background

TG bot 既有 `/find <keyword>` 按 keyword 子串搜本 chat 派单的 **标题
+ raw_description**。但 detail.md 文件内容（pet 写的进度笔记 / 决策
记录 / 复盘 / 步骤 dump 等）目前没 TG 搜索入口 — owner 想"我笔记里
写过 X" audit 时只能切桌面 PanelTasks 逐条展开找。

本 iter 加 `/find_in_detail <keyword>` — 搜每条 task 的 detail.md
内容，命中后返 status emoji + title + 命中点附近 60 字 snippet。与
/find 互补：
- /find：标题 + raw_description → 适合"我提过 X" / "标题包含 X"
- /find_in_detail：detail.md 内容 → 适合"笔记里写过 X"

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::FindInDetail { keyword: String }` 变体

紧贴 `Find`（同搜索族）。

#### 2. 解析

```rust
"find_in_detail" => Some(TgCommand::FindInDetail { keyword: title }),
```

所有 trailing arg 作 keyword（含空格保留）— 与 /find 同前向兼容。

#### 3. `format_find_in_detail_reply` pure 函数 + `FindInDetailHit` struct

```rust
pub struct FindInDetailHit<'a> {
    pub title: &'a str,
    pub status: TaskStatus,
    pub snippet: String,
}

pub fn format_find_in_detail_reply(hits: &[FindInDetailHit], keyword: &str) -> String;
```

- 空 keyword → usage hint 含 /find 互补提示
- 无 hits + 非空 keyword → "没有 task 的 detail.md 含「kw」" 兜底
- 有 hits → 标题行 `🔬 命中「<kw>」N 条（detail.md 内容搜索）：` + 每
  条 `<emoji> <title>\n   …<snippet>…`
- cap 8（小于 /find 的 10 — 因为每行含 snippet 更长）+ overflow hint

#### 4. `extract_find_in_detail_snippet` helper

```rust
pub fn extract_find_in_detail_snippet(content: &str, kw: &str) -> Option<String>;
```

- byte_idx = content.to_lowercase().find(&kw_lower)?
- byte → char index 转换（防 UTF-8 多字节切到中间）
- 30 字 left + 30 字 right context window
- whitespace flatten（newline / tab / 多空格 → 单空格）让 reply 行可读

Pure 函数 — handler IO 后传 content 进来。

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `Find`：

```rust
TgCommand::FindInDetail { keyword } => {
    if keyword.trim().is_empty() {
        format_find_in_detail_reply(&[], &keyword)
    } else {
        let views = read_tg_chat_task_views(chat_id.0);
        let mut hits = Vec::new();
        // sort: pending → error → done → cancelled
        let mut sorted: Vec<&TaskView> = views.iter().collect();
        sorted.sort_by_key(|v| status_rank(&v.status));
        for v in sorted.iter() {
            if v.detail_path.is_empty() { continue; }
            let content = match memory_read_detail_full(v.detail_path.clone()) {
                Ok(s) => s, Err(_) => continue,
            };
            if let Some(snippet) = extract_find_in_detail_snippet(&content, &kw) {
                hits.push(FindInDetailHit { title: v.title.as_str(), status: v.status, snippet });
            }
        }
        format_find_in_detail_reply(&hits, &keyword)
    }
}
```

设计：
- **handler 处理 IO**：每 task 读 detail.md（`memory_read_detail_full` —
  既有 path-traversal 安全 + 返空字符串兜底）。Pure formatter 仅做字符
  串拼装 — testable
- **sort active first**：与 /find 同 priority — pending / error 浮顶让 owner
  关心的当下相关条目在前
- **skip empty detail_path / Err read / empty content**：3 层 short-circuit
  避免无意义工作
- **没 cap N task 数**：本 chat 派单数典型 50-200，每 detail.md 单文件
  读 + substring → IO 单数毫秒；不必加 cap

### Registry + ALL_HELP_TOPICS + help-for-topic + table line + 2x drift defense

- 双 lang registry 各加一条（紧贴 find）
- ALL_HELP_TOPICS 紧贴 "find"
- format_help_for_topic 加长详细文案 + 与 /find 对比矩阵；同步在
  /find 详细文案末追加 /find_in_detail 交叉引用
- format_help_text 全表加 `/find_in_detail <keyword>` 一行
- 两处 drift-defense 测试列表加 "find_in_detail"

### 10 单元测试

- parse（keyword / 含空格）× 1
- format empty / no-hits / 含 hits / cap-8-overflow × 4
- extract_snippet none-when-no-hit / none-when-empty-kw /
  case-insensitive / newline-flatten / context-window × 5

## Key design decisions

- **snippet 30 char left + 30 char right**：60 字 context window 让 owner
  扫一眼看「这命中出现在什么上下文里」；不足则取边界。比"仅显
  title" 更有用，比"全文 dump" 更紧凑
- **byte index → char index 转换防多字节切坏**：CJK / emoji 每字符 ≥ 2
  bytes；直接 byte slice 可能截到 UTF-8 中间。`char_indices` + 比对让
  hit 落在 char 边界
- **whitespace flatten**：detail.md 含 `\n` / 多空格 / `\t` 拼出来的
  raw context 视觉乱；flatten 单空格让 reply 行清楚
- **cap 8 而非 10（同 /find）**：每行含 snippet（标题 + 换行 + 缩进 +
  context = ~80 字符），8 条已撑满 TG 单消息 ~4KB 经验；过多过密 owner
  看不过来
- **handler IO + pure formatter 分离**：方便单测（formatter 不依赖
  filesystem mocking）；handler 行数少（< 30 行），失败 case 简单
- **复用既有 `memory_read_detail_full`**：path traversal 安全 + 空兜底
  + UTF-8 安全 — 全部 production 验证过的 backend；本命令零新增 IO
  代码
- **不引入 max-tasks cap**：本 chat 派单数典型 50-200 — IO 单数毫秒；
  cap 反而让 owner 困惑「为啥某些条没搜到」。未来 > 500 时再加 cap

## Verification

- `cargo test --lib telegram::commands::tests::find_in_detail` — 5/5 通过
- `cargo test --lib telegram::commands::tests::extract_snippet` — 5/5 通过
- `cargo test --lib`（全表）— 1517 / 1517 通过（+10 from 1507）
- `cargo build --lib` — clean
