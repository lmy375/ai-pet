# PanelTasks filter 命中 0 条空状态加 "清除全部过滤" 按钮

## 背景

原 TODO 池里 "PanelTasks header 加 '今日 due' quick filter chip" 是
我上一轮自主提需求时的重复 entry —— 该功能在 R94 / R104 期间已实现
（`DueChip kind="today"` 在 dueTodayCount > 0 时浮，line 2710）。本
轮在拣最小项时发现，移除 TODO 重复 entry 并用一个相邻的小改进替代。

## 需求（替代项）

filter 命中 0 条时 panel 中段显文案"没有匹配筛选条件的任务"，但清
除按钮 "✕ 全部" 在 search 行（panel 顶部），用户长队列下滚 + filter
没命中 → 看到 empty 文案 → 还要往上滚才能找到清除按钮，体验断了。
在空状态下方加一个就地 "✕ 清除全部过滤" 按钮，落点直接。

## 实现

`src/components/panel/PanelTasks.tsx`，空状态分支：

- 在 `filtersActive=true` 路径下、原 "📋 用范例预填一条" 按钮（仅
  `!filtersActive && showFinished` 显）之前加一个新按钮 div
- 仅在 `filtersActive=true` 时浮，避免与"真空 queue 预填范例"按钮
  互相挤位置
- 按钮文案 "✕ 清除全部过滤"
- onClick 与 search 行的 "✕ 全部" 共行为：`setSearch("")` +
  `setSelectedTags(new Set())` + `setDueFilter("all")` +
  `setPriorityFilter(new Set())` + `setOriginFilter(new Set())`
- 视觉中性灰底（与 search 行一致 muted ghost button 风格，不像主
  CTA 抢眼）
- title tooltip 列五维 filter 名称帮用户理解"全部"是哪几个

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - filter 命中 0 条 → 空状态浮"清除全部过滤"按钮
  - 点击 → 五种过滤瞬间清零 → empty 消失 → tasks 全显
  - 无 filter 但 queue 真空 → 显 "📋 用范例预填一条"，不显清除按钮
  - 切到"仅进行中"全 done → 不显清除（filtersActive=false 不会触发）

## 不在本轮范围

- 没把 search 行的 "✕ 全部" 删掉换 single source of truth：search 行
  按钮在 filter 命中 > 0 时仍有意义（看到一堆结果想清掉），两处不重
  叠互补
- 没做"清除某一维"按钮（只清 search 不清 tag 之类）：极少场景；
  filter chip 本身已经可单独 toggle 关掉

## TODO 池剩余

- PanelChat ⌘K task 引用选择器
