# PanelMemory edit-schedule modal 扩支 every_weekdays

## 背景

iter #204 backend + 大部分前端已支持 `[every: 工作日 09:00]` weekday-set 任务，但 edit-schedule modal 暂留 disable —— owner 想改 weekday-set 任务的时间 / 周几集合，仍要手动编辑 description 字面量。本 iter 把 modal 扩为支持 every_weekdays：4 个 kind 选项 + 7 个 weekday checkbox grid + "工作日 / 周末 / 每天 / 清空"快捷按钮。

## 改动

### `src/components/panel/PanelMemory.tsx`

#### 1. EditScheduleDraft 类型扩

```ts
type EditScheduleDraft = {
  title: string;
  description: string;
  kind: "every" | "every_weekdays" | "once" | "deadline";  // 加 every_weekdays
  date: string;
  time: string;
  weekdayMask: number;  // 新字段：7-bit mask, bit 0 = Mon
};
```

`weekdayMask` 仅 every_weekdays 用；其它 kind 时存默认 0b1111111 占位（不影响保存）。

#### 2. focus useEffect 扩

every_weekdays 与 every 同走 time input focus（无 date input）：

```ts
if (kind === "every" || kind === "every_weekdays") {
  editScheduleTimeRef.current?.focus();
}
```

#### 3. row-level "✏️ 改 schedule" 按钮取消 every_weekdays disable

```tsx
{catKey === "butler_tasks" && parsed && (
  <button onClick={() => {
    const s = parsed.schedule;
    setEditScheduleDraft({
      title: item.title,
      description: item.description,
      kind: s.kind,
      date: s.kind === "once" || s.kind === "deadline"
        ? `${y}-${m}-${d}` : "",
      time: `${hh}:${mm}`,
      weekdayMask: s.kind === "every_weekdays" ? s.mask : 0b1111111,
    });
  }} ...>
)}
```

#### 4. modal select 加 4th option

```tsx
<option value="every">🔁 every（每天定时）</option>
<option value="every_weekdays">🔁 every_weekdays（按周几定时）</option>  // 新
<option value="once">📅 once（单次定时）</option>
<option value="deadline">⏳ deadline（截止前提醒）</option>
```

#### 5. every_weekdays 条件渲染 weekday 选区

```tsx
{editScheduleDraft.kind === "every_weekdays" && (
  <div>
    <label>weekday 集合（至少选 1 天）</label>
    {/* 工作日 / 周末 / 每天 / 清空 快捷按钮 */}
    <div style={...flex gap 4...}>
      {[{label: "工作日", mask: WEEKDAY_MASK_WORKDAYS}, ...].map(p => (
        <button onClick={() => setMask(p.mask)} active={mask === p.mask}>
          {p.label}
        </button>
      ))}
    </div>
    {/* 7 个 weekday checkbox grid */}
    <div style={{display: "grid", gridTemplateColumns: "repeat(7, 1fr)", gap: 4}}>
      {["一","二","三","四","五","六","日"].map((label, i) => {
        const bit = 1 << i;
        const checked = (mask & bit) !== 0;
        return <label onChange={() => setMask(mask ^ bit)} checked={checked}>
          <span>周{label}</span>
        </label>;
      })}
    </div>
  </div>
)}
```

- 快捷按钮 active 视觉态：accent border + 蓝 tint bg + 粗体
- weekday grid label-as-checkbox style：active 同 accent；隐 native `<input>`，所有 click 通过 label 触发 toggle
- 周一-周日 7 个独立 checkbox + 4 个 quick-set 按钮 互补

#### 6. 日期 input 显示条件扩到 once / deadline only

```tsx
{(editScheduleDraft.kind === "once" || editScheduleDraft.kind === "deadline") && (
  <div>...date input...</div>
)}
```

every / every_weekdays 都不显日期 input。

#### 7. save handler 扩

```ts
if (d.kind === "every_weekdays" && d.weekdayMask === 0) {
  setMessage("at least 选 1 个 weekday，或切到「🔁 every（每天）」");
  return;
}
const newPrefix =
  d.kind === "every"
    ? `[every: ${d.time}]`
    : d.kind === "every_weekdays"
      ? (d.weekdayMask === 0b1111111
        ? `[every: ${d.time}]`  // 全 7 选 → 等价 every，自动退化
        : `[every: ${formatWeekdayMaskLabel(d.weekdayMask)} ${d.time}]`)
      : `[${d.kind}: ${d.date} ${d.time}]`;
```

- 全 7 选时自动退化到 `[every: HH:MM]`（少 keyword 字符 + 语义等价）
- 0 选拒绝
- 其它 mask 用 `formatWeekdayMaskLabel` 拿 "工作日" / "周末" / "周一/三/五" 等可读 label

## 关键设计

- **保留 weekday mask 与 every 的双向迁移**：kind 切换不丢 mask 值；从 every 切到 every_weekdays 时 mask 已是 default 0b1111111；从 once/deadline 切回 every_weekdays 时若之前是 every_weekdays 任务则保留原 mask。
- **0 mask 拒绝 + 7 mask 退化**：边界值都给可识别行为而非崩溃。
- **label-as-checkbox + hidden native input**：让整个 weekday 格子可点（不只 checkbox 那一小块），UX 更友好。css-only active 态高亮。
- **快捷按钮 + 单选 grid 互补**：常用集合（工作日 / 周末 / 每天）一键设；个性化集合通过单 checkbox 自由切。
- **focus 同 every（time input）**：every_weekdays 不需 date，与 every 同行为 keyboard 输入流。

## 不做

- **不绑 keyboard shortcut（数字 1-7 切 weekday）**：UI 已经够直观；快捷键反而多记忆负担。
- **不显 mask 数字 / preview "本周还会 fire X 次"**：visual 占位 + 计算成本；owner 选 mask 时心智清晰 enough。
- **不写测试**：纯 React UI；既有 every_weekdays parser / mostRecentFire / 显示链路 iter #204 已 cargo + tsc 覆盖。视觉验证（开 modal → 切 every_weekdays → 选周一周三周五 + 09:00 → 保存 → row 显 "🔁 周一/三/五 09:00"）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.16s
- 改动 ~130 行（draft type 5 + focus 5 + row-button gate 撤 + populate mask 8 + select 1 option + 条件渲 weekday grid 90 + save logic 18）。既有 once / deadline / every modal 路径 / formatWeekdayMaskLabel / 既有 input style 完全不动。

## TODO 状态

剩 0 条 —— TODO 池清空。下个 cron tick 进 auto-propose 分支。

## 后续

- modal 内 active mask preview 实时显 "🔁 工作日 09:00" / "🔁 周一/三/五 09:00" 字符串预览，让 owner 保存前确认 description 长啥样。
- 加 "📋 复制 weekday 集合给其它任务" 一键 —— 当 owner 想"我有 5 个任务都按周末 10:00 触发"时省事。
- weekday set 拓展到 `[once: 周一 09:00]` —— "下个周一 09:00" 单次任务（语义类似 snooze monday 单次执行）。
- 把 modal kind 切换时自动按上下文给默认值：从 every → every_weekdays 时默认 mask = WEEKDAY_MASK_WORKDAYS（工作日常用最高频）。
