# PanelDebug 三 timeline tab 切换

## 需求

PanelDebug 顶部"应用" tab 内有三个紧挨着的 timeline 卡片：

- 🗯 宠物说（speech_history） — 紫
- 🔧 工具调用历史（tool_call_history） — 黄
- 💬 宠物反馈记录（feedback_history） — 绿

堆叠时占大量垂直空间，用户一次只关心其中一种。改成 tab-style toggle 让
用户聚焦看其中一种。

## 实现

`src/components/panel/PanelDebug.tsx`：

- 新 state `activeTimeline: "speech" | "tool" | "feedback"`，默认 "speech"
- 在三 timeline 卡上方插入 tab 行：
  - 单层 flex，padding 6/16，无大件背景
  - 每 tab 显 emoji + label + 计数（如"🗯 宠物说 12"）
  - active = accent 色字 + 下划线 2px accent；inactive = muted 字
  - cursor pointer / default 区分
- 三卡分别加条件包裹：
  - speech：`activeTimeline === "speech" && recentSpeeches.length > 0` 与
    既有空数组隐藏逻辑合并；新加一个 `=== "speech" && length === 0` 的
    "还没有宠物主动开口记录"占位（让 tab 切过去看不到 = bug 的体验消失）
  - tool：`activeTimeline === "tool" && (...)`；内部仍是既有"点 chip 展开
    list"的折叠形态，0 条 chip 会显 "(0)"
  - feedback：`activeTimeline === "feedback" && (...)`；同样保留内部既有
    filter chips / 展开逻辑

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 入页：默认 active = speech tab，仅紫色 speech 卡显示
  - 点 🔧 → speech 隐藏，黄色 tool 卡显示，内部 chip 仍可点展开
  - 点 💬 → 黄 tool 隐藏，绿色 feedback 卡显示
  - 计数实时反映背后数组长度（polling 来新条会自动更新 tab 上数字）
  - speech tab 在 list 空时显占位提示，不像 bug
  - 三 tab 切换之间 PanelDebug 其它内容（stats / 模态 / 风险表）不变

## 不在本轮范围

- 没改 PanelMemory 里的"butler 最近执行"timeline：那是 butler_tasks 记
  忆段的子视图，与 PanelDebug 三 timeline 并列没意义；本轮聚焦 PanelDebug
- 没做"展开多个 tab 并排"模式：横向空间也有限，本轮一次显一种已够。
  后续若用户反馈"想同时看 2 种"再加 multi-select
- 没动 reminders / proactive decisions 等其它卡：它们与 timeline 三件套
  语义正交，仍各自独立可见

## TODO 池剩余

- ChatMini drag-drop 图片到桌面气泡多模态
- PanelTasks task title 双击 inline 编辑
