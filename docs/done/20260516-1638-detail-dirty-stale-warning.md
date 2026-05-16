# detail.md "● 未保存" badge 持续 dirty > 60s 红色 pulse 警示

## 背景

TODO 上 auto-proposed 一条："detail.md 顶部 ● 未保存 badge 持续 dirty > 60s 时变红 + 微 pulse：让 owner 在长编辑场景下不忘 ⌘S。"

既有 "● 未保存" badge 是静态 muted gray + 文字提示。但 owner 写长 detail.md 一气呵成 30 分钟、起身倒水、回来切走 panel 不知不觉的情况下，badge 没有持续抓眼的能力 —— 关 window / 切任务 / Esc 误触都可能丢未保存内容。

加"持续 dirty > 60s → 染红 + opacity pulse + 显 elapsed 秒数"让 badge 在长编辑后能"主动叫醒" owner 该 ⌘S。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### 状态追踪

```ts
const dirtySinceRef = useRef<number | null>(null);
const [dirtyTickKey, setDirtyTickKey] = useState(0);
useEffect(() => {
  const dirty = editingDetailContent !== editingDetailOriginalRef.current;
  if (dirty) {
    if (dirtySinceRef.current === null) dirtySinceRef.current = Date.now();
  } else {
    dirtySinceRef.current = null;
  }
}, [editingDetailContent]);
useEffect(() => {
  if (editingDetailTitle === null) {
    dirtySinceRef.current = null;
    return;
  }
  const id = window.setInterval(() => setDirtyTickKey((k) => k + 1), 5000);
  return () => window.clearInterval(id);
}, [editingDetailTitle]);
```

- `dirtySinceRef`: 记录 dirty 起始时间戳，clean 时清。
- `dirtyTickKey`: 5s 周期 +1，仅触发 re-render；不存 elapsed 进 state（避免每 5s 多 1 次 React 状态更新 + 闭包变化）。badge 渲染时 read ref 算最新 elapsed。

#### badge 渲染条件分支

```tsx
{dirty && (() => {
  const since = dirtySinceRef.current;
  const elapsedSec = since ? Math.floor((Date.now() - since) / 1000) : 0;
  const stale = elapsedSec > 60;
  void dirtyTickKey;  // 让 ESLint 关联，引 hook 见证
  return (
    <span
      style={{
        color: stale ? "var(--pet-tint-red-fg)" : "var(--pet-color-muted)",
        fontWeight: stale ? 600 : 400,
        animation: stale ? "pet-detail-dirty-pulse 1.8s ease-in-out infinite" : undefined,
        ...
      }}
      title={stale ? `... 未保存超 ${elapsedSec}s ⚠️ —— 按 ⌘S 保存 ...` : "..."}
    >
      ● 未保存{stale ? ` ${elapsedSec}s` : ""}
    </span>
  );
})()}
```

stale 态：red tint + bold + opacity pulse + 显 elapsed seconds + tooltip 切到警示文案。

#### `@keyframes` 注入

既有 PanelTasks 顶部 `<style>` 块加：

```css
@keyframes pet-detail-dirty-pulse {
  0%, 100% { opacity: 1; }
  50%      { opacity: 0.55; }
}
@media (prefers-reduced-motion: reduce) {
  [style*="pet-detail-dirty-pulse"] { animation: none !important; }
}
```

opacity 1 → 0.55 → 1，柔和不抢主视觉但让 owner 余光能瞄到。`prefers-reduced-motion` 退化为常亮，与既有 `pet-task-now-pulse` 同处理。

## 关键设计

- **60s 阈值**：60s 是"短按一下保存忘了" vs "真的离开 editor" 的分界。短于 60s 是 normal 写作节奏（不该打扰），长于 60s 进 "stale" 警示。可调 —— 当前硬编码。
- **ref + tickKey 模式**：state 只存 `dirtyTickKey` 触发 re-render；`dirtySinceRef` 存真实时间戳 read-only 给渲染读最新值。比 `[elapsedSec, setElapsedSec]` 的 5s setState 每次 schedule update + diff 更轻。
- **`void dirtyTickKey`**：显式引用让 ESLint exhaustive-deps + 阅读者都看到"这个值是 trigger 重渲的"，否则 dirtyTickKey 被读但 lint 可能误标 `'dirtyTickKey' is unused`。
- **dirty→clean 重置**：用户 ⌘S 保存或 Esc 取消时 content 回到 original，dirty 检测自动 false → ref 清 null → 下一次 dirty 重新计时。
- **关编辑器清 ref**：editingDetailTitle 变 null 时（编辑器关闭）显式清 ref，防 stale 时间戳跨任务残留。
- **`[style*="pet-detail-dirty-pulse"]`**：与既有 pet-task-now-pulse 同 `prefers-reduced-motion` 防御策略 —— 选择器匹配 inline animation 字符串。
- **5s 间隔 tick**：60s 阈值精度 ±5s 已足够（owner 不会感知"59s 没红"vs"61s 红"），节省 cpu。
- **不动既有 dirty 检测路径**：badge 仍然只在 content !== original 时渲染；本 iter 仅给 badge 加 stale 视觉态。

## 不做

- **不写测试**：纯 UI tick + setState；既有 ✦ 等 chip 同模式无单测。视觉验证（写 detail.md 不存 → 等 1 分钟 → 看到红色 pulse + "62s" 显示）足够。
- **不写浏览器 beforeunload 拦截**：detail.md 编辑器关闭走 Esc / 切任务 / 关 panel 几条路径；既有 Esc dirty 二次确认已防误触，再加 beforeunload 是 over-engineering。
- **不持久化 dirtyAt timestamp 到 localStorage**：完全关 panel 后重启不需要"延续 stale 计时"；下次打开应该刷新。
- **不调 60s 阈值到 settings**：当前硬编码已够多数场景。等用户反馈"我打字慢 60s 太短"再加 settings 调档。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.20s
- 改动 ~60 行（state + 2 effect 18 + badge IIFE 重构 25 + @keyframes 注入 8 + 注释）；既有 dirty 检测 / ⌘S 保存 / Esc 取消 / 行号 chip / 字数 counter / chip 集群其它路径完全不动。

## TODO 状态

6 条 auto-proposed 已完成 1 条（含 stale 移除 1 条：PanelChat sort chip — 早已在 PanelTasks 实装），余 5 条留池：
- PanelTasks detail 编辑器加「↑ 上 / ↓ 下一条」导航箭头
- detail.md 自动每 60s 存草稿到 localStorage
- 桌面 pet hover 3s 浮 ambient 三段统计微卡片
- PanelMemory 类目 7 天 churn sparkline
- detail.md preview `[task: 标题]` 语法识别为 ref chip

## 后续

- ⌘S 后短暂变绿 "✓ 已保存" 1.5s 复用 pulse 动画 —— 视觉对称"未保存红 / 已保存绿"。
- 系统通知 `Notification` API：dirty 持续 5min+ 弹 macOS 通知。但 Tauri 调原生通知需权限 + 用户配置，复杂度大。
- 阈值用户可调（settings 暴露）—— 当前 60s 是合理默认，等真有 "我打字慢 / 我打字快"反馈再加配置。
