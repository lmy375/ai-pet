# PanelDebug 工具调用历史按 name 分组

## 需求

PanelDebug 工具调用历史是平铺时间线（最新在前）。用户想看"哪个 tool 调
最多"时得肉眼扫几十条数 name。加 toggle 切到"按 tool name 分组"视图：
header 显 name + 调用次数 + 最高风险 chip + 最近时间；展开看 group 内调
用列表。

## 实现

`src/components/panel/PanelDebug.tsx`：

- 新 state `toolHistoryGroupByName: boolean`（默认 false 保留原 timeline）
- 新 state `toolGroupExpanded: Set<string>` 跟踪哪些 group header 展开
- filter chip 行加一个 toggle 按钮 "📊 按工具分组 / 📜 时间线"，accent
  active 视觉同既有 priority chips
- `filtered.map((c, i) => ...)` 改条件分流：
  - `toolHistoryGroupByName === false` → 原 flat 渲染（per-call full
    card，含 args/result details）
  - `true` → IIFE 派生 `Map<name, ToolCallRecord[]>`：
    - 按 `calls.length` 降序，相等按 name 字典序破平
    - 每组 header 一行：▸/▾ + monospace name + 最高风险 chip
      (`high → medium → low` 优先级) + `× N` 计数 + 右侧最近调用 `HH:MM`
    - header 点击切换 toolGroupExpanded set
    - 展开后 group 内每 call 用更紧凑一行（risk chip + review status +
      timestamp + 截 60 字符的 purpose），不重复 args/result 详细 card
      避免 group 视图太长

## 设计选择

- 不重复 full per-call card 在分组视图：用户切到分组视图本来就是"扫频
  次"诉求，不需要每条都看 args / result。要看详情切回时间线
- 风险 chip 取一组中最高的 risk_level：让 header 一眼看出"这工具最坏调
  过啥风险"
- 计数降序：用户问"哪个 tool 调最多"时排前的就是答案
- session 内切换：不持久化（用户多数情况是临时看一下分组分布，下次开
  panel 期望 timeline 默认）

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 展开 PanelDebug 的 🔧 工具调用历史 → 默认 timeline 视图（与之前一致）
  - 点 "📊 按工具分组" → 列表切到分组：bash × 12 / read_file × 8 / ...
    降序排
  - 点 group header → 展开该 group 的紧凑调用 row（risk / status /
    timestamp / purpose 截 60 字符）
  - filter 切到"低险" → 分组也跟着 filter（rebuilds group from filtered list）
  - 切回 timeline → 回到原 flat 视图

## 不在本轮范围

- 没存 group 折叠状态到 localStorage：临时调试用，不必跨重启
- 没让分组 header 显平均执行时间 / 失败率：tool_call_history 不带 timing
  字段；后续要做需扩 schema
- 没在分组视图复用 args/result 复制按钮：保留 timeline 视图作详细操作入口

## TODO 池剩余

- ChatMini 桌面气泡 markdown 块级语法
- PanelTasks 归档独立 tab
- PanelMemory hover 显 detail.md preview
