# PanelTasks 右键「⏰ due in N min」sub-menu（iter #387）

## Background

PanelTasks 已有 reminderMin（fire 前 N 分提醒）和 snooze（推后到点）
submenu in 右键菜单，但缺"设 due time 本身的快速 preset"路径 —
owner 设 "20 分钟后开会要做这事的 due" 需点 ✏️ 改 schedule modal /
输 datetime-local。本 iter 加短期 due preset sub-menu 一键搞定。

与 [reminderMin: N] / [snooze: ...] 正交：
- reminderMin: fire 时刻前 N 分钟软提醒（reminderMin 是 buffer）
- snooze: 暂时把 task 隐藏到指定时刻（snooze 是推后）
- 本 sub-menu: 设 due time 本身为 now + N min（due 是 deadline）

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. state field 扩展（~line 2120）

```ts
dueInMinSubmenu: boolean;
```

加进 taskCtxMenu 的 union type；与 prioritySubmenu / reminderSubmenu
同模板。

#### 2. init 处加 default false（~line 8285）

每次右键打开 menu 时初始化 collapsed。

#### 3. H 高度算法（~line 14292）

```ts
const H = (m.prioritySubmenu ? 360 : 300) +
          (m.reminderSubmenu ? 60 : 0) +
          (m.dueInMinSubmenu ? 60 : 0);
```

让 viewport clamp 不切到展开的 sub-menu。

#### 4. submenu 渲染（~line 14982，reminderMin 子面板之后 / sep 之前）

```tsx
{(() => {
  const m3 = taskCtxMenu;
  if (!m3) return null;
  const presets = [5, 15, 30, 60, 120].map(...);
  return (
    <>
      <button onClick={() => setTaskCtxMenu(c => c ? {...c, dueInMinSubmenu: !c.dueInMinSubmenu} : c)}>
        {m3.dueInMinSubmenu ? "▾" : "▸"} ⏰ due in N min
      </button>
      {m3.dueInMinSubmenu && (
        <div style={{ display: "grid", gridTemplateColumns: "repeat(5, 1fr)" }}>
          {presets.map(p => (
            <button onClick={async () => {
              setTaskCtxMenu(null);
              const target = new Date(Date.now() + p.minutes * 60_000);
              const due = formatDueInput(target); // YYYY-MM-DDThh:mm 本地
              await invoke("task_set_due", { title: m3.title, due });
              await reload();
            }}>{p.label}</button>
          ))}
        </div>
      )}
    </>
  );
})()}
```

设计要点：
- Presets `5 分 / 15 分 / 30 分 / 60 分 / 2 小时` 覆盖最常见短期
  due 场景；想要 2h+ 走 ✏️ schedule modal
- `formatDueInput(date)` 把 Date 转 "YYYY-MM-DDThh:mm" 本地时区
  字符串（与既有 task_set_due 协议一致 — 与 `due` chip / 已有 4445
  / 4580 调用路径同）
- `setBusyTitle` 包裹 + `setActionErr` 失败反馈，与既有 task action
  handler 同模式
- `m3` 而非 `m2`（reminderMin 用 `m2`）— 同 file 内 IIFE scope 不
  冲突，但避免读者混淆

## Key design decisions

- **与 snooze preset menu 不重叠**：snooze 是"任务暂停到 X 时刻"，
  due 是"任务的截止时刻"— 两条不同语义路径。owner 在 ✏️ schedule
  modal / 右键 due picker / 本 menu 三处都能设 due，本 menu 专为
  短期场景。
- **preset 阶梯 5/15/30/60/120**：与 snooze preset（30m / tonight /
  tomorrow / monday）阶梯不同 — snooze 偏向"半天 / 一天 / 一周"
  长期推后，本 menu 偏向"分钟 / 小时"短期 deadline。两个 menu 阶
  梯互补不重。
- **`formatDueInput(now + N min)` 而非 ISO with timezone**：与既有
  due 写路径协议一致（YYYY-MM-DDThh:mm 本地时间，无 tz 偏移） —
  避免 toISOString UTC 偏移 8 小时的 footgun。
- **不显当前 due 值在 menu label**：与 reminderMin chip "（当前
  N 分）" 不同 — due 是经常变更字段，每次开 menu 都要查 task 当前
  state；为 UX 简洁不显当前值。owner 想看当前 due 走 row 内 due chip。
- **位置紧贴 reminderMin submenu**：两个"⏰" 相关 sub-menu 同 IDE
  cluster；视觉上 owner 一眼看到 "fire 前 N 分提醒" 与 "设 due
  N 分后" 两条 ⏰ 路径并排。
- **不做 `当前 due-relative` 推算**：preset 永远从 now 算 — 不试图
  "在已有 due 上 +N 分钟"。"now + N min" 是简单 contract，owner
  心智一致。

## Verification

- `npx tsc --noEmit`（frontend）— clean（一次 m2 vs m3 scope 修复）
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动 — 复用既有 task_set_due Tauri 命令 + formatDueInput
  util
