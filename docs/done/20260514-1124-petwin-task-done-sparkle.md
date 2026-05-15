# 桌面宠物窗任务完成 sparkle 庆祝

## 背景

TODO：

> 桌面气泡任务完成 sparkle：宠物 done 一条 butler_task 时桌面短动画庆祝（150ms 闪光 + ✅ chip 浮 1.5s），与 GOAL"可爱"贴齐。

宠物状态 pill 已经数字化告诉用户 `✓ M 今日完成`，但纯数字"+1"很难注意 —— 用户切回桌面那一刻不知道刚才有完事发生。`GOAL.md` 第六条「UI 要美观可爱」要求一些「会"逗你笑"的细节」。在 done 计数单调上升的瞬间触发一次轻量粒子动画，正好把"刚完成一件事"这个事件信号顶到视觉前台。

## 改动

### `src/App.tsx`

**1. 模块顶定义粒子表 `SPARKLE_PARTICLES`**

手工排出 6 颗粒子的 top/left/dx/rot/delay/size/glyph，弧线感而非纯随机散点。glyph 取 ✨ × 3 + ⭐ × 2 + 🌟 × 1 让视觉不机械重复。delay 0/80/160/240/320/400 ms 形成涟漪式涌现。

```ts
const SPARKLE_PARTICLES: Array<{
  top: string; left: string; dx: string; rot: string;
  delay: number; size: number; glyph: string;
}> = [
  { top: "62%", left: "32%", dx: "-14px", rot: "-12deg", delay: 0,   size: 22, glyph: "✨" },
  { top: "55%", left: "68%", dx: "14px",  rot: "12deg",  delay: 80,  size: 22, glyph: "✨" },
  { top: "44%", left: "48%", dx: "-2px",  rot: "0deg",   delay: 160, size: 26, glyph: "🌟" },
  { top: "58%", left: "18%", dx: "-22px", rot: "-18deg", delay: 240, size: 18, glyph: "⭐" },
  { top: "50%", left: "82%", dx: "22px",  rot: "18deg",  delay: 320, size: 18, glyph: "⭐" },
  { top: "36%", left: "62%", dx: "8px",   rot: "6deg",   delay: 400, size: 20, glyph: "✨" },
];
```

**2. done 计数单调上升检测**

紧贴 `taskStats` 轮询（已有的 60s `task_stats` IPC）：

```ts
const lastDoneTodayRef = useRef<number | null>(null);
const [sparkleKey, setSparkleKey] = useState(0);
useEffect(() => {
  const cur = taskStats.done_today;
  const prev = lastDoneTodayRef.current;
  lastDoneTodayRef.current = cur;
  if (prev === null) return;        // 首次观测仅 baseline
  if (cur > prev) {
    setSparkleKey((k) => k + 1);    // 单调 +N 才点燃
  }
}, [taskStats.done_today]);
```

边界设计：
- **首次观测仅 baseline**：开窗时 done_today 可能已是 N（用户白天已完成几条）。如果 baseline 也点燃，每次开窗都会误触一次"庆祝"。
- **午夜回 0 不触发**：`cur > prev` 自然兜住（0 < N 反向不点燃）。
- **多次连发**：`sparkleKey++` 每次自增让 React 通过 `key` 重新 mount 整段动画 → 第二次完成在第一次还没飘完时也能从头跑。

**3. 粒子层 JSX 插入到 Live2D 区**

放在 Live2D wrapper 内、`<MoodWidget />` 上方：

- `position: absolute; inset: 0; pointer-events: none; z-index: 70; overflow: hidden`
- 内置 `<style>` 块定义 `@keyframes pet-sparkle` —— 4 帧（0/25/70/100）控制 opacity + translate + scale + rotate。每帧 transform 都重新声明 `rotate(var(--rot))` 否则 transform 中间帧会丢 rotate（不同 transform 函数序列不可插值）。
- 每个 `<span>` 通过内联 `style={{ ['--dx' as never]: p.dx, ['--rot' as never]: p.rot }}` 注入 custom property，让单条 keyframe 复用 6 次方向各异。`as never` 绕过 React.CSSProperties 字符串 key 校验（同等价 `as any`，但意图更窄）。
- `@media (prefers-reduced-motion: reduce)` 把粒子 opacity 强制 0 + animation 关掉，无障碍兜底。

## 不做

- **不监听单条任务 done 事件**。后端没有"任务完成"广播 event；新增就要走 Tauri emit/listen + 后端 hook 进 task_queue 状态转换路径，工作量与本次"加点 sparkle"产品价值不匹配。`done_today` 计数轮询是已有 SoT，0 改造成本就够用。
- **不显"已完成 X" toast 文字**。pill 已经数字化展示了；toast 文字会让动画变"教学/抢答"感而非"惊喜小庆祝"。纯粒子更克制。
- **不响应任务"取消"/"重试"等其它状态变化**。Sparkle 是"庆祝"语义，错配会稀释信号。
- **不动 Telegram / Panel 端**。桌面常驻 pet 窗才是用户视觉中心；Panel 是查询/操作面，不需要庆祝动画。
- **不暴露 settings 开关**。reduced-motion 媒体查询是无障碍兜底；个人偏好不喜欢可走系统设置一处管全应用。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.16s
- 改动是纯叠加 —— 既有 taskStats 轮询路径不动，pill 渲染不变，Live2D / MoodWidget / 收起按钮所有兄弟元素位置不变（粒子层 z-index 70 仅压在它们之上）。

## 后续

- 任务完成事件 emit（后端 → 前端），让 sparkle 0 延迟点燃而不必等 60s 轮询窗口。
- 任务 cancel / error 也加一个对偶的微动画？暂不做 —— 怕信号过载。
