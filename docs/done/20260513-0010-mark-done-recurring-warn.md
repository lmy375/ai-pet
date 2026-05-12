# PanelTasks 手动标 done 时 recurring schedule 警告

## 需求

iter #196 给鼠标"标 done"路径加了 result 摘要 dialog。但有种情况
应该额外警告：task description 含 `[every: HH:MM]` 表示这是循环
schedule，标 done 让宠物下一周期跳过（视作已完成）。多数情况下用户
真意是"今天这条不再做"（应该 cancel）或"整条 schedule 退役"（应该
到 PanelMemory 删 item / 改 description）。inline warn 让误标先冷却。

## 实现

`src/components/panel/PanelTasks.tsx` mark-done dialog 内：

- 头部标题下、result label 上插入条件渲染 warn block
- 检测：从 tasks 数组找 markDoneTitle 对应 task，正则 `/\[every[:\s]/i`
  test t.raw_description
- 仅 `[every:` 匹配触发；`[once:]` / `[deadline:]` 不触发 —— 一次性
  schedule 本就该 done 收尾，警告反而干扰
- yellow tint 警示底 + ⚠ + 文案：
  - "这是循环 schedule（含 [every: ...]）。标 done 之后宠物会把它当
    '已完成' 跳过下一周期。如果你想'今天这条不要再做'，用'取消'更准
    确；想 retire 整条循环，请到「记忆」→ butler_tasks 删除或改
    description。"
- 不挡确认按钮 —— 用户仍能强行标 done（万一意图就是这样）

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 普通一次性 task → 标 done dialog 浮，无 warn
  - `[every: 09:00] 早安日程` 任务 → 标 done dialog 浮 warn banner
  - `[once: 2026-05-15] 文档发出` → 标 done dialog 浮，无 warn（一次性）
  - `[deadline: 2026-05-15] 提交方案` → 标 done dialog 浮，无 warn（一
    次性截止）
  - warn 不影响 result 填入 / Esc 取消 / 确认按钮可用

## 不在本轮范围

- 没做"快速跳转到取消" / "快速跳到 PanelMemory 删 item" 链接按钮：用
  户读完 warn 自行决定路径；inline 按钮会让 dialog 拥挤
- 没改键盘 d 路径加同款警告：键盘是零摩擦快路径，警告会破坏 muscle
  memory；warn 仅在 dialog（鼠标点击触发的路径）出现
- 没让 warn 自动转移到 cancel dialog：cancel 是独立 inline 流程，warn
  那边语义不同 —— 这里是"你想用 cancel 取消（更准确）"，cancel 流程
  本身没歧义
- 没拒绝 mark done（变成 hard block）：循环 task 偶尔确实要 done（如
  schedule 失效但 description 没改）；keep soft warn 让用户选

## TODO 池剩余

- PanelChat 长消息折叠中段文本可直接点击展开
- PanelTasks 多选 bulk action 加 "🔗 拼为 ref 列表" 复制
- PanelDebug 加 "复制全部 stash + settings 为 issue 模板"
