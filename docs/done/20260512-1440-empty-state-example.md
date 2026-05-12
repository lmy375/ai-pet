# PanelTasks 队列空状态 "用范例预填一条" 按钮

## 需求

PanelTasks 队列空时只显纯文案"还没有任何任务"。quickAdd 模态里
placeholder 是灰字 "比如：整理 Downloads" / "把要点说清楚，比如：把
30 天前的文件挪到 ~/Archive/" —— 但 placeholder 必须用户先点 + 新建
后才能看到，新装用户在 panel 第一次打开时直接看到空状态会卡住不知
道下一步是什么。给一个一键按钮，把 quickAdd 直接预填具体值并打开，
比 placeholder 提示强很多。

## 实现

`src/components/panel/PanelTasks.tsx` 单文件：

- 空状态文案下加 `<div style={{marginTop: 12}}>` 容器
- 仅在 `!filtersActive && showFinished` 时浮按钮：
  - filtersActive=true 是用户主动过滤掉了所有任务，弹引导反而打扰
  - showFinished=false 是切到"仅进行中"视图空，可能 archive 里有大量
    历史任务，浮"预填新任务"按钮不合适
  - 两个 false 即"真正什么都没有"才浮 —— 大多数是首次打开
- 按钮：
  - 文案 "📋 用范例预填一条"
  - accent 描边 + accent 字 + card 底，与既有 primary button 视觉
    一致但权重轻一档（hint 不是 primary action）
  - 点击 → setTitle("整理 Downloads") + setBody("把 ~/Downloads 里
    30 天前的文件挪到 ~/Archive/，按月份分子目录。\n做完在 detail
    写一句「已挪 N 个文件」，列出最大的 3 个文件名。") + setPriority(3) +
    setDue("") + setQuickAddOpen(true)
  - 复用既有 quickAdd state + handleCreate 流程，无新增 state machine

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 全新 panel（没任务）→ 看到 "📋 用范例预填一条" 按钮
  - 点击 → quickAdd 模态弹开，title / body / priority 已填好
  - 直接点保存 → 任务创建成功 → 空状态消失，按钮消失
  - 创建任务后再 filter 到"无匹配"空状态 → 按钮不浮（filtersActive）
  - 创建任务后切"仅进行中"且全 done → 按钮不浮（showFinished=false）
  - 改完范例值再保存 → 用户自己的版本进队列（不强绑 fixed 文本）

## 不在本轮范围

- 没做多个范例按钮供选择（"整理 / 提醒 / 周回顾" 各一）：单按钮 +
  一个具体例子已能教会用户"任务长什么样"；多按钮变选择题加摩擦
- 没做"我会自己写，藏掉" dismiss：空状态本就只在首次打开 + queue
  真空时浮，建一条就消失，不需要持久化 dismiss
- 没做 schedule 前缀范例：PanelTasks 是任务队列（直接交付）而非
  butler_tasks memory（定时模板）。schedule UX 引导应放在 PanelMemory
  butler_tasks 新建模态里，那是单独一条 TODO
- 没做 i18n：当前 panel 全中文，没有 i18n 框架

## TODO 池剩余

- PanelTasks header "今日 due" quick filter chip
- PanelMemory butler_tasks 新建 modal "从现有任务复制 schedule" 下拉
- PanelTasks detail.md 编辑面板 > 5000 字 banner
- PanelChat ⌘K task 引用选择器
