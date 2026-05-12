# PanelChat 长 session "↓ 跳到最新" 浮动按钮

## 需求

iter R103 给 PanelChat 加了 "↑ 跳到顶" 浮动按钮，让用户翻到老消息
后能快速回到会话开头。但反向的"已经翻到老消息 → 想回到最新"没入
口：用户得手动拖滚动条到底（长 session 几千行很累）。补一个对称
的 "↓ 跳到最新" 按钮。

## 实现

`src/components/panel/PanelChat.tsx`：

- 新 state `scrolledFromBottom: boolean`
- `onScroll` handler 加距底计算：
  `distFromBottom = scrollHeight - scrollTop - clientHeight`，
  > 200 → 浮 ↓ 按钮（与 ↑ 的对称阈值同形）
- 渲染两个按钮垂直堆叠：
  - ↑ 上移到 `bottom: 60` —— 原 `bottom: 16` 位置改给 ↓
  - ↓ 在 `bottom: 16` —— "向下"动作位置低，箭头方向 = 滚动方向，
    让用户不必读字就能找对
- 两 button visibility 互补但不互斥：中段滚动时可能同时浮（让用户两端都能跳）
- ↓ onClick：`scrollRef.current?.scrollTo({ top: scrollHeight, behavior: "smooth" })`

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 进入长 session，自动滚到底 → 仅 ↑ 浮（顶距 > 200）；↓ 不浮（底距 < 200）
  - 用户手拖到顶 → 仅 ↓ 浮（底距大）；↑ 不浮
  - 中段位置 → ↑ 与 ↓ 同时浮，垂直堆叠 ↑ 上 ↓ 下
  - 点 ↓ → 平滑滚动到 scrollHeight；自动滚动 effect 后续仍正常推进
  - 短 session（< 600px 高）→ 两按钮均不浮
  - 切换 session 后两 state 重新触发 onScroll → 状态正确

## 不在本轮范围

- 没显"距最新 N 条未读"badge：要持久 unread state 跨重启 / per-session，
  scope 翻倍；当前 ↓ 是纯导航
- 没在新消息到达且用户在历史时显 "有 N 条新消息 · 跳到最新" 提示
  bar：复杂度高（要 hook items append + diff 当前 scroll 位置）；
  本轮做无 badge 的纯按钮
- 没改 ↑ 与 ↓ 按钮样式（如 ↓ 主色 / ↑ 副色）：保持两者同款让用户感知
  对称；"哪个更重要" 由位置（下 = ↓ 更近）传达

## TODO 池剩余

- PanelTasks header 加 "P0 一键过滤" chip（注：可能与既有 priority chip 重复，待 review）
- PanelTasks 手动标 done 时弹 "可选 result 摘要" 输入对话框
- PanelMemory butler_tasks item 描述里的「title」ref hover preview / 双击导航
