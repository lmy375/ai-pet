# PanelMemory title 双击 inline 改名

## 需求

上一轮做了 PanelTasks task title 双击改名（复用新加的 `memory_rename` 后端
命令）。PanelMemory 是同一份后端数据但只有 "编辑" modal 修 description /
detail 内容；没法改 title。把 PanelTasks 那套 inline 编辑模式搬到 PanelMemory
统一编辑体验。

## 实现

`src/components/panel/PanelMemory.tsx`：

- 新 state：
  - `renamingMemoryKey: string | null`（同时仅一条 item 可改名；key =
    `${catKey}::${oldTitle}` 跨 category 唯一）
  - `renameMemoryDraft: string`
  - `renameMemoryBusy: boolean`（防双 commit）
- `commitRenameMemory` async：
  - 解 key → catKey + oldTitle
  - 空 / 同名 noop
  - 调 `invoke("memory_rename", {category, oldTitle, newTitle})`
  - **同步迁移 `pinnedKeys` 映射**：旧 `${catKey}::${oldTitle}` 命中时，
    删旧 key + 加 `${catKey}::${newTitle}`，写回 localStorage —— 否则改名
    后旧 pin 标记 dangling，新条目失去置顶
  - `loadIndex()` 重读 + 关 input
  - 失败 → setMessage 4s 红字
- `cancelRenameMemory`：直接 close
- 渲染：把原 `<div style={s.itemTitle}>{item.title}</div>` 换成 IIFE：
  - 当前在改名 → autofocus input + 1px accent 边 + Enter / Blur 提交、Esc
    取消
  - 否则 → `<div onDoubleClick>` 包原 title，cursor:text + tooltip "双击改名"

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 双击某条 memory title → 变 input，旧 title 已填入
  - Enter / Blur → 调 memory_rename → loadIndex → 卡 title 立即更新
  - 改名前条目被 📌 pin → 改名后 pin 标记跟到新 title（list 重排时仍置顶）
  - Esc → 关 input，无变更
  - 重名（同 category 已有该 title） → setMessage 显错误 4s
  - 改名期间 input disabled，防双 commit

## 不在本轮范围

- 没改 ai_insights / current_mood：后端已守门拒绝；用户双击改名会得到
  "current_mood is not renameable" 错误（清晰即可）
- 没做"批量重命名 by regex"：单条改名足够日常；批量改名风险高（容易误
  动 ai_insights 等系统类目）
- 没在搜索结果列表（searchResults）内开放改名：搜索结果是 read-only 跳转
  视图，编辑路径走类目内

## TODO 池剩余

- ChatMini 桌面气泡 pendingImages 缩略图条
- PanelChat session bar token badge 点击压缩历史
- PanelDebug LLM 日志单条复制为 cURL
- PanelTasks 批量重试所有 error 任务
