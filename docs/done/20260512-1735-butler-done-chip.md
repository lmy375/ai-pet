# PanelMemory butler_tasks ✅ 已完成 chip

## 需求

butler_tasks item 描述里 LLM 标 `[done]` 后，前端只通过 stripErrorBlock
strip 错误段，但没有针对 `[done]` 的视觉 chip。用户看 PanelMemory 这条
任务"是 pending 还是 done"得读 raw description 末尾的 marker（已被
stripDoneBlocks 隐藏 → 用户看不到任何信号）。后端 TaskView.status 已
经把这个信号转 status="Done"，但 PanelMemory 不走 TaskView 路径 —— 它
读 raw MemoryItem。补一个绿色 ✅ chip。

## 实现

`src/components/panel/PanelMemory.tsx`：

### 解析

新增 `parseButlerDone(desc)` → `{isDone: boolean, result: string}`：
- 用 `/\[done(?:\]|\s[^\]]*\])/` 匹配 `[done]` 或 `[done at=...]` 之类
  扩展，但拒绝未闭合 `[done...`。与后端 `has_done_marker` 行为对齐
- `[result\s*[:：]?\s*([^\]]*)\]` 抽出 LLM 在标 done 时常附的产物摘要

新增 `stripDoneBlocks(s)`：把 `[done...]` 与 `[result: ...]` 段从显示
正文剥掉，让 chip 与 displayDesc 不重复

### 渲染

在 butler_tasks 段每条 item 行的 chip 区，error chip 之前加 ✅ 已完成 chip：
- 仅 catKey=butler_tasks 触发解析
- 与 error chip 互斥：errInfo.hasError 时不解析 done（重试中状态以失败
  为优先，let user 先处理失败）
- chip 视觉：tint-green-bg + tint-green-fg 圆角描边 + 600 weight
- 显文案：`✅ 已完成` + （result 非空时）`: result 截 30 字`
- title tooltip 显完整 result（未截断），未填时提示 "LLM 已标 done（未
  填具体 result 段）"

### displayDesc

`stripErrorBlock` / `stripDoneBlocks` 两条剥离链按需触发：
- errInfo.hasError → 走 stripErrorBlock
- doneInfo.isDone → 走 stripDoneBlocks
- 都没有 → 原 parsed.topic 或 raw description

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - butler_tasks 某 item 描述含 `[done]` → 行内浮绿色 ✅ chip
  - 描述含 `[done] [result: 38 个文件已归档]` → chip 显 "✅ 已完成：38
    个文件已归档"，title tooltip 显完整文本
  - 描述含 `[done at=2026-05-12]` → 仍识别为 done（容忍扩展语法）
  - 描述含 `[error: 失败]` 又含 `[done]`（罕见 corrupt 状态）→ 优先
    error chip，不显 done chip
  - 描述里 `done` 单词出现（非 bracket token）→ 不误判
  - displayDesc 不再带 `[done...] [result: ...]` 段（已 strip）

## 不在本轮范围

- 没显 done 时间：后端 TaskView 有 updated_at，但本路径走 raw MemoryItem，
  没经 build_task_view —— 要拿精确"何时标 done"得 cross-ref TaskView
  或解析 `[done at=...]`。当前 LLM 一般只写 `[done]` 不带 at；改进等
  prompt 让 LLM 必填后再做
- 没做 result 段 markdown 渲染：chip 是 inline ellipsis；想看格式化版本
  打 detail.md
- 没改后端：done 解析逻辑前端独立 mirror，避免 PanelMemory 走 task_list
  整 round-trip 拿 TaskView.status 字段
- 没把 done chip 信号 hook 进 hover preview tooltip：preview 已显
  detail.md + history，"是不是 done"在 chip 上一眼可见，重复展示噪声

## TODO 池剩余

- PanelTasks task 操作后清掉 detailMap[title]
- PanelChat 复制按钮 alt-click 复制为 markdown
- PanelDebug 加 "上次 manual fire" 行
- PanelChat 双击 `「task title」` ref → 切到 PanelTasks tab + scroll
