# mood_history 清理入口 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> mood_history 删除按钮：`record_mood` dedupe 后偶尔写脏数据；面板「人格」段加一个「清掉过去 N 天 mood_history」管理入口。

## 目标

`record_mood` 在某些场景（reload 漂移 / 异常退出 / 早期 motion 标签解析问题）会
落几条脏 entry。当前只能去文件系统手动改 `~/.config/pet/mood_history.log`。
本轮在「人格」段「心情谱」section 下加一个折叠管理入口，给用户"清掉最近 N 天
mood_history"的能力（N=0 等于清空全部）。

## 非目标

- 不做单条删除 / 范围删除 —— 用户的实际诉求是"批量清掉 dirty 段"，逐条
  UI 复杂度不值。
- 不影响 `mood_history.log` 之外的文件（speech_history / butler_history 等）。
- 不写 README —— 调试 / 维护补强。

## 设计

### 后端

`mood_history.rs` 加 pure helper + Tauri 命令：

```rust
/// Pure：把超过 `days` 天前（更老）的条目保留，删除"过去 days 天内"的所有
/// 条目。days = 0 视作清空全部。malformed / ts 解析失败的行**也删除**（用户
/// 已显式请求清理，dirty 数据是首要目标）。
pub fn filter_mood_history_clear_recent_days(
    content: &str,
    days: u32,
    now: chrono::DateTime<chrono::Local>,
) -> String;

#[tauri::command]
pub async fn clear_mood_history(days: Option<u32>) -> Result<u32, String>;
```

- `days = None | Some(0)` → 清空全部
- `days = Some(N)` → 保留 ts < `now - N days` 的行
- 写盘成功 → 返回**剩余条目数**（让前端展示"清理完成，剩余 X 条"反馈）

注册到 lib.rs。

### 前端

`PanelPersona.tsx` 的 `MoodSparkline` 下方加一个折叠管理入口（默认折叠避免
噪音）：

- 入口：「⚙ 管理」小按钮（链接风格、灰色，不夺主视觉）
- 展开后：
  - 数字输入 `<input type=number min=0 />`，默认 7
  - 按钮「清理（0 = 全部）」
  - 反馈文案：成功 "已清理，剩余 N 条" / 失败 "清理失败：…"
- 清理后：reload sparkline 数据 + trend hint（已有 5s polling 会自动跟进，但
  手动 trigger 一次更即时）

### 测试

后端 `filter_mood_history_clear_recent_days` 是 pure，单测：
- 空输入 → 空输出
- days = 0 → 输出 ""（清空）
- 全部都在 N 天内 → 输出 ""
- 混合：N=3，部分行 ts 早于 cutoff → 仅保留早的
- malformed 行（无 ts / ts 不是 RFC3339）→ 删除（与"用户主动清理 dirty 数据"
  语义一致）
- 边界 ts == cutoff → 删除（即"恰好 N 天前"的行视作"在过去 N 天内"）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `filter_mood_history_clear_recent_days` 纯函数 + 5+ 单测 |
| **M2** | `clear_mood_history` Tauri 命令 + 注册 lib.rs |
| **M3** | PanelPersona 折叠管理入口 UI |
| **M4** | cargo test + tsc + build + cleanup |

## 复用清单

- `mood_history::history_path` / `read_history_content`（写盘后状态由现有
  `read_history_content` 在下次 fetch 时自然加载）
- 现有 5s polling 在 PanelPersona 里
- `Section` / 现有按钮样式

## 待用户裁定的开放问题

- malformed 行处理：保留 vs 删除？本轮**删除**（清理语义一致）。
- 是否做"撤销"？本轮**否**——清空文件是用户显式动作；想保留可在管理入口
  前先复制文件。文档可未来加。

## 进度日志

- 2026-05-05 21:00 — 创建本文档；准备 M1。
- 2026-05-05 21:30 — 完成实现：
  - **M1**：`mood_history.rs` 加 `filter_mood_history_clear_recent_days(content, days, now)` 纯函数：days=0 全清；days>0 保留 ts < now-N天 的行；malformed / ts 解析失败的行**也删除**（与"用户主动清理 dirty 数据"语义一致）。6 条单测覆盖：days=0 全清 / cutoff 边界严格 `<` / malformed drop / 空输入 / 全 recent / 输出契约（非空时尾随 `\n`）。
  - **M2**：`clear_mood_history(days: Option<u32>)` Tauri 命令：读 → 过滤 → 写盘 → 返回剩余行数。文件不存在视作"已经空"返回 0。注册到 lib.rs。
  - **M3**：`PanelPersona.tsx` 心情谱 section 下加折叠管理入口（`▸ 管理` 链接风格，默认折叠避免噪音）。展开后含数字输入（默认 7）+ 红字"确认清理"按钮 + 反馈文案。`handleClearMoodHistory` 成功后即时调 `get_mood_trend_hint` + `get_mood_daily_motions` 刷新 trend / sparkline，不必等 5s polling。
  - **M4**：`cargo test --lib` 885/885（+6）；`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 数据维护补强，不是新独立亮点。
  - **设计取舍**：边界 `ts == cutoff` drop（严格 `<`）—— "恰好 N 天前"视作"在过去 N 天内"，与 task overdue 同语义习惯；malformed 行 drop（用户已显式请求清理，dirty 数据是首要目标）；折叠默认（避免抢主视觉）；按钮红色（毁灭性操作的视觉警示）。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；纯函数 6 条单测含全部边界，IO 层是 read+filter+write 简单串联（与现有 record_mood 共享 history_path）。
