# PanelMemory ai_insights banner 加 daily_review 计数

## 背景

iter #214 给 ai_insights cat 加了 "🧠 由宠物自己写" banner。banner 提到 `daily_review_<date>` 是 protected 条目之一，但没显具体数量 —— owner 想看"我家宠物已积累多少天复盘" 必须滚 item list 自己数。

加一行简单计数 inline 显在 banner 内。

## 改动

### `src/components/panel/PanelMemory.tsx`

ai_insights banner 内 append daily_review 计数：

```tsx
{(() => {
  const count = cat.items.filter((it) =>
    it.title.startsWith("daily_review_"),
  ).length;
  if (count === 0) return null;
  return (
    <>
      {" "}· <span title="本 cat 含 N 条 daily_review_<date>...">
        📦 {count} 条 daily_review 历史
      </span>
    </>
  );
})()}
```

- 0 时不显（noise reduce）
- > 0 时 append " · 📦 N 条 daily_review 历史" inline 到 banner 末尾
- tooltip 解释 retention 由 consolidate 配置控制

## 关键设计

- **inline 而非新行**：banner 短文案已够；新 line 撑高 banner 占位。inline " · " 分隔与 muted 文案融合。
- **`title.startsWith("daily_review_")` 严格 prefix 匹配**：与 consolidate 写入约定一致（`daily_review_YYYY-MM-DD`），不抓 typo / 用户手动建的同名 item。
- **0 不显**：新用户 / 刚启用宠物时无 daily_review；显 "0 条" 是 onboarding 噪音。
- **不 link 到具体清单**：本 iter 仅 informational。owner 想看具体 daily_review 内容可直接滚 list（既有按日期降序）。

## 不做

- **不写"📦 click 弹 daily_review 时间线" overlay**：scope creep；既有 list view 滚动已能看到。
- **不显 retention setting 数值**：consolidate retention 是 ai_insights 自身管理；陈列到 ai_insights banner 维度有点错位。Settings 里看更合适。
- **不写测试**：纯字符串 startsWith filter + render；视觉验证（有 daily_review_<date> 条目时 banner 末尾应显 "📦 N 条" inline）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.31s
- 改动 ~25 行（IIFE filter + count + render + 注释）。既有 banner 文案 / 4 protected items 列表 / 删除 hint 完全不动。

## TODO 状态

剩 3 条留池：
- butler_task 行 [reminderMin: N] chip click 弹快速编辑
- TG /markers 命令一次列 pinned + silenced
- pet 区 hover 显本机时区 chip 浮卡

## 后续

- daily_review 历史长（> 30 条）时 banner 加 "（最早 YYYY-MM-DD）" inline 显时间跨度。
- 加 daily_plan 计数 / 其它 protected items 计数 inline（让 banner 信息密度提升不变长）。
- "📦 click 展开 daily_review 时间线 mini popup" —— 按日期排，每日 1 行 + tooltip 显复盘第一句。
