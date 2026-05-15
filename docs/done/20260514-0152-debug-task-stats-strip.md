# PanelDebug 加 "📊 任务状态" 横条

## 背景

`task_stats` 后端命令已经有三个 consumer：桌面 `/stats` slash、pet 窗 pill、TG `/stats`（独立路径）。PanelDebug 是 owner 排查 / 体检的主入口，但**目前不显任务状态** —— 想看就得切到「任务」tab。

加一条 chip 横条，与已有的"🛠 专用工具占比"strip 同款样式 + 同 30s 轮询节奏。

## 改动

`src/components/panel/PanelDebug.tsx`：

### 数据

`envInfo` 已经在 mount 时调过 `get_db_stats`。task_stats 是另一条独立命令：

```ts
type TaskStats = {
  pending: number;
  overdue: number;
  done_today: number;
  error: number;
  cancelled_today: number;
};
const [taskStats, setTaskStats] = useState<TaskStats | null>(null);
useEffect(() => {
  let cancelled = false;
  const fetchTaskStats = async () => {
    try {
      const s = await invoke<TaskStats>("task_stats");
      if (!cancelled) setTaskStats(s);
    } catch {
      // 旧 backend 缺命令 → 静默退化，strip 不渲染
    }
  };
  void fetchTaskStats();
  const id = window.setInterval(fetchTaskStats, 30_000);
  return () => { cancelled = true; window.clearInterval(id); };
}, []);
```

### 渲染：与既有 🛠 strip 同款

紧挨在"🛠 专用工具占比"strip 下方插一个新 strip：

```
📊 任务状态：  待办 12 · 🔴 逾期 1 · ✓ 今日完成 3 · ⚠️ 出错 0 · 🗑 今日取消 1
```

样式克隆既有 strip：6px/16px padding / 11px font / SF Mono / muted text。每个段独立 span，逾期 > 0 时该段染红（`var(--pet-tint-red-fg)`），其它段保持 muted 让逾期更突出。

`taskStats === null`（未 fetch 或旧 backend）→ 整条不渲染，向后兼容。

## 不做

- 不挂点击 deeplink：PanelDebug 不是导航入口；用户在 PanelDebug 看见数字后，自己用 ⌘3 跳「任务」tab 即可
- 不显完整 backlog（含 due/title）：那是「任务」tab 的活；PanelDebug 只看汇总
- 不与 🛠 strip 合并行：两条信息密度都已经较高，分两行可读性更好

## 验收

- `npx tsc --noEmit` ✅
- 切「调试」tab → 顶部能看到新的"📊 任务状态"strip
- 制造一条 overdue → 30s 内 strip 的"🔴 逾期"段染红
- 旧 backend（理论上不会有 —— 本机一直最新）→ strip 不出现

## 完成

- [x] PanelDebug.tsx: TaskStats state + 30s 轮询
- [x] 渲染 strip
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
