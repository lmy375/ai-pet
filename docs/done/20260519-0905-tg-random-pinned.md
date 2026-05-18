# TG bot 加 `/random_pinned` 命令（iter #550）

## Background

`/random` 从全 active task 集随机抽 1 条 — 给 owner「选择困难」时让 pet
决定下一步。但常见场景是「owner 钉了几条都重要 / 不知先做哪条」 — 全
active 抽包括没钉的杂项 task，决策价值打折。

本 iter 加 `/random_pinned` — `/random` 的 pinned 子集版。

## Changes

按 6+ surface 同步：

1. **Enum 变体** `RandomPinned`（紧贴 PeekPinned）
2. `name()` arm → `"random_pinned"`
3. `title()` arm → 无参 arm 集
4. parser arm
5. en + zh registry entries
6. ALL_HELP_TOPICS / help-detail / help-table / 两份 drift-defense lists

#### 纯 formatter `format_random_pinned_reply`

clone of `format_random_reply`：

- candidates filter 加 `v.pinned` 一层（叠加既有 Pending/Error active
  filter）
- header 显「共 N 条 pinned active」让 owner 一眼看 pool size
- 空集兜底教学指 /pin（设置）/ /random（fallback）/ /pinned（清单）

复用 `RANDOM_RAW_DESC_PREVIEW_CHARS = 200` 常量 + 「—— 选择困难？就先做
这条吧。」结尾文案（与 /random 一致心智）。

### Handler

紧贴 Random 之后：

```rust
TgCommand::RandomPinned => {
    let views = read_tg_chat_task_views(chat_id.0);
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0);
    format_random_pinned_reply(&views, seed)
}
```

与 /random 同 seed source（system time nanos）+ 同 `seed % candidates.len()`
索引算法 — 多次调用得不同 task。

## Key design decisions

- **复用 /random 协议**：同 seed 算法 / 同 raw preview cap / 同结尾鸡汤
  文案 — owner 切换两入口心智无歧义
- **pinned + active 双重过滤**：done/cancelled pinned task 不算（终态
  抽出来无意义）
- **空集教学指三条 alt**：/pin（owner 还没设过 pinned）/ /random（全集
  fallback）/ /pinned（看 pinned 清单），覆盖三种 next-action
- **clone 不抽 generic**：与既有 today/yesterday/thisweek + /peek /
  /peek_pinned 等 split 模板一致 — 单测点稳定
- **4 unit tests pin 真实行为**：parser（含尾部 token 容忍）+ 空集兜底
  （含 alt 入口验证）+ pinned active 过滤（pinned+pending in / 非 pinned
  out / pinned+done out / pinned+error in）+ seed % cycle（验 0/1/2/3
  循环到 0）

## Verification

- `cargo build`（src-tauri）— clean
- `cargo test --lib` — all 1702 tests pass（新 4 + 既有 1698 — 注：1
  来自 1697 后 ⌘⇧H heading cycle 提交不增 tests）
- 三个 drift-defense test all pass
- 手测：
  - pin 几条 task + /random_pinned → 「🎲 抽中 ⏳ 「title」（共 N 条
    pinned active）」+ raw preview + 鸡汤
  - 无 pinned task → 友好兜底
  - 连续调多次 → 不同 pinned task（seed 非确定性）

## Future iters (out of scope)

- `/random_today` — 从今日 touched 集随机；按需 propose
- 「随机抽 N 条」/ multi-pick — 当前单条已覆盖 80% 场景
