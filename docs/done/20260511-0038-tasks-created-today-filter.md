# PanelTasks "今天创建"过滤切片

## 需求

任务面板已有"今日到期 / 逾期"两个 chip 切片，但缺一个按 `created_at` 切今天的视图。用户晚上想复盘"今天接了什么单 / 派了什么单"时，只能搜索 + 看时间戳手工过滤，繁琐。补一个 chip 一键切片。

## 实现

### 状态扩到四态 enum

`dueFilter: "all" | "today" | "overdue"` → `"all" | "today" | "overdue" | "createdToday"`。互斥保证不会"今日到期 + 今天创建"两 boolean 死锁。

注释里说明 today / overdue 走 `t.due`，createdToday 走 `t.created_at` —— 不分 status，让用户也看到今天处理过的已完成单。

### 过滤逻辑

`createdToday` 分支：

```ts
if (t.created_at.length < 10) return false;
const todayPrefix = `${y}-${m}-${d}`;
return t.created_at.slice(0, 10) === todayPrefix;
```

后端 `chrono::Local::now().naive_local()` 输出 `YYYY-MM-DDTHH:MM:SS.fff`，前 10 字符就是本地日期 —— 复用 `isDueToday` 的同款 string-prefix 比对，避开 Date 解析跨时区陷阱。

### 计数

`useMemo` 算 `dueTodayCount / overdueCount / createdTodayCount`，与父级 search / tag / sort 解耦，让用户在任何 filter 下都能看到全集计数。

### Chip 组件扩展

`DueChip` 加 `kind: "createdToday"` 分支，蓝色 palette（与红 / 橙形成清晰区分）：

- bg `#eff6ff` → bgActive `#bfdbfe`
- fg `#1e40af`
- border `#bfdbfe` → borderActive `#3b82f6`

label `🆕 今日创建`，tooltip "只看今天本地日期内创建的任务（不分状态）"。

### 渲染

chip row 的 visibility 条件加 `createdTodayCount > 0`；新 chip 放在 today chip 之后，保持"逾期 → 到期 → 创建"时态由强到弱的视觉序。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 今天创建若干任务 → 蓝色 `🆕 今日创建 (N)` chip 出现
  - 点击 → 列表过滤到这 N 条（含已 done / cancelled 的"今天处理过"）
  - 再点击或切到其它 chip → 退出过滤
  - 跨过零点后下一秒 → chip 消失或切到新今天的计数（30s nowMs 滴答触发）

## 不在本轮范围

- 不加"昨天创建"chip —— 时态切片越多视觉越乱；用户要复盘多天用"任意 N 天前"过滤更合适，但那个交互（datepicker / 输入框）超出 chip 模式
- 没在 chip text 里复用 var(--pet-tint-blue-bg) / fg —— 这一行本身的红 / 橙都是 hardcoded hex，跟主题 tint 系统的 light/dark 自动切换不一致；统一主题化是另一项 cleanup，本轮不扩
