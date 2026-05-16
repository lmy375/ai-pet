# PanelTasks 顶 chip 行加 "✓ 今日完成 N" green chip

## 背景

PanelTasks 顶 chip 行已显 🔴 逾期 / 📅 今日到期 / 📌 钉 / etc. 计数，但缺 momentum 信号 —— owner 想看"今天我已经完成多少" 必须滚到列表中段看 "✅ 今日完成 N · 近 7 天 M" 那段。

把 "今日完成" 也提到 chip 行让 backlog + momentum 一行汇总。

## 改动

`src/components/panel/PanelTasks.tsx`：

1. tone strip 内（紧贴 🔴 逾期 chip 之后）加 green chip：

```tsx
{completionStats.today > 0 && (
  <span
    style={{
      fontSize: 11,
      padding: "2px 8px",
      borderRadius: 8,
      background: "var(--pet-tint-green-bg)",
      color: "var(--pet-tint-green-fg)",
      fontWeight: 600,
      whiteSpace: "nowrap",
    }}
    title={`今日完成 N 条任务${week > today ? "（近 7 天累计 M 条）" : ""}`}
  >
    ✓ 今日完成 {completionStats.today}
  </span>
)}
```

2. tone strip 外层 conditional 加 `|| completionStats.today > 0` 让 0 backlog 时仍能因 "今天完成了" 而显 chip 行。

## 关键设计

- **复用 completionStats.today useMemo**：iter 早期算法（line 3831）已经在算；本 iter 只是消费现成 state。
- **green tint chip**：与 dueChip 红色 / pinned amber 等区分；绿 = momentum / 正向反馈。
- **0 不显**：与其它计数 chip 同稀疏模板 —— 0 不打扰 clean state。
- **informational 不绑 filter**：完成任务不该 "click 切到 done view" —— owner 看 list 默认折叠 finished；想看具体完成 list 已有"✅ 今日完成 N · 近 7 天 M" small card。
- **tooltip 附 7 天累计**：让 owner hover 看周累计趋势 —— 今日 3 / 周 21 = 平均每天 3 条 ✓；今日 5 / 周 5 = 今天大爆发。

## 不做

- **不写测试**：纯 conditional render；视觉验证（造一条 done 今日 task → chip 应显）足够。
- **不绑 click → drill-down today done items**：scope creep；"✅ 今日完成" small card 已经能展开 list (completedListExpanded)。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.16s
- 改动 ~25 行（chip + 外层 gate +1 条件 + 注释）。既有 dueChip / pinChip / priorityChips / completionStats memo / small card 路径完全不动。

## TODO 状态

剩 4 条留池：
- PanelMemory items 长 description 行级折叠
- ChatMini bubble 双击 ref + audio bell
- detail.md toolbar 加 "🧠 ask LLM about selection"
- detail.md 编辑器 ⌘K 唤起 task quick-find palette

## 后续

- chip 加 "↑ 今日已超平均" 标识当 today > week / 7 时让 owner 看到 high day 信号。
- 同款 momentum chip 给 PanelMemory（"🌱 今日新增 N" 已经有；可加 "🔄 今日更新 M"）。
