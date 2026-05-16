# 桌面 pet 右键菜单加「⏰ 倒计时 N 分」nudge

## 背景

owner 工作 / 学习时常想"30 分后提醒一下我"。当前路径要：建一个 butler_task `[once: 2026-MM-DD HH:MM]` —— 还要心算"30 分后的时刻"。或者打开外部 timer app。

桌面 pet 右键菜单本来就在常用 reach；加一组倒计时 preset 让 owner click → 到点 ChatMini 软提醒，零摩擦。

与既有 butler_task `[reminderMin: N]` 不同：
- reminderMin 配 butler_task fire-time 前 N 分钟提醒（task-bound）
- 本 iter 倒计时是 stand-alone 计时（不绑 task，纯计时提醒）

## 改动

### `src/App.tsx`

#### 1. Timer state + cleanup

```ts
const countdownTimersRef = useRef<Set<number>>(new Set());

useEffect(() => () => {
  for (const tid of countdownTimersRef.current) {
    window.clearTimeout(tid);
  }
  countdownTimersRef.current.clear();
}, []);
```

unmount 时清全部 防 timer 漏。

#### 2. `startCountdownNudge(minutes)` handler

```ts
const startCountdownNudge = useCallback((minutes: number) => {
  if (minutes <= 0) return;
  appendAssistant(`⏰ 已设 ${minutes} 分倒计时（到点会浮一条提醒）`);
  const id = window.setTimeout(() => {
    appendAssistant(`⏰ ${minutes} 分倒计时到了 — 该回来看看了 🐾`);
    countdownTimersRef.current.delete(id);
  }, minutes * 60_000);
  countdownTimersRef.current.add(id);
}, [appendAssistant]);
```

- 立即 push 一条 ack（让 owner 知道已设）
- 到点 push 提醒（appendAssistant 软消息，不打开 Live2D proactive 模式）
- timer id 进 Set，cleanup 时统一 clear

#### 3. Pet ctx menu 3 个 preset 按钮（5/15/30 分）

```tsx
{[5, 15, 30].map((m) => (
  <button
    key={m}
    onClick={() => {
      setPetCtxMenu(null);
      startCountdownNudge(m);
    }}
    title={`设 ${m} 分钟倒计时...多个倒计时可并存。`}
  >
    ⏰ 倒计时 {m} 分
  </button>
))}
```

紧贴既有 mute 三按钮（语义反向：mute 安静 vs 倒计时提醒），共 separator 隔开。

#### 4. ctx menu 高度 H 经验值更新

7 buttons → 10 buttons (加 3 个倒计时)；H 从 270 调到 360 给字体放大 / 主题边距浮动留余量。

## 关键设计

- **3 档 preset (5/15/30)**：覆盖 pomodoro 短番茄（5 分微歇）/ 标准番茄（15 / 25 分 → 选 15）/ 半小时块（30 分）。owner 不需自定义就够用大多数场景。
- **多个并存 OK**：Set 接受多个 timer id；owner "设 5 分 + 15 分 + 30 分" 都能 fire。无需互斥。
- **timer id Set 不持久化**：unmount / 刷新 / 重启 pet 时清掉。倒计时是 ephemeral 任务，重启后忘是合理（owner 重启时多半已停下手中工作）。
- **appendAssistant 软消息 + 不打开 Live2D proactive**：与 `[reminderMin: N]` 同设计 —— owner 不希望"30 分到了立刻被拉成 LLM 对话"，仅需要 ChatMini 顶部浮一条"⏰ 到了" 提醒。
- **立即 ack push**：避免 owner 点完按钮怀疑"是不是没生效"。
- **clearTimeout 在 fire 内 + cleanup**：fire 后从 Set 删除自身（防泄漏）+ 全局 unmount cleanup。双重防御。
- **不引入显式取消 UI**：本 iter scope 范围外。owner 真的想取消可以重启 pet。

## 不做

- **不支持自定义 N 分**：要 prompt UI；3 档够用。owner 真要自定义可用 butler_task `[once: ...]`。
- **不显 active countdown chip**："还剩 N 分" 视觉 chip 需要每秒重渲 timer；过度工程。
- **不持久化倒计时**：重启 pet → reset，与 NOW marker 60s 倒计时同 ephemeral 模式。
- **不写测试**：纯 setTimeout + appendAssistant；视觉验证（点 ⏰ 5 分 → 5 分后 ChatMini 应浮 "⏰ 5 分倒计时到了"）足够。
- **不绑键盘快捷键**：右键菜单已经够近 reach。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.18s
- 改动 ~45 行（state + cleanup effect 12 + startCountdownNudge 15 + 3 menu 按钮 15 + H 调整 3）。既有 mute / 主题 / 重启 menu 按钮 / appendAssistant / ctx menu 关闭路径完全不动。

## TODO 状态

剩 0 条 —— TODO 池清空。下个 cron tick 进 auto-propose。

## 后续

- 加 active countdown chip 在 pet 区右下角，显 "⏰ 剩 N:MM" 实时倒计时，让 owner 视觉 ambient 看到剩余。
- ⌘+click 倒计时按钮直接取消最近设的一个 timer。
- 倒计时 fire 时 + Live2D 表情切到"提醒" motion（如 happy / surprise），让视觉反馈更强。
- 加 owner 自定义 preset（settings 内"我的倒计时预设" 5 个 chip）让 5/15/30 不够时也快。
- 与 macOS 通知中心打通：fire 时弹一条 native notification，让 owner 即使切走窗口也能看到。
