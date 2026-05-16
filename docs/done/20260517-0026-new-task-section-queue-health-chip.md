# PanelTasks "新建任务" section title 加队列健康 chip

## 背景

iter #192 在 "新建任务" 折叠 section title 旁加了 ⌘N hotkey hint chip。owner 折叠态看到 section title + ⌘N hint，准备建新单。

但 owner 此刻看不到当前队列健康 —— "我已经有 5 条逾期 / 3 条失败没处理，是不是先清 backlog 再加新的更合理？"。健康信号在下方 toneStrip / 行 chip 里，要滚下去才能看到。

把队列健康关键信号（🔴 逾期 + ❌ 失败）也浮到 section title 旁，让 owner "建新单前看一眼队列 backlog" 自然形成。

## 改动

### `src/components/panel/PanelTasks.tsx`

"新建任务" section title 行内、⌘N chip 之后加：

```tsx
{!createFormExpanded && (overdueCount > 0 || errorTaskCount > 0) && (
  <span
    style={{
      fontSize: 10,
      fontWeight: 600,
      marginLeft: 4,
      fontFamily: "monospace",
      background: overdueCount > 0 ? "tint-red-bg" : "tint-orange-bg",
      color: overdueCount > 0 ? "tint-red-fg" : "tint-orange-fg",
      borderRadius: 4,
      padding: "1px 6px",
      whiteSpace: "nowrap",
    }}
    onClick={(e) => e.stopPropagation()}  // 不抢 section toggle click
    title="队列里还有未处理任务：N 条逾期 · M 条失败。先看 backlog 再加新单..."
  >
    {overdueCount > 0 && `🔴 ${overdueCount}`}
    {overdueCount > 0 && errorTaskCount > 0 && " · "}
    {errorTaskCount > 0 && `❌ ${errorTaskCount}`}
  </span>
)}
```

显示规则：
- collapsed 时显（展开 section 下方 form 已占位，chip 多余）
- 两者都为 0 时不显（clean state 不打扰）
- 优先级配色：有逾期 = 红 tint；仅失败 = orange tint
- stopPropagation 防误触 section toggle

## 关键设计

- **复用既有 overdueCount / errorTaskCount useMemo**：iter 早期已计算（line 3707/3746）；本 iter 仅消费现成 state。
- **collapsed 时显**：与 ⌘N chip 同 gate —— "section title 是 collapsed 状态下唯一 UI affordance" 时显 hint / 健康；展开时 form 区域取代。
- **两者都为 0 不显**：clean state 不打扰；owner 自然知道队列健康（无干扰信号）。
- **优先级配色**：逾期红 > 失败 orange。owner 视觉直觉 = 红 > 橙 重要度。仅失败时也显红？不 — 失败有重试机制，没到 "red alert" 级别。
- **stopPropagation 防误触**：chip click 不该触发 section toggle；防回归。
- **tooltip 含教学**："先看 backlog 再加新单 / 看一眼队列健康再决定" 提示 owner 心智模型。

## 不做

- **不显 pending 计数**：pending 总数对"建新单"决策意义低（队列里东西多很正常）；只显 actionable 异常（逾期 / 失败）。
- **不写 click → 跳逾期 filter**：与既有 toneStrip 「🔴 N 逾期」chip click 重复；此处仅信息性显示。
- **不绑数字 click 路径**：chip 本身是 informational，单击不该有副作用。
- **不写测试**：纯 conditional render；视觉验证（造一些 overdue / error 任务 → section 折叠 → chip 显）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.20s
- 改动 ~30 行（chip 条件渲染 + 配色 + tooltip + 注释）。既有 overdueCount / errorTaskCount memo / ⌘N chip / section toggle / form 路径完全不动。

## TODO 状态

剩 1 条留池：
- pet 区右键加「📡 ping LLM 测延迟」

## 后续

- 再加 ⏸ pinned / 💤 snooze 计数 chip，让 section title 一行汇总队列全态。
- chip click 滚到 toneStrip / 跳逾期 filter（与既有 toneStrip 「🔴 N」chip click 同 deeplink）。
- 历史 trend chip："今日已完成 N 条 · 周内累计 M 条" 让 owner 看到 backlog 流通速度。
