# MoodWidget 双击展开 7 天心情 sparkline 浮窗

## 背景

TODO（auto-proposed 之前）：

> mood widget 双击展开 7 天 sparkline 浮窗：快速回看心情趋势不必开 Panel。

MoodWidget 当前 hover 弹一个 ring buffer 的最近 6 条 mood snapshot（session 内的本地缓存），但要看真实"过去 7 天"心情曲线得切到 PanelPersona —— 3 步操作，而且会暂时离开桌面陪伴语境。给 widget 加个轻量双击浮窗，复用 PanelPersona 用的同一份 `get_mood_daily_motions(days=7)` 后端数据，让"想看心情走势"是 1 步操作。

## 改动（frontend only）

### `src/App.tsx`

**1. 模块顶补 motion 色 + helper**

```ts
const MOOD_COLOR: Record<string, string> = {
  Tap: "#ec4899", Flick: "#f59e0b", Flick3: "#ea580c", Idle: "#64748b",
};
const MOOD_COLOR_FALLBACK = "#cbd5e1";

interface DailyMotionPayload {
  date: string;            // YYYY-MM-DD
  motions: Record<string, number>;
  total: number;
}

function topMotion(motions: Record<string, number>): string | null {
  // 同票时 keys 排序后取第一个让结果决定性（不依赖对象迭代顺序）
}
```

颜色与 PanelPersona MOTION_META 完全一致，但本地定义避免 App.tsx import 进 panel-only 文件造成 bundle 偶联。`topMotion` 单独抽出便于将来单测（前端无 vitest 时仍是清楚的纯函数）。

**2. MoodWidget 内新增 4 个 state + 2 effect**

```ts
const [sparklineOpen, setSparklineOpen] = useState(false);
const [daily7, setDaily7] = useState<DailyMotionPayload[] | null>(null);
const [loadingDaily, setLoadingDaily] = useState(false);
const sparklineFetchOnceRef = useRef(false);

// lazy fetch：首次打开拉一次，关闭后保留缓存
useEffect(() => {
  if (!sparklineOpen) return;
  if (sparklineFetchOnceRef.current) return;
  sparklineFetchOnceRef.current = true;
  setLoadingDaily(true);
  invoke<DailyMotionPayload[]>("get_mood_daily_motions", { days: 7 })
    .then((arr) => setDaily7(Array.isArray(arr) ? arr : []))
    .catch((e) => { console.error(...); setDaily7([]); })
    .finally(() => setLoadingDaily(false));
}, [sparklineOpen]);

// 点窗外 / Esc 关闭
useEffect(() => {
  if (!sparklineOpen) return;
  const onDoc = () => setSparklineOpen(false);
  const onKey = (e) => { if (e.key === "Escape") setSparklineOpen(false); };
  window.addEventListener("mousedown", onDoc);
  window.addEventListener("keydown", onKey);
  return () => { ... };
}, [sparklineOpen]);
```

**3. mood pill 加 `onDoubleClick` + `onMouseDown` stopPropagation**

```tsx
onDoubleClick={(e) => {
  e.stopPropagation(); // 不让 Live2D 区双击 happy motion 误触
  setSparklineOpen((v) => !v);
}}
onMouseDown={(e) => e.stopPropagation()} // 防 onDoc 自关
```

两道 stopPropagation 解决两个上下文冲突：
- App.tsx 顶层 Live2D wrapper 的 `onDoubleClick` 触发 happy motion —— widget 自带 stopPropagation 让"双击 widget 看心情"和"双击空白处 happy motion"语义不混。
- window 上的 `mousedown` 全局监听 —— widget 内的 mousedown 不能让 popover 自关。

**4. 浮窗 render**

```tsx
{sparklineOpen && (
  <div
    onMouseDown={(e) => e.stopPropagation()}
    onClick={(e) => e.stopPropagation()}
    style={{
      position: "absolute",
      bottom: "calc(100% + 6px)", left: 0,
      minWidth: 200,
      padding: "8px 12px",
      background: "var(--pet-color-card)",
      border: "1px solid var(--pet-color-border)",
      borderRadius: 10,
      boxShadow: "var(--pet-shadow-md)",
      zIndex: 80,
    }}
  >
    <div>最近 7 天心情（最旧 ← → 最新）</div>
    {/* 加载中 / 空数据 / 数据三态 */}
    {daily7 && daily7.length > 0 && (() => {
      const maxTotal = Math.max(1, ...daily7.map(d => d.total));
      return <div style={{ display: "flex", gap: 8 }}>
        {daily7.map(d => {
          const top = topMotion(d.motions);
          const color = top ? MOOD_COLOR[top] : MOOD_COLOR_FALLBACK;
          const opacity = d.total === 0 ? 0.25 : 0.45 + (d.total / maxTotal) * 0.55;
          const size   = d.total === 0 ? 8    : 8 + Math.min(6, d.total);
          // dot + date label + hover tooltip "{date}: {breakdown}"
        })}
      </div>;
    })()}
  </div>
)}
```

视觉规则：
- 圆点 size 8–14px 按 total 缩放；total === 0 时固定 8px + 25% opacity（让"没记录的日子"可见但极淡）。
- 颜色取 top motion 的 MOOD_COLOR；不识别 motion → 灰 fallback。
- date 标签显 MM-DD（去掉年份省宽度）。
- hover tooltip：`YYYY-MM-DD：💗×3 ✨×1 💤×1`（按计数降序）。

popover 锚定 `bottom: calc(100% + 6px); left: 0` —— 浮在 widget 正上方，与 widget 左对齐。hover history chart 在 widget 下方，两者不会重叠。

## 不做

- **不接 14 / 30 天切换**。PanelPersona sparkline 已有该控件，桌面 widget 是 quick-glance 工具；切换天数会让 popover 变复杂。需要深看仍走 Panel。
- **不让 popover 持续 polling**。打开时 lazy 拉一次 + 缓存即可。如果用户关再开 30 秒后才再开，数据可能略滞后；但 mood 日聚合本身就是慢节奏数据（按日聚合），不必 5s 轮询。如果要"实时" 走 ref 加 invalidate 也行，本次保守。
- **不写自定义 sparkline SVG**。圆点列表 + tooltip 就够 "趋势感"；SVG 折线对 7 个数据点反而是矫枉过正。
- **不动 PanelPersona 大 sparkline**。它有自己的 7 / 14 天切换 + clear 入口；本 widget popover 是补充而非替代。
- **不写测试**。前端无 vitest；`topMotion` 是清晰的纯函数，行为简单（已用 keys.sort 保 deterministic）。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.11s
- 改动 ~120 行（顶层 helper 30 + state 35 + render 70 + 两道 stopPropagation）；既有 hover history chart / mood polling / format 都不动。

## 后续

- 把 `topMotion` / `MOOD_COLOR` 抽到 `utils/moodMotion.ts` 让 PanelPersona 共用（当下 PanelPersona 有自己的 MOTION_META，未来要做"配色统一改一处"得有共享 source）。
- popover 内每个 dot 点击 → 跳转 Panel/Persona 的 sparkline 并 highlight 那一天 — 当前 popover 仅显示不导航。
- 7 天太短时（新装机 / 重启过几天）的友好提示文案。
