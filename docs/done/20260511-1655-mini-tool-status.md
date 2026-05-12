# ChatMini 流式中显当前 tool 名

## 需求

宠物调 tool 时 streaming chunk 流停下来 —— 桌面 mini chat 看上去像"卡死"。
useChat 已有 `toolStatus` state（panel 路径用），但 ChatMini 没消费。透传过
去显一行小字"✅ X done"，让用户知道宠物在执行而非死循环。

## 实现

`src/App.tsx`：

- useChat 解构加 `toolStatus`
- `<ChatMini toolStatus={toolStatus} ... />` 传过去

`src/components/ChatMini.tsx`：

- Props interface 加 `toolStatus?: string`
- 组件函数签名解构
- 在 mini chat 列表底部（streaming bubble 下方）加渲染分支：
  - 仅 `isLoading && toolStatus && trim 非空` 时显
  - 小字 italic muted，`title` tooltip "正在执行工具：X"

useChat 的现有 toolStatus 文案是"✅ X done"（tool 完成时设的）；它会被下一
个 chunk 清空，所以这条只在"tool 刚 done 但下一段文本还没流出"的窗口显，
正是用户需要"知道宠物在搞 tool 而不是卡"的时刻。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 桌面问宠物"看一下天气" → streaming 跑一段后 chunk 停 → 出现"✅ get_weather done" → 不久 stream 继续 → 这条消失
  - 单纯无 tool 流式 → 始终不显
  - cancel / done 都让 toolStatus 复位 → 自动消失

## 不在本轮范围

- 没改 toolStatus 文案 / 加 toolStart 路径细节（"调用 X 中..."而非"✅ done"）
  —— 那需要扩 useChat 的状态机；当前 toolStatus 设的是 done 时刻，对桌面
  显示已够用
- 没把 toolStatus 渲到 streaming bubble 内部（与 currentResponse 同 bubble
  视觉混合）—— 单独 row 更清晰

## TODO 池剩余

- 重启 pet 窗口加 reload 当前窗口语义
- PanelTasks 任务行右键菜单
- PanelChat 跨会话搜索 hit 高亮
