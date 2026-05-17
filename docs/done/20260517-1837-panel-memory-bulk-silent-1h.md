# PanelMemory butler_tasks「⏸ 全部 silent 1h」批量按钮（iter #366）

## Background

owner 想开会 / 集中写文档 1 小时不被 pet 主动 "我看 Downloads 该
整理了" 打扰，但不想 set_mute_minutes（那会让 pet 整体不开口连
聊天都不行）— 想要的是"把 butler_task 候选池清空，pet 仍可主动
聊天但不会派任务"的细粒度静默。

既有 task_set_silent(title, true) 后端可以做 per-task 静默，但
没有批量 + 时间窗口包装。本 iter 加桌面 UI 一键入口。

## Changes

### `src/components/panel/PanelMemory.tsx`

#### 1. state（~line 280）

```ts
type BulkSilentSnapshot = { titles: string[]; expiresAt: number };
const BULK_SILENT_STORAGE_KEY = "pet-panel-memory-bulk-silent-snapshot";
const BULK_SILENT_DURATION_MS = 60 * 60 * 1000;
const [bulkSilentSnapshot, setBulkSilentSnapshot] = useState(...);
const [bulkSilentBusy, setBulkSilentBusy] = useState(false);
const bulkSilentExpiryTimerRef = useRef<number | null>(null);
const [bulkSilentNowMs, setBulkSilentNowMs] = useState(() => Date.now());
```

- snapshot 仅记"被本次按钮置 silent" 的 titles 子集（filter 掉原本
  已 [silent] 的）— 到期不会把 owner 手动标 silent 的也撤掉
- localStorage 持久化 → app 重启仍能继续

#### 2. handlers

- `releaseBulkSilent(snapshot)` — 逐条 task_set_silent(title, false)，
  清 state / localStorage / timer，loadIndex 刷新。失败容忍（task 可
  能已被 owner 手动 unsilent / 删除）。
- `triggerBulkSilent(candidates)` — 过滤 `pending && !silent` 子集，
  逐条 task_set_silent(title, true)，写 snapshot，arm 1h timer。
- mount useEffect — 读 localStorage：错过窗口（expiresAt < now）立
  即解除作兜底；否则 re-arm timer for remaining duration。
- "剩 N 分" tick useEffect — snapshot 非空时每 60s 更新 bulkSilentNowMs，
  让 button label 自然下落；snapshot 清空时停 tick 省电。

#### 3. 按钮 UI（~line 4253，butler_tasks 段顶部按钮组）

紧贴 `📋 今日 todo` 按钮：
- **inactive 态**：扫 `pending && !silent` candidates；0 时不渲；否
  则显 `⏸ 全部 silent 1h (N)`，click → triggerBulkSilent(candidates)
- **active 态**：amber tint 表"临时态进行中"，文案 `🔊 解除 (剩 N
  分)`，click → 立即手动解除（早于到期）

## Key design decisions

- **frontend timer + localStorage 而非新 backend 字段**：避免 schema
  膨胀。timer 死掉的兜底已 mount useEffect 处理（错过窗口立即撤回）。
- **snapshot 仅 titles 子集（filter 原本非 silent 的）**：1h 后到
  期撤回时，只撤回 *本次* 按钮加的 [silent]，不会误碰 owner 手动
  标的 [silent]。是关键的"非破坏性"语义。
- **vs `set_mute_minutes(60)` 区别**：
  - mute → pet 整体不开口（不能主动聊天）
  - 本按钮 → 仅 butler_task 候选池清空，pet 仍可主动聊天 / 回 chat
  按钮 title 明示了这层差异，避免 owner 误用。
- **active 态显式倒计时**：让 owner 一眼知"还剩多久自动放风"，避
  免"我点了按钮但忘了什么时候过期"的焦虑。1 min 精度足够（每 60s
  tick），秒级精度无意义。
- **手动早解除入口 = 同一按钮 active 态 click**：不另加按钮，让 UI
  紧凑。amber → cyan 配色差异 + 文案明确"解除"已足够区分。
- **0 candidates 时不渲按钮（vs disabled）**：与 `📋 今日 todo`
  同模板（todayItems == 0 时 return null）。dead button 增加 UI 噪音
  且暗示 "可能能用但不行" — 干脆不渲。
- **不持久化"剩余分钟"显示精度偏好**：每个 owner 都看 minutes 桶
  够直觉，加偏好开关过度工程化。
- **不暴露 30m / 2h 时长选项**：1h 是经典"一节课 / 一个会"时长，覆盖
  绝大多数用例。想精细化走 PanelTasks 手动 [silent] marker 编辑（每
  task 单独）+ TG `/silent <title>`。
- **不为单 fn 引 frontend test runner**：项目无 .test.tsx；
  按钮逻辑是 IO + state ops，build pass + 手测即足。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.26s)
- 后端无改动 — 复用既有 task_set_silent Tauri command
