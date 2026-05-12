# PanelTasks 取消原因历史 datalist

## 需求

PanelTasks 单条 / 批量取消任务时弹 reason input。用户常用的几条原因
（"已不需要" / "转给人工" / "时间过了"）会重复出现，每次手敲累。补
最近 5 个 reason 历史 datalist 自动完成。与 iter #201 PanelMemory
search history datalist 同 pattern。

## 实现

`src/components/panel/PanelTasks.tsx`：

- 新 state `cancelReasonHistory: string[]`，localStorage key
  `pet-tasks-cancel-reason-history`，挂载读
- `pushCancelReasonHistory(reason)` helper：trim 空校验 + 去重 + cap 5 +
  写盘
- 两条 cancel-confirm 路径都在成功后推 history：
  - `handleCancelConfirm` 单条 task cancel
  - `handleBulkCancelConfirm` bulk cancel
- 两个 input 都加 `list="pet-tasks-cancel-reason-history"`：
  - 单条 cancel 行（line ~4448）
  - bulk cancel sub panel（line ~3334）
- 渲染单一 `<datalist>` 在 root 容器内顶部 —— 两个 input 共用同一 id
  且空 history 时不渲（datalist 无 option = 浏览器不浮 dropdown）

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 全新用户：空 history → 两个 cancel input 无 autocomplete 行为
  - 单条 cancel 输入 "转给人工" → 成功 → history = ["转给人工"]
  - 再点 cancel 另一条 → input 自动 dropdown 显 "转给人工"
  - bulk cancel 输入 → 成功也写入同一 history 集
  - 重复输入相同 reason → 移到首位（recency-bias）
  - 输入 > 5 条 → cap 5 队列 FIFO
  - 空 reason / 仅空白 → 不写 history（防误污染）
  - 重启 panel → localStorage 还原
  - localStorage 损坏 / 私密模式 → 空 history 退化，不阻塞 cancel

## 不在本轮范围

- 没显"清除历史"按钮：场景边际；用户清 localStorage 即清。可后续加
  到 PanelSettings 顶部
- 没做"全局共享 / 单 task scoped"：所有 task 共用同一 history（更实
  用 —— "转给人工" 适用任何任务）
- 没集成 cancel 原因 → decision_log marker 关联：cancel 已经走 decision_log；
  历史 datalist 只关心 UX，不动 log 语义
- 没做"按使用频次排序"：recency 优先比 frequency 更直观

## TODO 池剩余

- PanelDebug recent turns ring buffer modal 加 outcome filter chips
- PanelChat 消息加 "📌 标记" 按钮
