# PanelTasks hover preview 加 priority / due / tags chips

## 背景

iter #172/#180 把 hover preview tooltip 升级为 history + detail.md 双
段。但用户回看任务时还想立刻看到"这条任务的元数据" —— 优先级 / 截止 /
打的什么 tag。本来这些都在卡片上（priority badge / due 字段 / tag
chips），但卡片折叠态信息稀疏，多任务并列时扫读还是要逐条 hover 看。
把元数据 chips 也塞进 tooltip 头部，让 hover 一眼看到三大维度。

## 删除一个 TODO 项

同时把"PanelDebug 上次 manual fire 加 source 字段"从池中移除：经审查
后发现 frontend 已通过 `title === null ? '全局 fire' : '▶️ 「title」'`
分辨入口；且现仅 panel 路径触发，没 telegram 等第三入口需要列。属
"我自己提的冗余项"，dev log 此条同步声明。

## 实现

`src/components/panel/PanelTasks.tsx` hover preview 渲染段：

- 计算 hasChips 信号：`priority !== 3 OR due 非空 OR tags 非空`
  - priority=3 是默认值，单独不算"携带信息"；与"新建表单 priority 字段
    默认 3"语义对齐
- 空检查扩展：`!hasChips && history 空 && detail 空` 才彻底不浮 tooltip
  （之前仅看 history + detail）
- 在 tooltip 顶部新增 chips row（hasChips 时浮）：
  - 🎯 P{n}（仅 priority !== 3）
  - 📅 due（slice(0,16) replace T → 空格）
  - #tag（每个 tag 一 chip）
- chip 视觉走 muted bg + fg 黑字 + 圆角 3px + 等宽 inherit 字体
- chips 与下面的 history / detail 段间用 dashed border-top 分隔（仅
  在下方有内容时显，避免空 hr）
- tooltip maxHeight 从 260 提到 280 容纳新行

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 全新任务 default state（P3 无 due 无 tags 无 history 无 detail）→ tooltip 不浮
  - 设 P5 → tooltip 显 "🎯 P5" chip
  - 设 due "2026-05-15 18:00" → tooltip 显 "📅 2026-05-15 18:00" chip
  - 打了 #工作 #紧急 → tooltip 显两个 #tag chip
  - 同时有 chips + history → 上 chips / 虚线 / 下 history / detail
  - 仅 chips → 无虚线（避免空段被分隔）

## 不在本轮范围

- 没显 tag 颜色（tag chip 已有用户自选 tint）：hover 是 preview，颜
  色 sync 需要从 tagColors 拿，scope 不大但本轮聚焦 affordance；后续
  可加
- 没把"今日 due / 已逾期"色化进 due chip：现单色显时间字符串，用户
  按字面读；与 task card 上的 due 颜色编码冗余
- 没显原始 description 摘要（已有 raw_description tooltip 在 itemHeader
  on hover）：两层 hover 重叠会乱
- 没把 chip 顺序做成用户可配：固定 priority → due → tags 三段，符合
  "重要 / 时态 / 分类"信息阶层直觉

## TODO 池剩余

- PanelMemory butler_tasks 段 schedule 类别 chip 过滤行（[every:] / [once:] / [deadline:] / 无 schedule 四档）
- PanelTasks header 加"清除全部已结束（done / cancelled）"按钮
- PanelTasks 创建表单加 "📋 从模板" 下拉
