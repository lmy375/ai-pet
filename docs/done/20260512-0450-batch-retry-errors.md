# PanelTasks 一键重试所有 error 任务

## 需求

当前 task_retry 只能逐条点（单条 error 行的"重试"按钮 / 右键菜单 / 批量
工具栏 + 先勾 selected）。一段时间 LLM / network 抽风后会留 5-10 条 error
任务，逐个点繁琐。chip 行加一个 "🔄 重试错误 (N)" 红 chip 一键扫所有。

## 实现

`src/components/panel/PanelTasks.tsx`：

- 新 `errorTaskCount` useMemo（tasks 全集过 status==="error" 计数）
- 新 `handleRetryAllErrors` useCallback：
  - 直接扫 tasks 全集找 error 行，**不走 selected 路径** —— 与既有
    `handleBulkRetry`（用 selected）并行，让 chip 入口与 bulk 工具栏分离
  - 顺序 invoke `task_retry`，记 success / failed 计数 + 末尾 lastErr
  - 完成后 setBulkResultMsg "重试 N 条 ✓ [· 失败 M 条 (lastErr)]"、5s 自清、
    reload
  - bulkBusy 锁防双触
- chip 行加红色按钮：仅 `errorTaskCount > 0` 时渲染，bulkBusy 期间 disabled
  + 60% opacity。文案 "🔄 重试错误 (N)"，tooltip 解释执行顺序 / 失败汇总

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 一组任务全 pending → chip 不显
  - 3 条 error → chip "🔄 重试错误 (3)" 出现
  - 点击 → bulkBusy 标志 + chip disabled；逐条 task_retry；完成后 toast
    "重试 3 条 ✓"；error 行回到 pending → chip 消失
  - 部分失败 → toast "重试 2 条 ✓ · 失败 1 条 (LLM 超时)"
  - bulk 工具栏 selected-based 路径仍可用（如只想批重试某几条）

## 不在本轮范围

- 没改后端：task_retry 已存在，循环调用即可
- 没做"指数退避 + 并发"：error 数极少时（< 10）顺序串行已够；并发 +
  retry-backoff 是 incident response 级别的工程量
- 没做"自动定时重试 error"：定时重试无人监管会反复消耗 quota；用户手动
  一击更安全

## TODO 池剩余

- ChatMini 桌面气泡 pendingImages 缩略图条
- PanelChat session bar token badge 点击压缩历史
- PanelDebug LLM 日志单条复制为 cURL
