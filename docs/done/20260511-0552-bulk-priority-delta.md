# 任务列表批量 ±1 priority

## 需求

bulk toolbar 已经有了"绝对 set priority"路径（输 N → 全部设为 N），但日常场景
更多是"这一批活全部往上提一档"或"这堆都次要点排到下面" —— 不想思考"每条
应该是几"，只想相对升 / 降。补一对 ±1 按钮。

## 设计

各任务自己的当前 priority 各 +/- delta 后 clamp 到合法区间。已在边界（0 或
PRIORITY_MAX）的条不再发请求，runBulk 仍把它当 success（"不需改"语义），避
免 skipped 弹窗。

## 实现

`src/components/panel/PanelTasks.tsx`：

- `handleBulkAdjustPriority(delta: number)`：
  - 用 Map 捕获当前 tasks 快照的 `title → priority`
  - runBulk 遍历，每条从快照取原 priority + delta + clamp
  - 边界 noop 早返，省一次 invoke
  - label `priority +1` / `priority -1`，反馈走既有 bulkResultMsg
- priority 子面板加两个按钮 `↑ -1` / `↓ +1`，与"确认"绝对 set 同行
- "或相对：" muted 文案 + tooltip 提示"priority 数字越小越重要"，对齐
  PanelTasks 内 priority 的语义

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 多选 → 改 priority → "↑ -1" → 每条各自 -1 → reload 看到顺序与高亮变化
  - 已经 P0 的条 → 不发请求，不显失败
  - 混合 P0 + P3 + P7 → 全部 -1 后 → P0 (noop), P2, P6
  - 与绝对 set 仍互补：用户输 5 + 确认仍按老路径走

## 不在本轮范围

- 没做 ±2 / ±3 快捷键 —— 一次 ±1 重复点 2 下足够覆盖；后续若用户反馈再加
- 没在 BulkBar 之外暴露相对调整（如单条卡片右键菜单）—— 单条改 priority 已有
  PRIORITY field 直接拖；相对调整是 bulk-only 场景

## TODO 池剩余

- 设置页 raw YAML 加 lint
- 设置页 motion_mapping 加 motion 试播按钮
