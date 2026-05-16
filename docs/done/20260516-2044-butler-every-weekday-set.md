# butler_task `[every:]` weekday-set 限定支持

## 背景

既有 `[every: 09:00]` 每日固定时间触发。但 90% 的"标准时间任务" 都是 weekday-限定：
- 上班日 standup / 日报：周一到周五 09:00
- 周末整理：周六周日 10:00
- 周会准备：仅周一
- 每周日联系长辈：仅周日

owner 之前必须在 `[every: 09:00]` task 描述末追加"仅工作日做" 文字 hint 让 LLM 自觉判断 —— 不可靠。本 iter 扩 `[every:]` 协议接受 weekday-set 关键词，让 backend `is_butler_due` 直接 enforce。

## 改动

### Backend `src-tauri/src/proactive/butler_schedule.rs`

#### 1. 新 `EveryOnWeekdays(mask, h, m)` 变体

```rust
pub enum ButlerSchedule {
    Every(u8, u8),                       // 既有：每日 H:M
    EveryOnWeekdays(u8, u8, u8),         // mask, hour, minute；mask bit 0 = Mon ... bit 6 = Sun
    Once(NaiveDateTime),                 // 既有：单次绝对
}

pub const WEEKDAY_MASK_WORKDAYS: u8 = 0b0011111; // Mon-Fri
pub const WEEKDAY_MASK_WEEKEND: u8 = 0b1100000;  // Sat-Sun
```

#### 2. 新 helpers

```rust
pub fn parse_single_weekday_keyword(s: &str) -> Option<u8>;
// "Mon" / "周一" / "礼拜一" → 1 << 0
// "Sun" / "周日" / "周天" / "星期天" → 1 << 6
// ...

pub fn parse_weekday_set_keyword(s: &str) -> Option<u8>;
// "工作日" / "weekday" / "weekdays" / "周一到周五" → WEEKDAY_MASK_WORKDAYS
// "周末" / "weekend" / "双休" → WEEKDAY_MASK_WEEKEND
// 单 weekday → 单 bit mask
```

#### 3. `parse_butler_schedule_prefix` 扩

`[every: <weekday-set> HH:MM]` rsplit 末空白 token 当 HH:MM、左半当 weekday-set 关键词解析。无 weekday-set 时走原 `[every: HH:MM]` 路径，向后兼容。

```rust
if let Some(space_idx) = inside.rfind(char::is_whitespace) {
    let (left, right) = inside.split_at(space_idx);
    let weekday_keyword = left.trim();
    let time_part = right.trim();
    if !weekday_keyword.is_empty() && !time_part.is_empty() {
        let (hh, mm) = time_part.split_once(':')?;
        let hour: u8 = hh.trim().parse().ok()?;
        let minute: u8 = mm.trim().parse().ok()?;
        if hour > 23 || minute > 59 { return None; }
        let mask = parse_weekday_set_keyword(weekday_keyword)?;
        return Some((ButlerSchedule::EveryOnWeekdays(mask, hour, minute), topic));
    }
}
// 原 [every: HH:MM] 路径...
```

#### 4. `is_butler_due` 新 branch

```rust
ButlerSchedule::EveryOnWeekdays(mask, h, m) => {
    if *mask == 0 { return false; }
    let target_today = ...;
    let today_match = (mask & today_weekday_bit) != 0 && now >= target_today;
    let mut offset_back = if today_match { 0 } else { 1 };
    let mut most_recent_fire = None;
    while offset_back <= 7 {
        let cand_date = now.date() - Duration::days(offset_back);
        if (mask & weekday_bit_from_chrono(cand_date.weekday())) != 0 {
            most_recent_fire = Some(cand_date.and_hms(h, m, 0));
            break;
        }
        offset_back += 1;
    }
    match (most_recent_fire, last) {
        (Some(fire), Some(u)) => u < fire,
        (Some(_), None) => true,
        (None, _) => false,
    }
}
```

algorithm：从今天起向回扫 ≤ 7 天，找首个 mask 命中日 + HH:MM 作 most-recent-fire。今日 mask 命中 + 时刻已过 → 用今日；否则从昨日向回找。`mask == 0` → 不 fire（防御性）。

#### 5. 4 个新单测

- `parse_weekday_set_keyword_basic`：中英双语 + 单 weekday + 不识别
- `parse_butler_schedule_prefix_parses_every_weekday_set`：3 种 weekday-set 形态
- `parse_butler_schedule_prefix_rejects_invalid_weekday`："后天 09:00" 应整段 None
- `is_butler_due_every_weekday_set`：周一 10:00 (工作日 09:00 fire 过) + 周六 (回看到周五) + 周末 mask

跑 `cargo test --lib proactive::butler_schedule` ✓ 64 passed（4 新）。

### Frontend `src/components/panel/PanelMemory.tsx` 镜像

#### 1. ButlerSchedule TS 类型 + parser

新 `kind: "every_weekdays"` discriminant + parser 镜像 backend 同算法。`parseSingleWeekdayKeyword` / `parseWeekdaySetKeyword` 同中英双语关键词集合。

#### 2. `mostRecentFire` 新 branch

JS Date 周日是 0 但 chrono Mon 是 0 —— 用 `jsDayToMonBit(d) = 1 << ((d + 6) % 7)` 转换。算法同 backend。

#### 3. `scheduleLabel` 显示

```ts
formatWeekdayMaskLabel(mask) — 工作日 / 周末 / 每天 / 周一/三/五 等可读
"🔁 工作日 09:00" / "🔁 周末 10:00" / "🔁 周一 09:00"
```

#### 4. 已有交互点同步

- `📋 今日 todo` 按钮：every_weekdays 命中 mask & 今日 weekday bit 才算"今日"
- 计数 chip (everyCnt / onceCnt 等)：every_weekdays 归入 everyCnt
- schedule kind filter chip：every_weekdays 命中 "every" filter
- `📋 复制 schedule` 按钮：every_weekdays 输出 `[every: 工作日 09:00]` 格式
- "下次触发：X 后" chip：every_weekdays 向前扫 ≤ 7 天找未来命中日
- "✏️ 改 schedule" 按钮：every_weekdays 暂禁用（modal 需扩 weekday-set selector，后续 iter）

#### 5. SCHEDULE_TEMPLATES + placeholder 扩

```ts
{ label: "🔁 工作日", text: "[every: 工作日 09:00] " },
{ label: "🔁 周末", text: "[every: 周末 10:00] " },
```

placeholder 加示例："比如：[every: 工作日 09:00] 早上 standup / [every: 周末 10:00] 整理桌面"。

### README 加新功能 bullet

宠物管家 section 顶加一段解释 weekday-set 协议 + 用例 + 中英关键词集合。

## 关键设计

- **新增枚举变体而非扩 Every**：保留 `Every(h, m)` 作"全 7 天"语义，避免 (h, m, mask) 模式 match 处处变三元组。新变体让前向兼容 + 现有 caller 不需改。
- **mask 0b1111111 == 每天**：未来可统一为单 `Every(mask, h, m)` 模型，把 `Every(h, m)` 视作 `mask=0b1111111` 的 alias。当前为 minimal-disruption 选 dual-variant。
- **chrono Weekday::num_days_from_monday() vs JS Date.getDay()**：chrono Mon=0、JS Sun=0；前端 `jsDayToMonBit(d) = 1 << ((d + 6) % 7)` 转换。两端 mask 表示同一组 weekday 集合。
- **rsplit 末空白 token 作 HH:MM**：让 `[every: 工作日 09:00]` 与 `[every: 周一 周三 09:00]` 等多 token weekday-set 都能解析（后者目前不识别为 valid，但 future-proof）。
- **`[every: 工作日 09:00]` 整体识别失败 → None 而非退化为 `[every: 09:00]`**：避免 "后天 09:00" 等 typo 静默被当 daily 触发。
- **`is_completed_once` 把 EveryOnWeekdays 也 return false**：与 Every 一致，recurring 不计"已完成可清理"。
- **edit-schedule modal 暂禁 every_weekdays**：modal UI 暂无 weekday-set selector，先 disable 按钮防 owner 点击后 modal 拿不到 year/month/day 字段崩。下 iter 扩 modal。
- **chip 视觉 every_weekdays 与 every 同蓝色 🔁**：都是循环类，区分仅在 label（"每天" vs "工作日"）—— UX 一致。

## 不做

- **不支持 "周一,三,五" 自定义 weekday list**：parser 复杂度激增；最常用的 工作日 / 周末 / 单 weekday 已覆盖 90% 场景。future iter 加 comma-separated 解析。
- **不扩 edit-schedule modal 为 every_weekdays**：modal 需要 7 个 weekday checkbox，UI 工作量大；本 iter 让 owner 通过编辑 description 字面量改 weekday-set。
- **不让 LLM 工具 schema 暴露 weekday-set 参数**：LLM 写 description 字面量即可（既有路径），不需新工具参数。
- **不限制 owner 把 every_weekdays 用在历史日期**：owner 可写 "[every: 周一 09:00]" 在周六生效后即按"上次周一"作 most-recent-fire，符合直觉。

## 验证

- `cargo check` ✓
- `cargo test --lib proactive::butler_schedule` ✓ 64 passed（4 新单测）
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.16s
- 改动 ~400 行（backend 250：parse helpers + variant + is_butler_due branch + 4 单测；frontend 130：TS mirror + 5 处现有 caller 适配 + 2 template + placeholder；README 1）。`build_butler_tasks_hint` / `format_butler_tasks_block` / `consolidate.rs::sweep_completed_once_butler_tasks` 上游所有调用 ButlerSchedule 的路径 cargo check pass —— pattern match 都已 cover EveryOnWeekdays。

## TODO 状态

TODO 池清空 —— 下个 cron tick 进 auto-propose。

## 后续

- edit-schedule modal 扩 weekday checkbox grid (Mon-Sun)，让 owner click 切换 mask；保存时拼回 weekday-set label / 单 weekday 串。
- 支持 comma list "周一,三,五" / "周一 周三 周五" 自定义 weekday set parser；formatWeekdayMaskLabel 显 "周一/三/五"。
- TG bot `/task` 命令 schema 接受 weekday-set 参数。
- LLM tool butler_task_edit 工具 schema 加 weekday-set 字段教学。
- proactive prompt 给 LLM 看本周已经 fire 过的 weekday count，让 LLM 知道 "工作日 standup 上次周三做了" 等等。
