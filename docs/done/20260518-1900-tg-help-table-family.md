# TG `/help_table [family]` 加 family 详细分支（iter #586）

## Background

iter #582 加了 `/help_table` 全表分组速查表。本 iter 扩展为 `/help_table
[family]` — 加 family 名 → 显该家族详细命令清单 + 一行描述，省 owner
逐 `/help <cmd>` 翻一次性。

## Changes

1. Enum: `HelpTable` → `HelpTable { family: Option<String> }`
2. parser: 接受 title 作 family arg（empty → None）
3. 新 pure `format_help_table_family(family_key: &str) -> String` —
   匹配 family 名 → 输出该 family 详细清单。13 family 全覆盖。
4. `format_help_table_reply_full(family: Option<&str>)` — None 走全表
   现状；Some 走 `format_help_table_family`
5. wrapper `format_help_table_reply()` 保留向后兼容
6. handler 取 `family.as_deref()` 传给 _full
7. tests 更新 + 加 5 个新 case（with_family / pin detail / alias case
   insensitive / unknown fallback / full no-family overview）

## Family alias map

每 family 接受中英 alias：

| canonical | aliases |
|-----------|---------|
| pin | 关注度, 钉 |
| cat | 类目, 活跃度 |
| rename | 重命名, alias |
| idle | stale, 闲置 |
| streak | 连续 |
| find | search, 搜 |
| tag | 标签 |
| speech | 对话, 说话 |
| alarm | mute, 通知, 静音 |
| status | overview, 概览 |
| task | 增删改, edit |
| batch | 危险, 批量 |
| system | 系统 |

`.to_ascii_lowercase()` 让英语 case-insensitive；中文别名走单独 alias
分支。

## Output

```
📚 📌 pin 关注度 家族详细清单
钉住关键 task；与 priority 正交标 owner intent

· /pin <title>
   钉住任务（写 [pinned] marker）
· /unpin <title>
   取消钉住
· /pinned
   列本聊天派单所有钉住 task
...
· /streak_pin
   连续多少天有 pinned task active（attention streak）

相关：/help <cmd>（单命令详细用法）；/help_table（无参全表概览）。
```

## Key design decisions

- **family alias map 硬编码**：与全表硬编码同精神 — curation quality
  vs auto-derivation tradeoff。新命令加时需更新两处（全表 + family
  alias map）
- **未知 family → 列出 available**：友好兜底教学；不静默忽略
- **`format_help_table_reply()` wrapper 保留**：旧测试 / 兼容 caller
  不需改。新 `_full(family)` 是 canonical entry
- **5 unit tests**：parser default (no args) + parser with family +
  parser with 中文 family + format pin detail + alias case insensitive
  (中文 + 大写) + unknown fallback + full no-family overview

## Verification

- `cargo build` clean
- `cargo test --lib` — 1773 pass（新 5 + 既有 1768）
- 三份 drift-defense test all pass

## Future iters (out of scope)

- **family alias auto-completion**：fuzzy match `pi` → `pin`；现严格
  匹配整个 alias。按需 propose
- **registry-derived family map**：每命令加 `audit_family` field 到
  ALL_HELP_TOPICS — formatter 自动 group。需 schema 扩展；中型 refactor
- **`/help_table list`**：仅列 available family 名 + 各家族 1 句描
  述。当前 unknown fallback 部分覆盖；按需独立命令
- **桌面端 PanelMemory「📚」chip click 跳 family list modal**：桌面
  端同 navigation aid — 与 TG /help_table 平行
