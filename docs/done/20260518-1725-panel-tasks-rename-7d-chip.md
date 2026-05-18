# PanelTasks chip-bar「🏷 7d rename N」chip（iter #581）

## Background

iter #574 / #580 加 TG /aliases + /recent_renames 让 rename audit 远程
可见。桌面端缺概览：「最近我改了多少 task 名」refactoring 节奏信号。
本 iter 加 chip-bar 单数字 chip。

完成 rename audit 三视角：
- /aliases <title>: 单 task vertical chain
- /recent_renames [N]: cross-task horizontal list（TG）
- 🏷 7d rename chip: 桌面 chip-bar 数字概览（本 iter）

## Change

`PanelTasks.tsx`：

1. 新 `renameCount7d: number | null` state + useEffect fetch
   `get_butler_history(100)` + frontend filter (action='rename' AND ts ≥
   now-7d) — 与既有 `hourly24h` chip 同 mount + 5min refresh pattern
2. chip-bar visibility gate `|| (renameCount7d ?? 0) > 0`
3. 紧贴 🚀 今日 P7+ chip 后插「🏷 7d rename N」chip — slate-tint
   neutral 色与 amber/red 错开
4. click → 复制「近 7d N 次 rename」单行 + 2.5s ✓ 反馈

## Key design decisions

- **frontend filter vs 新 backend command**：与 iter #578（PanelTasks
  row hover 🔄 chip 复用 sparkline buckets）同精神 — 不加新 Tauri 命
  令，复用既有 get_butler_history(100) 然后前端 parse + filter。
  butler_history cap 100 entries 让前端 O(N) 扫成本可忽略
- **`body.startsWith("rename ")` 不严格 word boundary**：因 record_event
  写 `<action> <title> :: <snippet>` 已保 action 是首 word。startsWith
  够稳；无需 regex
- **5min refresh tick**：与 hourly24h chip 同 cadence — butler_history
  写频率低（仅 update / rename / delete），秒级实时无必要
- **slate-tint neutral 色**：避免与 stale (red) / fresh (green) / urgent
  (amber) 等情感色族冲突。rename 是 metadata signal 非紧急
- **复制单行「近 7d N 次 rename」简洁**：与 chip-family copy pattern
  （pinned / idle / completion 等）一致

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — chip 加在熟悉 chip-bar 位置，同 click-to-copy
  pattern，无 layout race

## Future iters (out of scope)

- **chip click 打开 rename 详情 modal**：展开「近 7d N 条 rename」每行
  「ts · old → new」— 把 chip 升为入口而非单数字。但弹 modal 显多
  数据可能比 TG /recent_renames 更复杂；按需 propose
- **chip 颜色随频率告警**：N > 阈值（如 20+）切红色提示「rename 节
  奏过频」— audit「我命名标准是否不稳」decisional signal。需经验阈值
- **「🏷 30d」cousin chip**：与 cat_growth/decay 30d pattern 一致。但
  rename 频率本身低，30d 不常超 20+；先 7d ship 看实际 owner 使用
