# PanelTasks 「💤 Nd 未动」hover chip（iter #559）

## Background

PanelTasks 已有 `⏳ pending Nd` chip 显 task 自创建以来的天数 — age
视角。但 owner 真关心的「这条还活跃吗 / 我搁了多久没动它」用 age 看
不出来：新建几天后频繁 touch 的 task 也显 90 天，老 task 偶被 update
一次也显 90 天，无法区分活跃与 stale。

本 iter 加 `💤 Nd 未动` chip — inactivity 视角，按 `updated_at` 距 now
的天数算。两 chip 并存形成 audit 双视角：
- ⏳ pending Nd：「这条 task 出生多久」
- 💤 Nd 未动：「这条 task 现在还动吗」

## Change

`PanelTasks.tsx` 紧贴既有 `⏳ pending Nd` chip 加平行 chip：

```tsx
{taskPreviewHoverTitle === t.title &&
  t.status === "pending" &&
  t.updated_at.length > 0 &&
  (() => {
    const uMs = Date.parse(t.updated_at);
    if (isNaN(uMs)) return null;
    const idleMs = nowMs - uMs;
    if (idleMs < 0) return null;
    const idleDays = Math.floor(idleMs / 86_400_000);
    if (idleDays < 7) return null;
    const veryStale = idleDays >= 21;
    return <button …>💤 {idleDays}d 未动</button>;
  })()}
```

## Key design decisions

- **7 天起算 vs 14 天**：⏳ pending Nd 是 14 天 stale 阈值（age 14 天
  以下都正常）；本 chip 7 天阈值（idle 7 天就值得 audit — 因为多数活
  跃 task 周内必有 touch）。两阈值差异有意为之
- **21 天 veryStale 红 bg 加粗**：与 ⏳ 的 30 天阈值平行；idle 21 天
  比 age 30 天更严重（active task 偶 update 一次能拖 age 但 idle 一定
  反映"完全没碰"）
- **gate 同 ⏳ 套路**：pending + 有 updated_at + idle ≥ 阈值；hover
  permanent gate `taskPreviewHoverTitle === t.title`。done / cancel /
  error 不显本 chip — finished task 不再需要 inactivity 信号
- **`updated_at` 而非 detail-md mtime**：本字段已涵盖 detail.md edit
  （memory_edit_detail / memory_rename 都 bump task row updated_at）+
  title edit + status change + [markers]。owner 关心"任何 activity"
  比"仅 detail.md edit"语义宽更对应"还活跃吗"audit
- **click 复制单行**：与 ⏳ pending Nd / ⏱ 历经 chip 同模式。文案
  「<title> Nd 未动（last update YYYY-MM-DD）」可直发同事 / 写 audit
- **🛌 vs 💤 vs 🕸 emoji 选择**：💤 zZ 直观传达 dormancy 又与既有 💤
  snooze-wake chip 形似（同一族「休眠 / 暂停」语义）— 不混淆是因
  snooze chip 是 active-with-future-wake，本 chip 是 stale-no-future。
  tooltip 区分清楚

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — 是单 chip 添加，与既有 ⏳ chip 同 gate / 同样式
  系统，无跨层 race
- 无新 lib test — UI 纯 React 添加

## Future iters (out of scope)

- **detail.md mtime 单独信号**：若 owner 想精准区分「task row 动过但
  detail.md 没动」（如仅刷 [silent] marker），需读 fs::metadata 的
  mtime。需要后端 expose；out of scope
- **「💤 0d 未动」（今天动过）反向 chip**：positive 信号 — owner 想
  确认「这条今天动过吗」。但今日 chip 家族已含 isRecentlyUpdated；
  无需重复
- **批量「💤 7d+」filter toggle**：toolbar 加「仅显 idle ≥ 7 天 task」
  — 让 owner 一次性看所有 stale backlog。比单 chip hover 信息密度高，
  按需 propose
