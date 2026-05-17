# ChatMini 顶部「⏱ pet 沉默 N 分」chip（iter #462）

## Background

ChatMini ambient hint 行已含 transient_note / alarms / mute 三 chips
让 owner 一眼知「pet 现在感知到什么」。但缺一个 chip：**pet 自上次主动
开口 / 回复后沉默多久**。owner 想觉察「pet 是不是又卡住了 / proactive
pipeline 是不是没在跑」时，当前没直接信号 — 要切到 PanelDebug 或翻
chat 看时间戳。

本 iter 加 ⏱ pet silent chip — 计算自上次 assistant message ts 起
经过的分钟数，severity 分三档（默认 muted / 30+ amber / 90+ red），仅
≥ 5 min 时显（避免 pet 刚说完闪 chip 噪音）。

## Changes

### `src/components/ChatMini.tsx`

#### 1. `silentTick` + `petSilentMins` computation

```ts
const [silentTick, setSilentTick] = useState(0);
useEffect(() => {
  const id = window.setInterval(() => setSilentTick((t) => t + 1), 30_000);
  return () => window.clearInterval(id);
}, []);
const petSilentMins = useMemo<number | null>(() => {
  void silentTick;
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i];
    if (m.role !== "assistant") continue;
    const raw = m.ts;
    if (!raw) continue;
    const t = Date.parse(raw);
    if (Number.isNaN(t)) continue;
    return Math.floor((Date.now() - t) / 60_000);
  }
  return null;
}, [messages, silentTick]);
const showPetSilentChip = petSilentMins !== null && petSilentMins >= 5;
```

设计：
- **`silentTick` 30s 独立 tick**：与既有 `nowTick`（1s，NOW marks 专
  用，仅有 marks 时启用）分开 — 后者条件性 + 高频，不适合"分钟级 display
  的稳定刷新" duty。30s 节奏让分钟数字最多 30s 滞后于真实，分钟级 UX
  无感
- **反向遍历找最近 assistant**：messages 是时间升序；从末向前找首个
  `role=assistant` + valid ts。messages 已被 caller cap 到 MINI_CHAT_MAX_ITEMS
  量级，O(N) 可接受
- **`null` 表示"无可用 assistant ts"**：新机 / session 刚 reset / 全
  user-only 消息时返 null → chip 自动隐藏

#### 2. ambientHasContent 加入 showPetSilentChip

```ts
const ambientHasContent =
  ambientTransient !== null ||
  ambientAlarms > 0 ||
  ambientMuteMins !== null ||
  showPetSilentChip;
```

让本 chip 单独可触发整行可见 — 即使 transient / alarms / mute 都为空，
"⏱ pet 沉默 30m" 也能让行渲染。

#### 3. chip JSX（紧贴 🔇 mute chip 之后）

```tsx
{showPetSilentChip && petSilentMins !== null && (() => {
  const mins = petSilentMins;
  let bg: string, fg: string;
  if (mins >= 90) { bg = #dc2626-14%; fg = "#dc2626"; }     // 红
  else if (mins >= 30) { bg = #d97706-14%; fg = "#d97706"; } // 琥珀
  else { bg = fg-8%; fg = muted; }                            // 默认 muted
  const label = mins < 60 ? `${mins}m` : `${Math.floor(mins/60)}h${mins%60}m`;
  const sev = mins >= 90 ? "🔴 长时间沉默 — 检查 proactive pipeline 是否卡住"
           : mins >= 30 ? "🟡 偏久 — 可能正在等 mute / silent / 长 cron 间隔"
                       : "默认节奏";
  return (
    <button
      onClick={() => { ...写 debug deeplink + invoke open_debug }}
      title={`pet 自上次主动 / 回复以来已沉默 ${mins} 分钟。${sev}\n\n点击 → 打开 debug 窗 + 滚到 ToneStrip 看 mute / transient / proactive 状态。`}
      style={{ ...same as sibling chips, background: bg, color: fg }}
    >
      ⏱ {label}
    </button>
  );
})()}
```

设计：
- **3 档 severity**：5..30 默认 muted 灰（"正常节奏"）；30..90 琥珀
  （"偏久值得注意"）；≥ 90 红（"明显卡住"）。颜色与既有 ChatMini chip
  调色 inline literal 同协议（`#dc2626` / `#d97706` 与 alarms / mute
  chip 同 tier 颜色风格）
- **mins ≥ 60 用 `Nh Mm` 格式**：分钟数 200 显「3h 20m」比「200m」直觉。
  数值边界对齐分钟 — 不引入秒级
- **click → debug deeplink + ToneStrip 锚点**：与 transient_note / mute
  chip 同 click target — owner 想 audit「为啥沉默」时一键到 ToneStrip
  看 mute / transient / proactive cooldown 信号
- **tooltip 含 severity 解释**：把 chip 状态 + 解释 + 跳转 hint 全在
  tooltip 里 — chip 自身保留 emoji + 数值最简

## Key design decisions

- **5 分钟下限**：pet 主动开口 / reply 后 1-2 分钟立刻闪「⏱ 1m」是
  视觉噪音 — 那时 owner 心智里 pet 还"刚刚说过"。5 分作为「值得监控」
  的入口阈值
- **`silentTick` 而非复用 `nowTick`**：nowTick 条件性（nowTasks > 0 才
  启 1s tick）+ 高频 1s（render 每秒触发对所有 useMemo 影响 — 不健康）。
  本 chip 独立 30s tick 让节奏专属 + 与其它 ambient chips 30s polling
  对齐
- **不轮询服务端**：本 chip 数据完全派生自 `messages` prop（已 push
  到 React state），不需要 IO；30s tick 仅触发"现在距上次 N 分钟"重算
- **三档 severity 色定到 inline literal**：与既有 alarms 红 / mute 紫
  chip 同 inline-color 协议（不引入新 CSS var）；颜色 token 已固定让
  跨 chip 视觉一致性
- **不引入"已 mute 时隐藏 silent chip"短路**：owner mute pet 期间
  自然不会有 assistant message → silent 分钟数会持续累积，chip 会
  逐档变红。这是**真实**信号 — pet 真没在说话；mute 解除后 pet 立刻
  开口 → chip 重置。强制隐藏会丢「pet 解除 mute 后是不是立刻开口」的
  关键观察
- **不写 unit test**：纯数据派生 + render；逻辑 trivial（messages 反
  向遍历 + Date.parse + 分钟 floor）；`tsc + vite build` clean 即够。
  GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.33s)
- 后端无改动 — 纯前端派生 chip
- 手测：等 pet 主动开口（或自己发消息让 pet 回复）→ 5 分钟后看 ambient
  行出现「⏱ Nm」灰 chip → 等 30 分钟变琥珀 → 等 90 分钟变红；hover
  tooltip 显 severity 解释 + 跳转 hint；click → debug 窗 + ToneStrip
  锚点
