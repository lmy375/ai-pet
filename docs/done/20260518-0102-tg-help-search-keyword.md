# TG bot `/help search <kw>` 命令（命令自助搜索）（iter #344）

## Background

owner 在 TG 端有 36+ 命令，`/help` 全表一屏挤满 / `/help all` 长版多消
息 — 想"我记得有个 xxx 功能但不知道是哪个命令"时只能瞎找。本迭代扩
`/help` 入口支持 `search <kw>` 子命令 — 扫所有命令名 + registry 描述 +
详细文案的 case-insensitive 子串命中，返清单。

## Changes

仅 `src-tauri/src/telegram/commands.rs`：

- `format_help_for_topic` 顶部 dispatch 加 "search" / "search <kw>" 分支
  （在 "all" 之后）:
  - 走新 helper `format_help_search(kw, custom)`
- 新 pure helper `format_help_search(kw, custom)`：
  - 空 kw → usage hint with 例子
  - 遍历 ALL_HELP_TOPICS：
    - in_name = command name 含 kw_lower
    - in_desc = zh registry 描述含 kw_lower
    - in_detail = `format_help_for_topic(name)` 完整 detail 含 kw_lower
    - 任一命中 → 收录 (name, registry_desc)
  - 无命中 → 友好兜底 + 提示 /help all
  - 有命中 → 头部 "🔍 /help search「<kw>」命中 N 条：" + 列 `· /<name>
    — <desc>` + 尾部"用 /help <cmd> 看单条详细 / /help all 看长版"
- `format_help_for_topic` "help" 详细文案补 `search <kw>` 第四种用法 +
  示例
- 8 个新 unit test：
  - 空 kw → usage hint
  - 命中 name (done)
  - 命中中文描述（"复制" 多条命中）
  - case-insensitive（done / DONE / Done 三种 case 命中数一致）
  - 无命中 → 兜底文案
  - dispatch via topic 顶层（"search done" → 同结果）
  - 仅 "search" → usage hint
  - 带 `/` 前缀（"/search done"）也能 trim 后命中

## Key design decisions

- **走 format_help_for_topic 顶层 dispatch 而非独立 TgCommand 变体**：
  /help search <kw> 是 /help 的子用法（与 /help all / /help <cmd> 同级），
  parser 已经 capture entire topic — 不必额外加 Search 变体。dispatch
  在 format_help_for_topic 顶部分流即可。
- **同时扫 name + desc + detail**：name 命中是最强信号（"done" → /done）；
  desc / detail 命中覆盖"我记得功能描述但不记得命令名"场景（"复制" →
  /digest / 其它含复制功能的命令）。
- **使用 zh registry 而非 en**：中文文案命中率更高（owner 通常用中文
  搜）+ en 命令名仍是 ASCII 总会被 in_name 路径 cover。简化避免双 lang
  扫两遍。
- **strip_prefix("search ") 模式**：name 已经 trim_start_matches('/')
  + to_lowercase；strip_prefix 完美对应 "search foo" / "/help search
  foo" 两路径。fall through 到 "search" exact 处理空 kw。
- **不命中时 hint /help all**：让 owner 知道还有"全文长版"备选 — 如果
  关键词太冷门搜不到，可能在 detail 里有相关说明。
- **跨大小写测试**：用 `s.matches("·").count()` 数 hit 行数对比 lower /
  upper / mixed — 验证 to_lowercase() 正确生效。

## Verification

- `cargo test --lib`（backend）— 1246 passed / 0 failed（8 新 help_search
  测试通过）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.22s)
