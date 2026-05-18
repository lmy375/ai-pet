# ChatMini bubble「📊 全谱 chip rack」— 设计权衡 pivot drop（iter #584）

## TODO 提案

「ChatMini bubble 顶 chip 加 「📊 全谱 chip rack」hover-展开 — 把分散
在四角的 ⏱/📊/💭/✏️/↺ chip 收纳。」

## 既有 chip 空间分布

ChatMini bubble row 现有 4 个 hover-revealed chip 分布 4 corner：

```
       top-left                                 top-right
       ⏱ ts (assistant)                         📊 chars (assistant)
       OR 📊 chars (user)                       OR ⏱ ts (user)
                          ┌──────────┐
                          │  bubble  │
                          └──────────┘
       bottom-left                               bottom-right
       ↺ ⌘R 重发 (assistant, isLast)            ⏱ rel (assistant)
       OR ⏱ rel (user)                          OR ✏️ ⌘+click 编辑重发 (user)
```

**位置 vs 角色对称**：user 与 assistant 镜像（chips swap sides 对应
bubble 对齐方向）。每 corner 服一类信号：
- 顶 ts (绝对时刻) — 时钟侧
- 顶 chars (字数 ambient awareness) — bubble 内容统计侧
- 底 rel (相对时间) — 与顶 ts 形成视觉对偶（绝对 + 相对）
- 底 action hint — discoverability 提示

## 「rack」设计的隐含 tradeoff

把 4 corner chip 合并到一个 rack：

### 收益
- 视觉密度低 — 单 icon 而非 4 个分散 chip
- 一致 UX — 不必记「ts 在哪 corner」

### 失去
- **空间编码语义丢失**：4 corner 各服一类信号；rack 把信号 source 拍
  扁后，owner 看 rack 内文字时大脑要重新 parse「这是 ts 还是 rel」—
  比直接 corner 位置识别多一步认知
- **ambient peripheral 信号没了**：现有 chip hover 才显但 corner 位置
  固定 — 余光扫到该 corner 就能预期信号类型。rack 化后 owner 必须主
  动定位 rack + hover 才能读，认知成本上升
- **角色镜像被打破**：现 user / assistant chip 位置 swap 实现「视觉
  对称」— rack 化为固定位置后丢失这层心智模型
- **discoverability hint 被埋藏**：↺ ⌘R 重发 / ✏️ 编辑重发 hint 本就
  是 discoverability nudge — 藏 rack 内后 owner 更不知有此入口
  （iter #573 教训）

### 平衡评估

提案的「视觉密度」收益对 bubble row 已有 4 chip 来说是真实存在的
（ambient cluttering 风险）。但用户实际抱怨吗？

迄今没收到「ChatMini bubble chip 太挤」owner feedback。各 chip 是
opacity 0 → 0.5 hover 才显，idle 态完全透明 — clutter 仅在 hover 时
出现，rest 态 bubble 干净。

## Decision

**不实现 chip rack**。3 条理由：

1. 现 chip 已 hover-gated（opacity 0 默认）— "clutter" 仅 hover 态可
   见；rest 态 bubble 是干净的，没有持续视觉负担
2. 4 corner 空间编码 + 角色镜像是 deliberate UX 选择 — 牺牲这层心智
   模型换「单点 rack」收益不清
3. discoverability hint chip（iter #573）藏起来违 add hint chip 的
   初衷

procedure 教训：consolidation 类提案需在「单点便利 vs 空间分布」UX
tradeoff 明确收益时才做。本案缺 owner 反馈支撑 — 「他人觉得 cluttered」
不等于 owner 自己觉得 cluttered。

## Alternative：聚焦实际痛点

如未来 owner 反馈「我看 chips 累」：

### Option A：单 chip 切换详细 vs 紧凑模式
- 加一个 toolbar toggle「📊 chip 详细 / 紧凑」让 owner 自由选
- 紧凑模式：仅 ts + rel；详细模式：全 chip family
- 不改 chip 位置，仅控显隐 → 心智一致

### Option B：减少 chip 默认数量
- 仅核心 chip（ts / rel）默认 hover-revealed
- 高级 chip（chars / action hint）走 ctx menu / 设置项
- 降低默认密度但保 corner 编码

### Option C：消息焦点态 chip 持久可见
- click bubble → 选中态 → 该 bubble 所有 chip 持久浮（不 hover-gate）
- 其它 bubble 仍 hover-only
- owner 关注某条时一眼看全 chip，不关注的不被打扰

## Future iters (out of scope)

- 等收到 owner 实际「chip 太多 / 太乱」feedback 后再选 Option A/B/C
  实施
- 当前 4 corner chip family 维持现状 — 各 chip 都通过设计 review，无
  显式 regression
