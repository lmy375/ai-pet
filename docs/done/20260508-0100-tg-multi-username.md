# TG bot allowed_username 多用户支持 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG bot allowed_username 多用户：现 `allowed_username` 只支持单个；改成 `,` 分隔多个 username 列表，让多人 bot（family / team）能共用。

## 目标

`TelegramConfig.allowed_username` 当前是单个字符串，handle_message 检查
`username != state.allowed_username` 拒绝其它用户。家庭 / 小团队场景下
希望白名单 N 个 username 共用同一个 bot；本轮把字段改为 "comma-separated"
格式，bot 启动时拆 list 校验。

## 非目标

- 不动 settings 字段名 / 类型（`allowed_username: String`）—— 用户可能
  已经填了单个 username，把字段改为 Vec<String> 会引入 settings migration
  问题。保持 String + 用 `,` 分隔是 wire format 演进。
- 不做 per-user 配额 / 偏好 —— 当前 bot 是"单 chat 多消息"模型，每个用户
  有自己的 chat_id 已是足够隔离；加 per-user 偏好是另一轮决策。
- 不做 group chat 支持 —— 那会引入 chat_id 的 owner 概念，本轮只解 1:1
  场景的多用户。

## 设计

### 解析

新增 pure helper `parse_allowed_usernames(raw: &str) -> Vec<String>`：
- 按 `,` 分隔
- 每段 trim + `@` 前缀去除 + lowercase
- 空段（连续逗号 / 全空白）跳过
- 同名去重保留首个

放在 telegram::commands 模块（与既有 fuzzy / suggest 同源），写单测。

### HandlerState

`allowed_username: String` → `allowed_usernames: Vec<String>` （内部）。
启动时调 parse_allowed_usernames(&config.allowed_username) 填充。

### 校验

```rust
if !state.allowed_usernames.is_empty()
    && !state.allowed_usernames.iter().any(|u| u == &username) {
    // 拒绝
}
```

空 Vec = "全开放" 与现有空 String 同语义（之前 unset 默认放过任何人）。

### 设置 UI 提示

PanelSettings TG section 的 allowed_username 输入框 placeholder / hint 加
"多个用户用 `,` 分隔，例如 `alice, bob`"。

### 测试

- parse_allowed_usernames("alice") → ["alice"]
- parse_allowed_usernames("alice, bob") → ["alice", "bob"]
- parse_allowed_usernames("@Alice, @bob") → ["alice", "bob"]
- parse_allowed_usernames("alice,,bob") → ["alice", "bob"]（跳过空）
- parse_allowed_usernames("alice, alice") → ["alice"]（dedup）
- parse_allowed_usernames("") → []
- parse_allowed_usernames("   ") → []

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | parse_allowed_usernames 纯函数 + 单测 |
| **M2** | HandlerState 改 Vec + handle_message 校验调整 |
| **M3** | PanelSettings hint 文案更新 |
| **M4** | cargo test + tsc + build + cleanup |

## 复用清单

- 既有 TelegramConfig.allowed_username field（语义扩展 + 同字段名）
- 既有 PanelSettings TG 段输入框

## 进度日志

- 2026-05-08 01:00 — 创建本文档；准备 M1。
- 2026-05-08 01:10 — M1 完成。`parse_allowed_usernames(raw)` 纯函数：`,` 分隔、trim、剥 `@` 前缀、lowercase、空段跳过、同名 dedup；6 个新单测覆盖 single / 多用户 / @ 大小写归一 / 跳空段 / dedup / 空输入。
- 2026-05-08 01:20 — M2 完成。HandlerState `allowed_username: String` → `allowed_usernames: Vec<String>`；启动时调 parse 填充；handle_message 检查改为 `iter().any(|u| u == &username)`，空 Vec 仍为 "全开放" 兼容旧行为。
- 2026-05-08 01:25 — M3 完成。PanelSettings `允许的用户名` label 后加 `多个用 `,` 分隔` 灰字 hint；placeholder 改 "@alice, @bob (留空则允许所有人)"。
- 2026-05-08 01:30 — M4 完成。`cargo build` 11.85s 通过；`cargo test --lib` 982 通过（+6 新测）；`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过。归档至 done。
