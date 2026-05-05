# 任务面板加 "今天到期" 快捷过滤 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务面板加 "今天到期" 快捷过滤：任务多时扫一遍找当日 due 累，加 chip "今日到期 N" 一键只看今天 due 的活动任务。

## 目标

PanelTasks 现在支持 search / tag / showFinished / sortMode（queue or due）四
档过滤，但缺一个高频场景的 1-tap 入口：「今天必须搞掉的活动任务」。tag 行
下面加一个独立 chip "📅 今日到期 N"，点亮 → 只看 due 日期 == 本地今天的
**未结束** 任务（pending / error；done / cancelled 不算"今日到期"）。

## 非目标

- 不做"明天到期" / "本周到期" —— 档次太多反而没人挑；先让最高频场景成立。
- 不与 sortMode 绑定 —— 用户开 "今日到期" 时仍可选 queue 或 due 排序，互不
  干扰（典型用户开 due 排序，但偶尔想看 priority 顺序）。
- 不做 toast / animation —— 点 chip → 列表立刻收敛已经是足够的反馈。

## 设计

### State

`dueTodayOnly: boolean`，default false。与既有 search / selectedTags 同级。

### 过滤合成

现有过滤链：
```
tasks → showFinished → search → selectedTags → visibleTasks (排序段)
```
扩展为：
```
tasks → showFinished → dueTodayOnly → search → selectedTags → visibleTasks
```
`dueTodayOnly` 段：
- 任务 `t.due` 非 null 且解析后日期 == 本地今天（`new Date().toLocaleDateString()`
  无时区计算）
- **且** !isFinished(t.status)（done/cancelled 不视作"今日到期"，他们已结束）

### 计数

`dueTodayCount`：在 tasks 全集上算（与 allTags 同样派生自 tasks，不被链上其它
filter 影响），让用户即使在 selectedTags 模式里也能看到"今天总共有 N 条到
期"，决定要不要切到 dueTodayOnly。计数为 0 时 chip 整个不渲染（避免在没事
做的日子也占视觉位置）。

### UI

在 `tagFilterRow` 下面新增一行（独立 row），仅当 `dueTodayCount > 0` 时渲染：
```
[今日到期 3]
```
chip 复用 `tagFilterChip` 样式，但 accent 改成橙色（`#fed7aa` bg / `#9a3412`
fg）以区别于 tag 蓝紫，让"时间紧迫"的视觉权重独立。

### filtersActive

加 `dueTodayOnly` 一并作为 active 信号（让 `条匹配` 计数行出现）。

### 纯函数

`isDueToday(due: string | null, now: Date): boolean` 放在文件顶部 utils 区
（紧挨 `dueUrgency` / `dueColor` 旁），便于将来需要时复用。

## 测试

`PanelTasks` 是 IO 重的容器，无 vitest。`isDueToday` 是纯函数；项目无前端测
试框架，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `isDueToday` 纯函数 + state |
| **M2** | filteredTasks 链接入 + dueTodayCount 派生 |
| **M3** | chip 行 UI + filtersActive 接入 |
| **M4** | tsc + build + cleanup |

## 复用清单

- 既有 `dueUrgency` / `dueColor` 同款 due 字符串解析 (`Date.parse(":${due}:00")`)
- 既有 `isFinished(status)` 判定
- 既有 `tagFilterChip` 样式
- 既有 `filtersActive` / `条匹配` 反馈

## 进度日志

- 2026-05-06 13:00 — 创建本文档；准备 M1。
- 2026-05-06 13:10 — M1 完成。`isDueToday(due, now)` 纯函数（拿 `due.slice(0, 10)` 比 now 的 `YYYY-MM-DD`，避开 UTC 解析偏移）；`dueTodayOnly` state 加在 selectedTags 旁。
- 2026-05-06 13:15 — M2 完成。`filteredTasks` 链插入 dueToday 段（在 status 之后、search 之前）；`dueTodayCount` useMemo 派生自 tasks 全集，不被链上其它过滤影响；`filtersActive` 加入 dueTodayOnly。
- 2026-05-06 13:20 — M3 完成。tagFilterRow 上方插入独立的橙色 chip 行，仅在 `dueTodayCount > 0` 时渲染；点击 / Enter / Space 都可切换；选中时填橙底深棕字。
- 2026-05-06 13:25 — M4 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 923ms)。归档至 done。
