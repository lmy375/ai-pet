# PanelTasks priority badge 色阶渐变

## 需求

P0..P9 badge 此前一致是 yellow tint，无视觉差异。用户扫长队列时不
易看到"还有几条 P0 红字紧急"。改成 5 档色阶让 priority 分布一眼可
读。

## 实现

`src/components/panel/PanelTasks.tsx` 把 `priBadge` 从 const object
改成 `(priority) => CSSProperties` 函数：

- P0 → 红（最紧急）：`#fee2e2 / #991b1b`
- P1-2 → 橙：tint-orange
- P3-4 → 黄（默认）：tint-yellow（base）
- P5-6 → 淡 muted：bg+fg base
- P7-9 → muted（idea 抽屉色）：bg+muted

call site `style={{...s.priBadge}}` → `style={{...s.priBadge(t.priority)}}`
唯一改一处（badge 仅在任务卡 header 用过一次）。

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - P0 任务卡 priority badge 显红底深红字
  - P1 任务卡显橙色
  - P3 任务卡显黄色（与之前一致）
  - P5 任务卡显 muted 灰色背景
  - P9 任务卡显 muted bg + muted fg（最不显眼）
  - 多条任务并列时一眼看到红 / 橙 / 黄 / 灰分布

## 不在本轮范围

- 没用 HSL 平滑渐变（10 档逐级）：5 档已能区分；逐级看不清
- 没改 priority chip filter 行的颜色：那是 filter UI，独立维度，保
  持 chip 行视觉对称
- 没改 priority picker 子菜单选项颜色：picker 是 modal 内的临时选择，
  着色冗余
- 没让色阶可配（用户自定义阈值）：3 个默认 zone 覆盖 95% 场景

## TODO 池剩余

- PanelChat marks modal "📋 全部复制" 按钮
- PanelMemory 顶部 export 单 category 下拉
- PanelDebug "立即开口" 加 "✏️ 编辑临时 prompt"
