# PanelMemory item「⏰ 一次性 5/15/30 分钟后提醒」chip（iter #372）

## Background

既有 `[reminderMin: N]` marker 挂在 butler_task 的 `[once: ...]` /
`[every: ...]` / `[deadline: ...]` schedule 上，做"既定 fire-time 前
N 分钟提醒"。但 owner 在看 PanelMemory 时想"这条我等 15 分钟后跑
一下"的**一次性**提醒，跟 schedule 解耦，当前无快捷入口。

解决：复用既有 `todo` 类目 + `[remind: YYYY-MM-DD HH:MM]` 协议
（`proactive::reminders.rs`）— 这条 pipeline 已就绪：proactive 扫
todo 段，到 due 触发 ChatMini 软提醒；consolidate sweep 24h 后清
扫 Absolute target stale 条目。本 iter 仅加前端 chip 入口。

## Changes

### `src/components/panel/PanelMemory.tsx`

#### 1. state（~line 800，紧贴 reminderQuickPickerTitle）

```ts
const [alarmPickerKey, setAlarmPickerKey] = useState<string | null>(null);
const [alarmBusy, setAlarmBusy] = useState(false);
```

key 复合 `${catKey}::${title}` — 与 historyPicker / moveCatPicker 同
模板，让"同时只允许一个 popover 打开"。outside-click + Esc 关闭 useEffect
也与 reminderQuickPicker 同模板。

#### 2. `armOneShotAlarm(srcTitle, minutes)` callback

```ts
const target = new Date(now + minutes * 60_000);
const targetIso = `YYYY-MM-DD HH:MM`; // 本地时区，与 reminders.rs parse_reminder_prefix 期望一致
const reminderTitle = `⏰ ${srcTitle} @ HH:MM`;
const description = `[remind: ${targetIso}] ${srcTitle}`;
invoke("memory_edit", { action: "create", category: "todo", title, description });
```

- 标题前缀 `⏰ ` + 触发 HH:MM → owner 在 todo 段一眼识别 reminder
  出处 + 本次什么时候 fire
- description prefix `[remind: ...]` → 让 `reminders.rs::parse_reminder_prefix`
  解析；proactive loop 扫 todo 段时把这条挂进 prompt（is_reminder_due
  返 true → 注入到 "你有以下到期的用户提醒" hint）
- 创建后 loadIndex 刷新 → todo 段显新条目
- 失败显 setMessage

#### 3. chip + inline popover（~line 5984，pin 按钮后）

- 触发按钮：`⏰`，open 态 blue tint
- popover：3 preset 按钮 `⏰ 5 分钟后` / `⏰ 15 分钟后` / `⏰ 30 分钟后`
- 防嵌套：`catKey !== "todo"` — 不在 todo 段渲（owner 想给已有 reminder
  续期应直接 edit description）

## Key design decisions

- **复用 `todo` 类目 + `[remind: ...]` 协议而非新 schema**：proactive
  reminder pipeline 已就绪（reminders.rs 解析 + 时序 due 检查 +
  consolidate stale 清扫），只缺前端入口。零后端改动，仅 UI 链路。
- **5/15/30 三档而非自由输入**：owner 一键场景，免输入框 friction。
  如未来需要自定义分钟数，把 popover 第 4 行加 "自定义…" 跳现有
  reminder edit modal 即可。
- **alarm title 含触发 HH:MM**：todo 段长列表里多个 alarm 时，标
  题里的 HH:MM 让 owner 一眼分辨"哪个先 fire / 哪个还没到点"。
- **不在 `todo` 段渲 chip**：嵌套 reminder 无意义。owner 想给已有
  todo item 续期应走 edit description（既有路径）。
- **持久化通过 memory_edit（已有 mirror_todo_create SQL 镜像）**：
  应用重启后 todo 段仍含本条目，proactive 下个 tick 仍能扫到 →
  软提醒不会因重启丢失（与 iter #366 frontend timer / iter #371
  backend tokio timer 的 transient 性质对比，本路径更可靠）。
- **不抢现有 reminderMin 路径**：reminderMin 是 "task 既定 fire-time
  前 N 分钟提醒"，挂 schedule 上下文。本 chip 是 "完全独立的 N 分钟
  后 alarm，无 schedule"。两者正交 — title 上明示"与 reminderMin 区
  分"。
- **不为单 chip 引 unit test runner**：项目无 .test.tsx；行为是 IO
  + state ops，build pass + 手测足够。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.25s)
- 后端无改动 — 复用既有 memory_edit + proactive::reminders pipeline
