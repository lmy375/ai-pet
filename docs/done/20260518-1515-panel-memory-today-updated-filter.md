# PanelMemory 段标题加「🏃 今日更新 (N)」filter chip（iter #480）

## Background

PanelMemory 已有：
- 顶部全局 sort：📅 按时间（updated_at）/ 🔀 按创建（created_at）/
  📏 按字数 / ⏰ next-fire — 排序所有 items
- 顶部全局 filter：🌱 仅今日新增（created_at = today）— 全 cat AND
- 每 cat 段标题已含：🔇 silent / 💤 snooze / 📊 schedule 等

但缺一个 **per-cat 今日 updated 过滤** 入口。owner 在大段 cat（如
butler_tasks / ai_insights 含 50+ 条）想 audit「这 cat 今天 pet / 我
动过哪些 item」时只能开 📅 sort + 心算"前几条是今日"。

本 iter 加「🏃 今日更新 (N)」 chip — per-cat 硬过滤 updated_at = today。
与全局 sort 入口对偶（sort 排序，本 chip 过滤）。

## Changes

### `src/components/panel/PanelMemory.tsx`

#### 1. `todayUpdatedCats: Set<string>` state

```ts
const [todayUpdatedCats, setTodayUpdatedCats] = useState<Set<string>>(
  new Set(),
);
```

session 内 state（与既有 silentOnlyCats / butlerScheduleFilter 同
lifecycle，不持久化 — filter 是临时阅读偏好）。

#### 2. 过滤管道分支

紧贴 `silentOnlyCats` filter 之后插：

```ts
if (todayUpdatedCats.has(catKey)) {
  const todayLocal = new Date().toLocaleDateString("sv-SE");
  pool = pool.filter(
    (it) => it.updated_at && it.updated_at.startsWith(todayLocal),
  );
}
```

- `toLocaleDateString("sv-SE")` → `YYYY-MM-DD` 本地 — 与既有
  `todayOnlyFilter`（created_at = today）同算法保 cross-DST 边界稳定
- `startsWith` ISO 字典序 = 时间序匹配；`updated_at` 是
  `YYYY-MM-DDTHH:MM:SS.fff` 形式 prefix 检查精准
- AND 关系叠加（既 schedule kind filter / silent filter 后再 AND today
  updated）— 让 owner 「only silent + today updated」也成立

#### 3. Section header chip

紧贴 📤 .md chip 之后插：

```tsx
{cat.items.length > 0 && (() => {
  const todayLocal = new Date().toLocaleDateString("sv-SE");
  const todayUpdatedN = cat.items.filter(
    (it) => it.updated_at && it.updated_at.startsWith(todayLocal),
  ).length;
  if (todayUpdatedN === 0) return null;
  const active = todayUpdatedCats.has(catKey);
  return (
    <button onClick={() => setTodayUpdatedCats(prev => toggle(catKey))}
      style={{ ...active ? bluetint : default }}
      title={active ? "..." : "..."}
    >
      {active ? "✓ " : ""}🏃 今日更新 ({todayUpdatedN})
    </button>
  );
})()}
```

- **`todayUpdatedN > 0` gate**：count = 0 时 chip 隐藏避免空 chip 噪
  音（与既有 silentN > 0 / pinnedN > 0 等同 gate 模板）
- **active tint**：开启 filter 时蓝 tint（与既有 silentOnlyCats /
  butler schedule chip 同 active visual）
- **count 显具体数**：let owner 一眼看「今天动过 N 条」无需 click，
  click 后只是切到聚焦视图

## Key design decisions

- **per-cat 而非全局 filter**：跨 cat 「今日 updated」无意义 — 不同
  cat 语义不同（butler_tasks today updated = 派单 / done；ai_insights
  today updated = pet 自反思；general today updated = 杂项 brain-dump）。
  per-cat 让 owner 在「我今天在 butler 段动了啥 / pet 在 ai_insights
  写了啥」两不同 lens 间独立切换
- **`updated_at` 而非 `created_at`**：与既有 🌱 仅今日新增（顶部全
  局，filter created_at）正交。本 chip 看「今天动过」— 含今日新建
  + 今日修改既有。owner audit 「今天我用 PanelMemory 实际触摸了
  哪些」入口
- **`scheduleFilteredItems` pipeline 内做 filter**：与 silent / schedule
  filter 同位置 — 让 sort（pin / updated / created / charcount /
  next-fire）后于 filter 自然组合。`pinnedKeys` 在更下游也仍生效
- **不持久化 toggle state**：filter 是临时阅读偏好（"我现在想看今天
  的"），下次开 PanelMemory 自然回到全显。与既有 silentOnlyCats /
  butlerScheduleFilter 同 ephemeral lifecycle
- **不引「shift+click 多 cat 同时启用」**：each cat 独立 toggle 足够；
  owner 想跨 cat 看 today updated 走全局排序 + 心算前几条
- **count 计算用 `cat.items` 而非 `scheduleFilteredItems`**：count 显
  「不考虑其它 filter 时本 cat 有几条 today updated」 — 让 owner 决
  策"要不要开此 chip"看到无干扰真值。点击后 filter 再与其它 filter
  AND 让显示数可能 < count（owner 心智清晰：count 是上限）
- **不写 unit test**：纯 toggle + filter 字符串 startsWith；逻辑
  trivial（既有 silentOnlyCats / todayOnlyFilter 同 pattern production
  验证）。GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.30s)
- 后端无改动 — 纯前端 filter 增强
- 手测：PanelMemory 任一含今日 updated item 的 cat → section header
  看「🏃 今日更新 (N)」chip → click → 仅显该 N 条 + chip 蓝 tint + ✓
  prefix → 再 click → 恢复全显；cat 内没 today updated 时 chip 不渲染
