# PanelTasks 行 💤 snooze chip click 弹 preset popover

## 背景

iter #200 给 task_set_snooze backend 加了 EN/CJK preset 关键词解析（tonight / tomorrow / monday / 今晚 / 明早 / 30m / 30分 / 2小时 etc.）。任务行右键菜单已有 4 个 snooze 预设 + 解除按钮。

但 owner 看到行上的 💤 snooze chip 时，自然想法是 "click 这个 chip 改 / 解除"。当前 chip 是 read-only `<span>` —— 必须再走右键菜单 (4 步)。加 click → mini popover 直达。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### 1. 新 `snoozePickerTitle: string | null` state

```ts
const [snoozePickerTitle, setSnoozePickerTitle] = useState<string | null>(null);
```

#### 2. union 接入既有 click-outside / Esc 关 popover effect

既有 useEffect 已统一关 priorityPickerTitle / statusPickerTitle / taskCtxMenu / tagColorPicker。把 snoozePickerTitle 加入 deps + close 列表。

#### 3. 💤 chip 改 `<button>` + mini popover

```tsx
<span style={{position:"relative", display:"inline-block"}}>
  <button
    onClick={(e) => {
      e.stopPropagation();
      setSnoozePickerTitle((cur) => cur === t.title ? null : t.title);
    }}
    style={{...原 chip style + cursor:"pointer" + fontFamily:"inherit"}}
    title="...点击改 / 解除"
  >
    💤 至 {short}
  </button>
  {open && (
    <div style={{position:"absolute", top:"calc(100% + 4px)", left:0, ...popover style...}}>
      4 个 preset 按钮:
        💤 暂停 30 分 (key: "30m")
        💤 至今晚 18:00 (key: "tonight")
        💤 至明早 09:00 (key: "tomorrow")
        💤 至下周一 09:00 (key: "monday")
      separator
      ☀️ 解除暂停 (until: null)
    </div>
  )}
</span>
```

preset key 直接传给 `invoke("task_set_snooze", { title, until: p.key })` —— backend (iter #200) 接受 "30m" / "tonight" / "tomorrow" / "monday" / 中文别名等 preset 串。

## 关键设计

- **复用 task_set_snooze backend preset 入参**：iter #200 加了 parse_snooze_token 接受 EN/CJK preset；前端无需 JS 算绝对时刻，直接传 "tonight" / "monday" 让 backend 解析 `now` 为标准 YYYY-MM-DD HH:MM。
- **mini popover anchored 在 chip 下方**：with absolute positioning + zIndex 30，让 owner 视觉锁定"这 popover 跟此 chip 关联"。
- **复用既有 outside-click / Esc 关 effect**：与 priorityPicker / statusPicker / tagColorPicker 同一 useEffect 内 union 接入 — 一个 effect 管理多个 popover。
- **解除按钮独立 separator + accent 色**：destructive 反向动作（清掉 marker）单独视觉分组，与 4 个 preset 设置动作错开。
- **busyTitle 守 + setActionErr 反馈**：与既有 任务行 操作（snooze / pin / silent）同模板。
- **chip 仍显完整时刻 `💤 至 MM-DD HH:MM`**：click 后下面浮 popover 选 preset；与既有 chip 文案 UX 一致。

## 不做

- **不写自定义 N 分钟 input**：preset 4 档已覆盖常用；自定义场景用右键菜单 + datetime-local input。
- **不显当前时刻预览**："至明早 09:00" 明确，不需 hover 显具体时刻。
- **不写测试**：纯 React state + 既有 task_set_snooze IPC；视觉验证（一个含 snooze 的行 → click 💤 chip → popover 弹 → 选预设 → row 显新 snooze 时刻）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.19s
- 改动 ~140 行（state 6 + outside-click union 8 + chip → button + popover 渲染 125）。既有 task_set_snooze backend / 右键菜单 4 个 snooze 预设 / chip 视觉 / busyTitle 路径完全不动。

## TODO 状态

剩 3 条留池：
- detail.md 编辑器 toolbar "📋 复制选中段 → 新 task"
- PanelTasks "+ 新建" chip 显未读 / 错误任务计数
- pet 区右键加「📡 ping LLM 测延迟」

## 后续

- 同模板给 task row 顶部 due chip click → datetime-local input + 4 个 due preset 一键。
- popover 加 footer hint "右键菜单可定义任意时刻" 让 owner 知道还有 escape hatch。
- snooze popover 内显当前时刻 "你目前暂停至 ..." inline echo，让 owner click "明早" 时知道"等同于把现有 snooze 改成 ..."。
