# PanelMemory butler_task 行 [reminderMin: N] chip click 弹快速编辑

## 背景

iter #190 加了 reminderMin marker 软提醒 + PanelMemory 行显 🔔 -Nmin 灰 chip。但 owner 想改 N 值（从 5 改 15 / 清掉）必须手动编辑 description 字面量。

类比 schedule chip 旁有 "✏️ 改 schedule" 快速编辑按钮（iter #210 还扩成支持 every_weekdays），给 reminderMin chip 也加 click → 弹小 modal 快速改 N。

## 改动

### `src/components/panel/PanelMemory.tsx`

#### 1. `reminderEditDraft` state

```ts
const [reminderEditDraft, setReminderEditDraft] = useState<{
  title: string;
  description: string;
  n: number | "";  // "" = 自定义 input 清空中
} | null>(null);
const [reminderEditBusy, setReminderEditBusy] = useState(false);
```

#### 2. reminderMin chip 从 `<span>` 改 `<button>`

```tsx
<button
  type="button"
  onClick={() => setReminderEditDraft({title, description, n})}
  style={{...原 chip style + border: "none", cursor: "pointer"}}
  title="...点击快速编辑（5/15/30 preset 或自定义 / 清除）"
>
  🔔 -{n}min
</button>
```

#### 3. 新 Modal（紧贴既有 edit-schedule modal 之后）

```tsx
<Modal open={reminderEditDraft !== null} onClose={...} maxWidth={340}>
  <div>
    <div>🔔 改 reminderMin —「{title}」</div>
    <div>到点前 N 分钟在桌面 ChatMini 浮软提醒...</div>
    {/* preset 行：5/15/30 一击 set */}
    <div>{[5, 15, 30].map((p) => <button active={n === p}>{p} 分</button>)}</div>
    {/* 自定义 input */}
    <input type="number" min={1} max={1440} value={n} onChange={...} />
    {/* 操作按钮：清除 / 取消 / 保存 */}
    <div>
      <button onClick={清除}>🗑 清除</button>
      <button onClick={取消}>取消</button>
      <button onClick={保存}>{busy ? "保存中…" : "保存"}</button>
    </div>
  </div>
</Modal>
```

#### 4. 保存 logic

```ts
const stripped = description.replace(/\[reminderMin:\s*\d+\s*\]/g, "").replace(/\s+/g, " ").trim();
const newDesc = stripped ? `${stripped} [reminderMin: ${num}]` : `[reminderMin: ${num}]`;
await invoke("memory_edit", { action: "update", category: "butler_tasks", title, description: newDesc });
```

clear logic 仅 strip。两路径走完 setReminderEditDraft(null) + loadIndex 重新渲染列表。

## 关键设计

- **chip 改 button**：保留视觉（绿 tint chip 样式）+ 加 cursor: pointer + border: none + onClick。一致 chip 形态。
- **strip-before-write**：与既有 task_set_pinned / task_set_snooze 同模板 — strip 所有 `[reminderMin: ...]` + 空白归一 + 追加新 marker。多次切换 description 不会累积冗余 marker。
- **N 范围 [1, 1440]**：与既有 parse_snooze_token / butlerReminder.ts 上限一致（24h 内）。
- **3 preset + 自定义**：5（pomodoro 子段）/ 15（标准提醒）/ 30（半小时 buffer）覆盖最常用；自定义留尾巴给特殊 case。
- **active preset 视觉态**：accent border + 蓝 tint bg + 粗体，与 edit-schedule modal 工作日 / 周末 快捷按钮 active 态同视觉语言。
- **清除按钮在左下**：与 取消 / 保存 视觉分组分离 —— 清除是"撤销 marker"独立 destructive 动作。
- **modal zIndex=110 + edit-schedule modal 同层**：两 modal 都可能存在但同时不打开；同 z 不冲突。
- **只对已有 marker 的 chip 弹**：本 iter 不支持"添加新 reminderMin 到没标过的任务"—— 那走 schedule 编辑器或手动写字面量。chip 只代表"已有 marker 想改"。

## 不做

- **不支持"给没标过 reminderMin 的任务一键加"**：scope creep；schedule chip click 已经能 add marker via "✏️ 改 schedule" 模式（虽然目前 modal 不暴露 reminderMin field —— 留下 iter）。
- **不绑键盘快捷**：modal 内 input focus 时 owner 按 Enter 提交即可（自然 form behavior）—— 但本 iter 没显式绑，加 onKeyDown 可后续。
- **不写测试**：纯 React state + clipboard / memory_edit IPC 已验证；视觉验证（一个含 reminderMin 的 butler_task 行 → click 🔔 chip → modal 弹 → 选 15 → 保存 → chip 显 -15min）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.19s
- 改动 ~200 行（state 14 + chip 改 button 10 + Modal 170 + 注释）。既有 reminderMin 解析 / 行渲染 / schedule chip / silent chip / pinned 路径完全不动。

## TODO 状态

剩 0 条 —— TODO 池清空。下个 cron tick 进 auto-propose 分支。

## 后续

- 把 reminderMin 编辑 collapse 到 edit-schedule modal 的一个 field 里（owner 一次开 modal 改 schedule + reminderMin + weekday）—— 单 modal 统一编辑入口。
- chip click 直接进 mini popup（绝对定位浮 chip 右侧）而非全屏 modal —— 减一步视觉跳。
- "🔔 +" 按钮在未标过的 butler_task 旁让 owner 一键加 reminderMin，与现行 chip click 编辑对偶。
