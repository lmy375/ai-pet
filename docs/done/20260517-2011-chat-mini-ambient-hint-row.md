# ChatMini 顶部「💡 ambient」hint 行（iter #383）

## Background

iter #381/#382 加 `/aware` `/here` 让 TG 端 owner audit pet 临时上下
文。桌面端要看同样信号需开 PanelDebug / PanelToneStrip — 跨 tab
对常用场景过重。owner 想"瞄一眼 pet 现在被啥影响"理想是 ChatMini
就近一行 hint，免离手即可看到三类关键信号：

1. transient_note — owner 自己留的临时指示
2. active alarms — 桌面 alarm chip + TG /transient 创建的 reminder
3. mute remaining — pet 当前是否被静音

## Changes

### `src/components/ChatMini.tsx`

#### 1. import + state（~line 340）

```tsx
import { invoke } from "@tauri-apps/api/core";

const [ambientTransient, setAmbientTransient] = useState<{
  text: string;
  mins: number;
} | null>(null);
const [ambientAlarms, setAmbientAlarms] = useState<number>(0);
const [ambientMuteMins, setAmbientMuteMins] = useState<number | null>(null);
```

#### 2. polling useEffect

每 30s 并行调三个 Tauri command：
- `get_transient_note` → (text, until_iso)
- `get_pending_reminders` → PendingReminder[]
- `get_mute_until` → ISO string（空 = 未静音）

并发 Promise.all 一次 IPC 周期拿全三值；失败 .catch fallback 空，
不打扰主聊天流。轮询仅 visible 时挂；卸载时 cleanup。

minutes 计算 `ceil((untilMs - now) / 60000)` clamp 最小 1 防过期边
界态显 "0m"。

#### 3. hint row 渲染（chat container 顶部）

```tsx
{ambientHasContent && (
  <div style={{ display: "flex", flexWrap: "wrap", gap: 6, ... }}>
    {ambientTransient && <span>📝 {preview} · {mins}m</span>}
    {ambientAlarms > 0 && <span>⏰ {ambientAlarms}</span>}
    {ambientMuteMins !== null && <span>🔇 {muteMins}m</span>}
  </div>
)}
```

设计要点：
- **全空 → 整行不渲**：avoid idle 态占垂直空间。`ambientHasContent`
  memo 化检查三段是否至少一个 active
- **transient text 30 字截断**：chip 最大 220px 宽 + ellipsis；hover
  title 显完整文本
- **3 段配色区分**：cyan #0891b2（transient）/ blue tint var（alarms）
  / purple #7c3aed（mute）— 与 PanelToneStrip 同 emoji 对应配色
- **monospace 字体 + 小字号 10**：紧凑 hint 风，不抢 chat 视觉重心
- **title attribute 给完整信息**：preview 截断后 hover 显全文 +
  含 N 分钟剩余 + 信号含义解释

## Key design decisions

- **每 30s 轮询 vs prop drilling 父组件**：transient / mute / alarms
  分钟级粒度变化 — 30s tick 足够新鲜（vs `usePollingState` 父组件
  的 60s session-token 轮询节奏）。轮询在 ChatMini 内部避免父组件
  signature 膨胀（onSetTransientNote 等已有 5+ callback）。
- **并发 Promise.all 而非顺序 await**：3 IPC ~平均 5ms 各自；并发
  避免 15ms 串行延迟（虽然轮询场景 not user-facing latency 但保
  consistency）。
- **失败容忍 .catch fallback 空**：网络分区 / IPC 偶发失败 → hint
  行可能显部分信号 / 整行消失；不抛错不阻塞主聊天。
- **visible gate 防 background 轮询**：ChatMini 不可见时（!visible
  早 return）无需 ambient — 节省 IPC 开销。
- **不持久化 ambient 状态**：当 visible 切换 / unmount 重 fetch；
  这是"现在的 snapshot"，跨 session 旧值无意义。
- **30 字 preview cap 而非 20 / 50**：与 `/aware` 60 字、ChatBubble
  excerpt 80 字风格阶梯一致 — chip-level hint 最紧凑（30）。
- **不在 chat 列表内嵌而是 chat container 顶部**：避免与 message
  rows scroll 一起滚；ambient 是 always-visible 状态指示。
- **不为单 polling fn 引 unit test runner**：纯 IPC + setState；
  build pass + 手测足够。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动 — 复用既有 `get_transient_note` / `get_pending_reminders`
  / `get_mute_until` 三 Tauri 命令
