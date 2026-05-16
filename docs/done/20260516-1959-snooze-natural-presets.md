# `[snooze: ...]` 自然短串预设 + task_set_snooze 接受 preset 字符串

## 背景

owner 想给某 butler_task snooze 几小时 / 到明天，每次都要算"现在是 14:32，明早 09:00 → 写 [snooze: 2026-05-17 09:00]"。心算成本不大但烦琐。

iter 早期 TG bot 已有 `parse_snooze_token` + `compute_snooze_until` 把 EN 预设 (tonight / tomorrow / monday / Nm / Nh) 解析到绝对时刻 —— 但只在 TG `/snooze` 路径使用。`task_set_snooze` Tauri 命令仍只接受严格 YYYY-MM-DD HH:MM。

本 iter:
1. 扩 `parse_snooze_token` 接受 CJK 预设：今晚 / 明早 / 明天 / 明日 / 周一 / 下周一 / 下周1 / N分 / N小时
2. 让 `task_set_snooze` 接受任意 preset string，命中后用 `compute_snooze_until(now)` 解析到绝对时刻再写盘
3. 写测试覆盖新 path + 不破坏既有 TG `/snooze <title> <preset>` 解析

## 改动

### `src-tauri/src/telegram/commands.rs` — `parse_snooze_token` 扩 CJK

```rust
pub fn parse_snooze_token(token: &str) -> Option<SnoozeSpec> {
    let raw = token.trim();
    if raw.is_empty() { return None; }
    let t = raw.to_lowercase();
    // EN: tonight / tomorrow / monday
    match t.as_str() {
        "tonight" => return Some(SnoozeSpec::Tonight),
        "tomorrow" => return Some(SnoozeSpec::Tomorrow),
        "monday" => return Some(SnoozeSpec::Monday),
        _ => {}
    }
    // CJK 关键词：明早 / 明天 / 明日 → Tomorrow；周一 / 下周一 / 下周1 → Monday
    match raw {
        "今晚" => return Some(SnoozeSpec::Tonight),
        "明早" | "明天" | "明日" => return Some(SnoozeSpec::Tomorrow),
        "周一" | "下周一" | "下周1" => return Some(SnoozeSpec::Monday),
        _ => {}
    }
    // CJK 数字后缀：30 分 / 2 小时（允许内部空白）
    let raw_compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    if let Some(num_str) = raw_compact.strip_suffix('分') {
        let n: u32 = num_str.parse().ok()?;
        if n == 0 || n > 7 * 24 * 60 { return None; }
        return Some(SnoozeSpec::Minutes(n));
    }
    if let Some(num_str) = raw_compact.strip_suffix("小时") {
        let n: u32 = num_str.parse().ok()?;
        if n == 0 || n > 7 * 24 { return None; }
        return Some(SnoozeSpec::Hours(n));
    }
    // EN Nm / Nh：保留既有路径
    if let Some(num_str) = t.strip_suffix('m') { /* ... */ }
    if let Some(num_str) = t.strip_suffix('h') { /* ... */ }
    None
}
```

新增 3 个单测：
- `parse_snooze_token_cjk_keywords`：今晚 / 明早 / 明天 / 明日 / 周一 / 下周一 / 下周1
- `parse_snooze_token_cjk_durations`：30 分 / 90 分 / 2 小时 / 1 小时 + 中间空白宽容
- `parse_snooze_token_cjk_rejects_overflow`：0 分 / 99999 分 / 200 小时 / 后天（未实现的关键词）

### `src-tauri/src/commands/task.rs` — `task_set_snooze` 接受 preset

```rust
let parsed_until = match until.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
    Some(s) => {
        if let Some(spec) = crate::telegram::commands::parse_snooze_token(s) {
            let now = chrono::Local::now().naive_local();
            let abs = crate::telegram::commands::compute_snooze_until(spec, now);
            Some(abs.format("%Y-%m-%d %H:%M").to_string())
        } else if NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").is_ok() {
            Some(s.to_string())
        } else {
            return Err(format!(
                "invalid snooze input {:?} —— 预设支持：tonight / tomorrow / monday / 今晚 / 明早 / 明天 / 下周一 / 30m / 2h / 30分 / 2小时；或绝对格式 YYYY-MM-DD HH:MM",
                s
            ));
        }
    }
    None => None,
};
```

两路径：先预设、后绝对格式。两者都失败 → Err 错误信息同时列两种格式让调用者知道选哪种。

### `src-tauri/src/telegram/commands.rs` — `parse_tg_command` 端到端测

新 `parses_snooze_cjk_preset`：
- `/snooze 倒垃圾 今晚` → `{ title: "倒垃圾", token: "今晚" }`
- `/snooze 整理桌面 明早` → `{ title: "整理桌面", token: "明早" }`

验证 TG bot 端用户用 CJK preset 也走通。

### README

宠物管家 section 加 "自然短串预设入参" 段，解释新 EN + CJK 预设 + 解析规则（Tonight = 18:00 / Tomorrow = 09:00 / Monday = 下个周一 09:00）。

## 关键设计

- **复用 SnoozeSpec / compute_snooze_until**：既有 TG bot 路径完整测试过；新 CJK 关键词只是 token → SnoozeSpec 映射的拓展，下游解析不变。
- **CJK suffix 用 raw_compact 而非 raw**：让 owner 写 "30 分" / "2 小时" 与 "30分" / "2小时" 同样接受 —— 中文打字习惯常带空格。lowercased 对 CJK 不生效，所以保留原 raw 比对（"今晚" 是单一字面，无大小写）。
- **保留 EN Nm / Nh 路径**：放在 CJK 之后让 CJK preset / suffix 优先命中；EN 是 fallback。"30m" / "2h" 仍按既有解析。
- **task_set_snooze 错误消息列两种格式**：第一次失败时用户能看到完整可选项，免去"找文档"摩擦。
- **拒"后天" 等未实现关键词**：保持单 iter scope —— 后天 = day+2，可下一 iter 加 if 真需要；当前已覆盖最高频场景。
- **不修改 parse_snooze 解析器**：description 内字面量仍要求 YYYY-MM-DD HH:MM 绝对格式 —— 一致性比"description 里也写 [snooze: 今晚]" 重要。owner 想 preset 自动解析就走 `task_set_snooze` 或 TG `/snooze` 入口。
- **不写 frontend 改动**：PanelTasks ctx menu 已有 4 个 preset 按钮且都在 JS 里解析。本 iter 只让 backend 也接受 preset 串，给 LLM tool / 未来 CLI / TG bot 用，前端不需改。

## 不做

- **不让 description 字面量 `[snooze: 今晚]` 自动解析**：见上 —— description 是 SoT 字面量，preset 解析时刻不应"漂移"（同 task 下次 reload 时刻不同会解析出不同的 wake-up）。
- **不实现 "后天 / 大后天 / 下下周一" 等关键词**：长尾，等 owner 实际有需求再加。
- **不在 LLM butler_task_edit 工具描述加 preset 教学**：当前工具不 expose snooze 字段；下一 iter 如果 LLM 想 set snooze 才用得到，再补 doc。
- **不写 frontend 改动**：ctx menu 既有按钮 + 现行 JS 解析 与 backend preset 并行无冲突。

## 验证

- `cargo check` ✓
- `cargo test --lib telegram::commands::tests::parse_snooze_token` ✓ 6 passed（3 新 CJK 测）
- `cargo test --lib telegram::commands::tests::parses_snooze` ✓ 5 passed（含 1 新 CJK preset end-to-end）
- `npx tsc --noEmit` ✓ 0 error（仅后端改动）
- 改动 ~150 行（parse_snooze_token 扩 CJK 40 + task_set_snooze 双路径 20 + 单测 60 + 错误消息 + README 10）。既有 TG bot snooze pipeline / parse_snooze description-marker 解析器 / strip_snooze_markers / SnoozeSpec / compute_snooze_until 完全不动。

## TODO 状态

剩 1 条留池：
- 桌面 pet collapse tab hover 1s 浮 ambient mini card（与 useAutoHide 即时展开 UX 冲突，可能不可实现 —— 下 iter 评估或替换）

## 后续

- task_create_tool / butler_task_edit_tool 工具 schema 加 `snooze` 可选字段（接受 preset 字符串）让 LLM 直接调用 — 例如"等用户上厕所回来再做" 之类的 buffered task。
- PanelTasks ctx menu 简化 —— 把 4 个 preset 按钮的 JS 解析改成调 task_set_snooze 传 raw preset 字符串，由 backend 解析（减一处时区 / 闰秒 / DST 边界 bug 风险）。
- 扩支持 "下周三" / "本周末" / "this friday" 等周内任一天预设。
- 解析 / 表达式扩展为 "明天下午"（明日 14:00）/ "下个工作日"（避开周末 + 跳到下周）。
