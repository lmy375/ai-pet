# PanelTasks 行右键「📌+⏰ 5min 提醒」组合项（iter #463）

## Background

sprint 中常见场景：「现在被中断了，但这条 task 5 分钟后必须回来处理 /
提醒我」。当前 PanelTasks 右键 ctx menu 有 📌 钉住 + 📅 调期 popover
（独立两步）。owner 想「pin + 5min later overdue」要点两次 + 选 preset。

本 iter 加 ctx menu 「📌+⏰ 5min 提醒」 单击组合项 — 钉住 + 设 due 为
5 分钟后。task 立即浮 📌 顶 + 5 min 后进入 overdue 触发 pet proactive
关注 + due chip 红警示。

## Changes

### `src/components/panel/PanelTasks.tsx`

紧贴 📌 钉住 ctx menu 按钮之后插组合按钮：

```tsx
{t && !isFinished(t.status) && (
  <button
    onClick={async () => {
      setTaskCtxMenu(null);
      setActionErr("");
      setBusyTitle(m.title);
      const dueArg = formatDueInput(new Date(Date.now() + 5 * 60_000));
      let pinErr = "";
      let dueErr = "";
      if (!t.pinned) {
        try {
          await invoke<void>("task_set_pinned", { title: m.title, pinned: true });
        } catch (e: any) { pinErr = String(e); }
      }
      try {
        await invoke<void>("task_set_due", { title: m.title, due: dueArg });
      } catch (e: any) { dueErr = String(e); }
      if (pinErr || dueErr) {
        setActionErr(`📌+⏰ 部分失败 — ${[pinErr && `pin: ${pinErr}`, dueErr && `due: ${dueErr}`].filter(Boolean).join(" · ")}`);
      }
      await reload();
      setBusyTitle(null);
    }}
    title="sprint 突发：一键钉住 + 设 due 为 5 分钟后。task 立即浮到 📌 顶；5 min 后进入 overdue 触发 pet proactive 关注。已 pinned 时仅设 due。"
  >
    📌+⏰ 5min 提醒
  </button>
)}
```

设计：
- **`!isFinished(t.status)` gate**：done / cancelled 设 due 无意义 —
  与 既有 📅 调期 chip 同 gate
- **`!t.pinned` 短路**：已 pinned 时跳过 pin 调用直接设 due — 避免无
  意义写 + 与既有 task_set_pinned 单调用 idempotent 模式一致
- **顺序而非并行 invoke**：两调用都改同 task description；并行会引
  race（两路径都先 strip markers → 后写覆盖前写）。顺序串行让两动作
  无歧义
- **部分失败累积不阻断**：先 pin（失败 → 记 pinErr），再 due（失败 →
  记 dueErr）；任一失败 setActionErr 显具体哪步错；reload 仍走 — 保
  view 与 backend 一致
- **`formatDueInput(new Date(Date.now() + 5 * 60_000))`**：与既有 📅
  调期 popover 「+1h / +1d / +3d」preset 同算法（now + ms → 本地时间
  ISO）；不引新时间 helper

## Key design decisions

- **5 分钟为何 5（不参数化）**：sprint「中断 / 5 分钟后回来」场景的
  常识值。≤ 1 分 太短（pet 来不及 proactive cycle）；≥ 15 分 偏长（已
  脱离"现在被中断"心智）。固定值让单击 UX 简单 — 想精确控制走 📅 调
  期 popover 选 +1h / 自定义 datetime-local
- **位置紧贴 📌 钉住之后**：两按钮语义紧密关联（都涉及 pinned 状态），
  视觉相邻让 owner 心智「📌 单独 vs 📌+⏰ 复合」对比明显
- **icon `📌+⏰` 双 emoji**：与 既有 「⚡ mark NOW」/「📌 钉住」/
  「📅 调期」单 emoji 同 chip family 但用 `+` 视觉表达「组合」 — 与
  「⚡ mark NOW (60s)」的 60s 短期 vs 本组合的 5min 中期对比清晰
- **不引「📌+⏰ 自定义分钟数」二级 popover**：与单击 UX 简单原则冲
  突。owner 要其它分钟数走 📅 调期 popover —— 那个已 5 preset 覆盖
  +1h / +1d / +3d / +1w / +2w
- **不写 unit test**：纯 invoke + UI click + setActionErr 副作用；逻
  辑 trivial（既有 task_set_pinned / task_set_due backend tests 已覆盖
  各自语义）。GOAL.md "meaningful tests only" 规则下不引装饰性测试。
  `tsc + vite build` clean + 手测足够
- **不引「📌+⏰5min 后剥 [pinned]」自动清理**：自动 unpin 让 owner 失
  去「这条还重要」的视觉信号。让 owner 主动决定 unpin 时机更可控；
  与 5min 后 due 自动 overdue 是两个独立信号

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.25s)
- 后端无改动 — 复用 task_set_pinned / task_set_due Tauri 命令
- 手测：PanelTasks pending row 右键 → 看「📌+⏰ 5min 提醒」入口在 📌
  钉住 之后 → click → task 立即 pinned 浮顶 + due chip 显「📅 in 5
  min」→ 5 分钟后 chip 转红 overdue
