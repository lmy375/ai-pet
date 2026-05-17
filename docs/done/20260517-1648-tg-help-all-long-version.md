# TG `/help all` 长版说明书（iter #314）

## Background

TG bot 已有 `/help`（一行一命令的精简全表）和 `/help <cmd>`（单条详细
用法）。但 owner 想"一次拿全本说明书"（学习曲线 / 离线 audit 时把所有
命令的详细文案都看一遍）只能逐条 `/help cancel` / `/help retry` …
30 次 — 没人会这么干。

本迭代加 `/help all` 一次输出全部命令的详细文案（含用法 + 示例 + 相关
命令）。受 TG 4096 字符限制，复用既有 `format_split_chunks` 自动切多
条 TG 消息发送，对 owner 透明。

## Changes

仅 `src-tauri/src/telegram/commands.rs`：

- 新 const `ALL_HELP_TOPICS: &[&str]`（31 条命令名）— 与
  `format_help_for_topic` 单条详情表保 sync 的 single source of truth
- `format_help_for_topic` 函数顶部新 "all" 分支：
  - 输出头 `📚 /help all（长版说明书）`
  - 遍历 ALL_HELP_TOPICS，每条 recurse `format_help_for_topic(name, &[])`
  - 段间用 `\n\n────\n\n` 分隔（视觉断点 + bot.rs format_split_chunks 自然
    切块边界）
- `/help` 详情文案补 `· /help all → 长版说明书` 第三种用法 + `/help all`
  示例
- 5 个新 unit test：
  - 解析 `/help all` → `Help { topic: Some("all") }`
  - all 输出含长版 header + 长度远大于精简全表
  - all 含若干命令详情 anchors（验证拼接生效）
  - all 含至少 N-1 个分隔符（验证 join 逻辑）
  - drift 防护：每个 ALL_HELP_TOPICS 都能拿到非空 detail（保 const 与
    match 表 sync）

bot.rs 无需改动 — `format_split_chunks(reply, TELEGRAM_MSG_LIMIT)` 既
有路径自动把超 4096 char 的回复切多条 TG 消息（带 `(i/n)` 前缀）。

## Key design decisions

- **复用既有 split 路径而非手切**：bot.rs 已有"reply > 4096 → 切块 +
  (i/n) 前缀"完整流水线（line 416-425）。format_help_for_topic 仍返
  单 String，让分块策略集中在 bot.rs 一处 — 未来调 split 阈值 / 加 (i/n)
  样式不必到处改。
- **`────` 分隔符**：4 个 U+2500 BOX DRAWINGS LIGHT HORIZONTAL，全角宽
  度让 TG 等宽渲染时一眼可见。普通 `---` 在 markdown 不渲染时会被字面读
  到效果更弱。
- **ALL_HELP_TOPICS pub const**：与既有 drift-defense 测试列表逻辑同源
  但**不**重构既有测试用列表为引用此 const（避免本 iter scope 扩展过大；
  既有测试已经覆盖"全表 → 详情表" 一致性，本 const 是 "全表 → 长版渲
  染" 的额外 sync 点）。两条测试同失败保护"添加新命令时漏注册"。
- **递归 format_help_for_topic 而非内联表**：避免维护两份 31 项详情字符
  串拷贝；递归在 "all" 分支已 short-circuit（递归参数都不是 "all"），
  不会无限循环。
- **bot.rs 不改**：handler `TgCommand::Help { topic: Some(t) }` 走既
  有 `format_help_for_topic(&t, &custom)` → 返长字符串 → bot.rs 路径
  自动切发。owner 体验是"发 /help all 收到 N 条消息按顺序拼起来读"。
- **`/help` 自身详情更新**：让单条 `/help help` 命令的输出明确列出
  三种用法 + `/help all` 示例 — 让 owner 在 `/help help` 时就发现新功能。

## Verification

- `cargo test --lib`（backend）— 1165 passed / 0 failed（5 新 help_all
  测试通过；drift-defense 各原测试也仍通过）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.25s)
