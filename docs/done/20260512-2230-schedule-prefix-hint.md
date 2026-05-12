# PanelTasks 新建表单 schedule 前缀 inline 提示

## 需求

PanelTasks 队列是一次性派单：用户写 title + 描述 → pet 立即执行。
但用户常把 butler_tasks 的 schedule 前缀（`[every:]` / `[once:]` /
`[deadline:]`）误敲到 PanelTasks 的创建表单 —— 因为 schedule 语法
看起来"任务相关"。结果是一次性任务带个无效 marker，pet 不会定时
执行，用户也不知道哪里错。inline hint 让错误尽早 surface。

## 实现

`src/components/panel/PanelTasks.tsx` 两处补 hint（inline form +
quickAdd modal）：

- 检测正则 `/\[(every|once|deadline)[:\s]/i` —— 匹配 `[every:` 与
  `[every ...]` 两种形态（容忍空格 / 大小写）；不要求闭合 `]`，让用
  户敲到一半就触发
- 检测 title OR body 两个字段 —— 用户可能在任一处敲
- yellow tint 警示底 + 💡 灯泡 + 文案：
  - "检测到 schedule 前缀 —— 想定时 / 周期执行？建议改在「记忆」面
    板的 butler_tasks 段新建（pet 会按时间自己跑）。这里建的任务是
    一次性"立即派单"。"
- inline form 与 quickAdd modal 同款 hint（quickAdd 文案略简，因为
  modal 内空间紧）
- 检测是 reactive（每 keystroke 重算），不挡保存 —— 用户仍能强行提
  交（万一有合理误用场景）

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 敲 title "[every: 09:00] 早安" → 浮 hint
  - title 不含但 body 含 → 仍浮（在 title input 下方与 body 之间）
  - 删除前缀字 → hint 立刻消失
  - 提交按钮不被 hint disable —— "你要这么干 ok 但提醒过"
  - quickAdd modal 内同样工作
  - 大小写 `[Every:` / 带空格 `[every 09:00]` 都命中

## 不在本轮范围

- 没做"点 hint → 直接跳到 PanelMemory butler_tasks 新建"按钮：跨 panel
  state lift + prefill PanelMemory editor 涉及多组件 + 大量字段抽取，
  scope 翻倍；当前 hint 是引导而非动作
- 没做语法高亮：title input 单行 plain text；schedule marker 视觉化
  需要 contenteditable，不值
- 没做 body textarea 内的"已检测 schedule 前缀（位置 X-Y）"内联标
  注：banner 已经足够说明问题
- 没改 backend 拒绝带 schedule 的 PanelTasks 创建：那是 hard rule，
  用户偶有合法误用空间，hint 是更温和的选择

## TODO 池剩余

- PanelMemory 顶部搜索框加最近 5 个 keyword 历史下拉
- PanelChat ⌘K task picker 加 char-order 子序列 fuzzy 匹配
- PanelDebug 加 "重置 in-process stash" 按钮
