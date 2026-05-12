# PanelTasks hover preview "⚡ NOW" 倒计时 chip

## 需求

iter R94 引入"⚡ NOW"任务标记（60s 内浮顶 + 桌面气泡 nudge）。卡片上
已有动画 chip 提示，但 hover preview tooltip 里没暴露 mark 状态 +
剩余秒数。用户多任务并存时翻 hover 看哪条还在 NOW 窗口内、还剩多
久 —— 没入口。加 chip 倒计时。

## 实现

`src/components/panel/PanelTasks.tsx`：

- 既有 `nowMarkedTitles: Set<string>` 不带 ts 信息（用户加 mark 即进
  set，60s 后 timer 自动移除）。新增并行 `nowMarkedAtRef: useRef<Map<title,
  epochMs>>`：
  - markTaskNow 时 set 当前 ts
  - 60s timer fire 时也清掉
  - 用 ref 而非 state —— 避免每秒更新触发整面板 rerender；倒计时是
    hover 时单次读取
- hover preview 内：
  - 计算 `isNowMarked + nowRemainingSec`
  - hasChips 条件加入 `isNowMarked` 触发
  - chip 行头部插入 ⚡ NOW chip（最左侧，与卡片浮顶语义一致）：
    - orange tint 配色（与既有桌面 nudge 色匹配）
    - 文案 `⚡ NOW {N}s`
    - title tooltip 说明剩余秒 + 桌面 nudge 已发送
- 数字精度：hover 单次计算（ceil((60 - elapsed) / 1）—— 用户再 hover
  时取新值，省 setInterval 1s 翻面板的浪费

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 卡片上点 NOW 按钮 → nowMarkedTitles 含 title + markedAt ref set
  - hover 该任务 → tooltip chip 行最前显 "⚡ NOW 58s"（或 N s 取决于
    hover 与 mark 的时间差）
  - 隔几秒再 hover → 数字更新（每次 hover 重算）
  - 60s 后再 hover → 标记自动消失 → chip 不浮（与 set 清除同步）
  - 多任务同时 NOW 标记 → 各自独立倒计时
  - mark 后立即在卡片上看到既有的浮顶 chip（pulse 动画）和动画 + 现在
    hover 也能看 chip 倒计时

## 不在本轮范围

- 没做每秒实时更新（setInterval refresh）：用户主动 hover 时取读已经
  够；持续 refresh 会让整面板 1Hz 重渲，得不偿失
- 没在 NOW chip 上加"延长 60s"按钮：mark 已经支持 re-click 重置；hover
  tooltip pointer-events: none 也不该承担动作
- 没改 NOW chip 自身的视觉（卡片上的脉冲 chip）：iter R94 已有动画
  好用；本轮 scope 限 hover tooltip
- 没让倒计时支持自定义 60s 阈值：常量级配置；future 若用户提需求再做

## TODO 池剩余

- PanelSettings "📋 导出全部 settings 为 markdown" 按钮
- PanelPersona "重置 SOUL.md 为内置默认" 按钮
- PanelChat session 切换草稿提示 toast
