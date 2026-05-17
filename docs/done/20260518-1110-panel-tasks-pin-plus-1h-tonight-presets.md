# PanelTasks 行右键「📌+⏰ 1h / tonight 18:00」preset 组合（iter #469）

## Background

iter #463 加「📌+⏰ 5min 提醒」单击组合 ctx menu 项 — sprint 突发场景
（被中断 5 分钟回来）。但 5min 不能覆盖：
- **开会场景**：会议典型 30..60 min；「📌+⏰ 1h」对偶
- **今晚下班前**：计划性「今晚回家前必须处理」；「📌+⏰ tonight 18:00」
  对偶

本 iter 加两 sibling preset。共享 inline helper `runPinPlusDue(dueArg,
presetLabel)` 避 3 个按钮各 30+ 行 copy-paste。

## Changes

### `src/components/panel/PanelTasks.tsx`

把既有「📌+⏰ 5min 提醒」单按钮重构为 IIFE pattern + 3 按钮（5min / 1h /
tonight 18:00）：

```tsx
{t && !isFinished(t.status) && (() => {
  const runPinPlusDue = async (dueArg: string, presetLabel: string) => {
    setTaskCtxMenu(null);
    setActionErr("");
    setBusyTitle(m.title);
    let pinErr = "", dueErr = "";
    if (!t.pinned) {
      try { await invoke<void>("task_set_pinned", { title: m.title, pinned: true }); }
      catch (e: any) { pinErr = String(e); }
    }
    try { await invoke<void>("task_set_due", { title: m.title, due: dueArg }); }
    catch (e: any) { dueErr = String(e); }
    if (pinErr || dueErr) {
      setActionErr(`📌+⏰ ${presetLabel} 部分失败 — ...`);
    }
    await reload();
    setBusyTitle(null);
  };
  return (
    <>
      <button onClick={() => void runPinPlusDue(formatDueInput(new Date(Date.now() + 5 * 60_000)), "5min")}>
        📌+⏰ 5min 提醒
      </button>
      <button onClick={() => void runPinPlusDue(formatDueInput(new Date(Date.now() + 60 * 60_000)), "1h")}>
        📌+⏰ 1h 提醒
      </button>
      <button onClick={() => void runPinPlusDue(dueTonight(new Date()), "tonight 18:00")}>
        📌+⏰ tonight 18:00
      </button>
    </>
  );
})()}
```

设计：
- **共享 `runPinPlusDue` helper**：闭包内捕获 `m.title` / `t.pinned` /
  `setTaskCtxMenu` 等 — 避 3 按钮 onClick 各自重复 30+ 行 boilerplate
- **三 preset 同 IIFE 内**：共用 `!isFinished` gate（done / cancelled
  设 due 无意义）+ 同 orange tint visual identity；不引嵌套 popover
- **`dueTonight(new Date())` 复用既有 helper**：已 production 验证 —
  返今日 18:00 / 已过则明日 18:00（既有 quickAdd / 调期 popover 同源）
- **错误反馈含 preset label**：`📌+⏰ 5min 部分失败 / 📌+⏰ 1h 部分失
  败 / 📌+⏰ tonight 18:00 部分失败` — owner 知道哪个 preset 出错

## Key design decisions

- **3 个 inline 按钮而非二级 popover**：与既有「📌+⏰ 5min 提醒」单击
  UX 简单原则一致 — owner 单击直接生效。二级 popover 多一步 click 且
  需 outside-click / Esc / hover open 等 popover 维护逻辑，复杂度涨
- **5min / 1h / tonight 18:00 三段时长**：覆盖典型场景 — 突发中断
  / 一小时会 / 今晚下班前。再加 preset（如 +30min / tomorrow morning）
  让 ctx menu 视觉过密；owner 想其它走 📅 调期 popover（既有多 preset
  覆盖）
- **共享 helper 提取到 IIFE 内而非组件顶层**：helper 依赖 `m.title` /
  `t.pinned` / 局部 setActionErr 等 closure 变量，提取到顶层要传一堆
  参数。IIFE 内闭包是 colocated 最少抽象代价
- **「tonight 18:00」label 直接写出时刻而非动态计算**：动态显「today
  18:00」/「tomorrow 18:00」要 dueTonight 暴露日期；label 写死 "tonight
  18:00" 让 owner 知道目标钟点，dueTonight helper 自动跨日不需在 label
  反映（tooltip 已说明"已过则次日"）
- **`!isFinished(t.status)` gate**：与 5min preset 同 gate — done /
  cancelled 设 due 无意义
- **不写 unit test**：纯 UI click + invoke + 闭包封装；逻辑 trivial
  （既有 task_set_pinned / task_set_due / dueTonight backend tests 覆
  盖各自语义）。GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.27s)
- 后端无改动 — 复用既有 task_set_pinned / task_set_due Tauri 命令 +
  dueTonight 助手
- 手测：PanelTasks pending row 右键 → 看「📌+⏰ 5min / 1h / tonight
  18:00」三 entries 紧贴排列 → 点 1h → task 立即 pinned + due 显「📅 in
  1h」；点 tonight 18:00 → due 显「📅 18:00 today」（或 tomorrow 已过
  时刻）
