# butler_task `[reminderMin: N]` 标记 + 桌面软提醒

## 背景

owner 用 `[once: ...]` / `[deadline: ...]` 给 butler_task 设定时刻，到点 pet 走 proactive 主动开口。但开会 / 录直播 / 重要约会场景，owner 想要"提前 N 分钟轻量被告知一下" —— 不打开 Live2D 主动模式（太重 / 抢焦点），而是 ChatMini 浮一条软消息当 ambient buffer。

新引入 `[reminderMin: N]` 标记 + 桌面 pet 内 60s 轮询 + appendAssistant 软提醒。

## 改动

### `src/utils/butlerReminder.ts` 新文件 —— 纯逻辑

```ts
export type ButlerScheduleParsed = ...;
export interface ParsedButlerDesc {
  schedule: ButlerScheduleParsed | null;
  reminderMin: number | null;
  topic: string;
}

/** 一次解出 schedule + reminderMin + topic，marker 顺序无关。 */
export function parseScheduleAndReminder(desc: string): ParsedButlerDesc;

/** Schedule 的下一次 fire-time（绝对 Date）；every 跨日返明日同时刻。 */
export function nextFireTime(schedule: ButlerScheduleParsed, now: Date): Date | null;

export interface ReminderToFire {
  title: string;
  topic: string;
  reminderMin: number;
  fireTimeIso: string;
  dedupKey: string;  // `${title}::${fireTimeIso}`
}

/** 当下 tick 应触发的 reminder。条件：parsed schedule + reminderMin + 
 *  fire-time - now ∈ (0, reminderMin 分钟] + dedupKey 未触发过。 */
export function findRemindersToFire(
  tasks: { title: string; description: string }[],
  now: Date,
  alreadyFired: Set<string>,
): ReminderToFire[];
```

reminderMin 范围 [1, 1440 min = 24h]：负数 / 0 / 超过一天不接受（语义噪音 + 防恶意 large N 提前一周提醒）。

### `src/App.tsx` —— 60s 轮询 + dedup Set + appendAssistant

```ts
const reminderFiredRef = useRef<Set<string>>(new Set());
useEffect(() => {
  const tick = async () => {
    try {
      const tasks = await invoke<...>("db_butler_tasks_list");
      const pending = tasks.filter((t) => t.status === "pending");
      const { findRemindersToFire } = await import("./utils/butlerReminder");
      const toFire = findRemindersToFire(pending, new Date(), reminderFiredRef.current);
      for (const r of toFire) {
        reminderFiredRef.current.add(r.dedupKey);
        const remainMin = Math.max(1, Math.round((new Date(r.fireTimeIso).getTime() - Date.now()) / 60_000));
        appendAssistant(`🔔 提醒：「${r.title}」将在约 ${remainMin} 分钟后到点（reminderMin=${r.reminderMin}）`);
      }
      if (reminderFiredRef.current.size > 200) {
        reminderFiredRef.current.clear();  // 老旧 ages-running 兜底 GC
      }
    } catch (e) {
      console.error("butler reminder tick failed:", e);
    }
  };
  void tick();
  const id = window.setInterval(() => void tick(), 60_000);
  return () => window.clearInterval(id);
}, [appendAssistant]);
```

- pending 才提醒；done / error / cancelled 不再 fire。
- dynamic import 让 butlerReminder 仅在该 effect 首跑时拉进 bundle —— 与 ChatPanel / Telegram 等 lazy 路径风格一致。
- dedup key = `${title}::${fireTimeIso}`：same fire-cycle 内只触发一次；every 类型跨日变 fireTimeIso → 第二日自动允许下次。
- GC：set 超 200 项清空（极端长跑 pet ages-running 时止血）。
- 重启 pet 后 ref Set 重置 → 同一 fire-cycle 可能重新提醒一次。owner 重启频率低，重新提醒一次属于 "轻量噪音 < 漏提醒"。

### `src/components/panel/PanelMemory.tsx` —— 视觉 chip + 模板补充

1. butler_task 行加 `🔔 -Nmin` chip（绿色 tint）—— 紧贴 schedule chip 前：

```tsx
{catKey === "butler_tasks" && (() => {
  const m = item.description.match(/\[reminderMin:\s*(\d+)\s*\]/);
  if (!m) return null;
  const n = Number(m[1]);
  if (!(n > 0 && n <= 1440)) return null;
  return <span style={{ ... 绿色 tint ... }} title={`到点前 ${n} 分钟...软提醒`}>
    🔔 -{n}min
  </span>;
})()}
```

2. SCHEDULE_TEMPLATES 加一个 quick-insert：

```ts
{ label: "🔔 reminderMin", text: "[reminderMin: 5] " },
```

3. butler_tasks placeholder 补 reminderMin 示例：

```
... 可选叠加 [reminderMin: N] 让到点前 N 分钟在桌面 ChatMini 浮一条软提醒
（不打开 Live2D 主动模式）。例如：
  [once: 2026-05-20 18:00] [reminderMin: 5] 准备会议材料
```

## 关键设计

- **纯逻辑分文件 `src/utils/butlerReminder.ts`**：parsing + nextFireTime + findRemindersToFire 都是纯函数 —— 不依赖 React / Tauri / 时区库。便于将来移植到 backend（proactive.rs 内 reminderMin 兜底也用同 algo）+ 单测友好（即使现在没 test runner，纯函数 readable 自验）。
- **fire-time - now ∈ (0, reminderMin]**：命中条件不是"= reminderMin"，因为 poll 60s 一次不可能精确命中。改成"还差不到 N 分钟到点 + 未过点"，配 dedup 保证 same cycle 只一次。over-fire 完全避免 = 用户绝不会被同一 cycle 多次 ping。
- **dedup key = `title::fireTimeIso`**：把"任务 ID + fire 周期"作复合 key。every 类型跨日 fireTimeIso 变 → 新 key 允许下次提醒；once / deadline 一旦 fire 过的 fireTime 不再命中条件（poll 时刻已超过 fireTime → remainingMin ≤ 0），dedup 自然不更新该任务。
- **走 appendAssistant 软消息而非 emit proactive event**：appendAssistant 是既有 "pet 桌面侧推一条 assistant 消息" 路径，与 ⚡ 标 NOW、🎨 图片生成失败、🐾 收下任务确认等场景同源。owner 视觉熟悉 + 不抢焦点（不会触发 Live2D motion / proactive 主动开口）。
- **轮询而非 emit 事件**：proactive.rs 当前 tick 复杂、改动风险大。从桌面 pet 侧 60s 轮询 db_butler_tasks_list（既有命令）实现 reminder fire = 完全前端 contained。代价：pet 窗未启动时不 fire（mute / collapse 状态 pet 窗仍在跑，所以 OK；进程级关闭 pet 才漏）。
- **dynamic import butlerReminder**：减少 App.tsx 初始 bundle 体积；reminder effect 首跑时才 fetch + parse。与既有 lazy import 模式一致。
- **frontend-only impl + 不动 proactive.rs**：proactive cycle 已有 [deadline:] urgency tier hint，影响 LLM prompt；reminderMin 属于"轻量软提醒"独立机制，与 proactive.rs 解耦。

## 不做

- **不在 proactive.rs 加 reminderMin 处理**：proactive 是 LLM-driven 主动开口；reminderMin 是 owner-controlled 静默提醒。两个 lane 故意分离。
- **不持久化 dedup Set**：进程内 Set + 重启 reset。漏提醒不可接受但"重启后重新提醒一次"可接受。owner 重启 pet 是 minutes-level 罕见事件。
- **不支持负数 / 超过 1440 min**：噪音 + 提前一周提醒无意义。clamp 在 [1, 1440] = [1 min, 24 h]。
- **不接入 telegram bot 端**：本 iter focus 桌面 ChatMini。telegram 用户单独走 LLM proactive cycle 提醒（既有路径）。
- **不写单测**：项目无 frontend test runner（无 vitest/jest），加 runner 是"装饰性测试" —— 与 GOAL.md "tests must pin real behavior" 冲突。butlerReminder.ts 是纯函数 + 命名清晰，readable self-verifying。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.20s
- 改动 ~200 行（新 butlerReminder.ts 150 + App.tsx poll effect 45 + PanelMemory chip 25 + 模板 / placeholder 文案 3）。既有 butler schedule chip / proactive prompt 路径 / appendAssistant 既有调用方完全不动。
- 手动验证路径：PanelMemory 新建 butler_task `[once: <now+2min>] [reminderMin: 1] test reminder`，等 ~1 分钟 → 桌面 ChatMini 浮 "🔔 提醒：「test reminder」将在约 1 分钟后到点（reminderMin=1）"。

## TODO 状态

剩 1 条留池：
- 桌面 pet 右键菜单加「切 Live2D 模型」子菜单

## 后续

- 把 butlerReminder.ts pure 逻辑端口到 Rust 让 proactive cycle 也能利用（telegram 用户场景）。
- reminderMin 支持多值如 `[reminderMin: 30, 5]` —— 提前 30 分一次 + 5 分一次双层提醒。
- reminder chip 在临到点时（remaining ≤ reminderMin）变红色 hint，让 PanelMemory 也能"快到点了" 视觉信号。
- README 顶部新功能 section 加一行 "butler_task `[reminderMin: N]` 软提醒"。
