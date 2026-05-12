# PanelMemory consolidate 进度 + cancel

## 需求

"立即整理"按钮按下后 button disable + "整理中…"几个字直到 LLM 跑完
（可能 10-30s）。用户不知道现在卡在哪个阶段、不能中断。加：

- 后端 emit 进度事件让前端能显进度条 + 当前阶段
- cancel token：用户随时点 ✕ 取消，pipeline 在下一个 checkpoint 退出

## 实现

### 后端 `src-tauri/src/consolidate.rs`

- 新静态 `CANCEL_FLAG: AtomicBool` —— 用户点取消时 store(true)
- 新 `cancel_consolidate()` Tauri 命令：set CANCEL_FLAG = true
- `trigger_consolidate` 入口先 reset CANCEL_FLAG = false（防上次残留）
- 新 `ConsolidateProgress` payload + `emit_progress(app, phase, progress, total)`
- 新 `check_cancel() -> Result<(), String>` —— flag 已 set 返 `Err("用户取消")`
- `run_consolidation` 内插 8 个 checkpoint：
  - `starting (0/8)` → `sweep reminders (1)` → `sweep plan (2)` → `sweep
    one-shot butler (3)` → `prune daily reviews (4)` → `archive old tasks
    (5)` → `butler daily summary (6)` → `prepare LLM prompt (7)` →
    `LLM thinking (8)` → emit `done (8/8)`
  - 每个 checkpoint 前 `emit_progress` + `check_cancel?`
  - LLM 调用本身无法 fine-grained 中断（pipeline 没暴露 cancel handle），
    最后一道 checkpoint 在 LLM 启动前，最有效的取消窗口在 sweep 阶段

### 注册 `src-tauri/src/lib.rs`

- `consolidate::cancel_consolidate` 加 invoke_handler

### 前端 `src/components/panel/PanelMemory.tsx`

- import `listen` from `@tauri-apps/api/event`
- 新 state `consolidateProgress: { phase, progress, total } | null`
- 挂载时 `listen("consolidate-progress")` → 更新 state；unlisten 在 cleanup
- `handleConsolidate` 初始 setConsolidateProgress({phase:"starting", 0, 8})
  让进度条立即出现；完成 / 失败 / 取消 时清回 null
- 检测错误信息含"用户取消"→ message 改成"已取消整理（已完成的步骤保留）"
- 新 `handleCancelConsolidate`：invoke cancel_consolidate；toast 提示
- UI：
  - "立即整理" 按钮旁条件渲染 "✕ 取消" 按钮（仅 consolidating 时显，红 tint）
  - 按钮 row 下方进度条 + phase label + N/8 计数；100% 流畅过渡

## 验证

- `cargo check` clean
- `npx tsc --noEmit` clean
- 行为：
  - 点"立即整理"→ 进度条出现，phase 文案在阶段间切换（sweep reminders →
    sweep plan → ... → LLM thinking → done）
  - 进度条 width 跟随 progress/total 比例
  - 整理期间点 ✕ 取消 → setMessage "已发出取消信号"；下一个 checkpoint
    pipeline Err 返回；message 变 "已取消整理（已完成的步骤保留）"
  - 已 sweep 完的部分（如 reminders 已删）不回滚（cancel 不是事务回滚，
    只是停止后续步骤）
  - LLM 已经开始流式输出后取消信号无法生效；要在前 2-3s sweep 阶段抢
  - 整理成功 → 进度条满 + 消失；message 显 LLM 总结

## 不在本轮范围

- 没让 LLM 调用支持 fine-grained 中断：那要 pipeline 暴露 cancel handle
  + 终止 reqwest stream。工程量大；本轮的"sweep 阶段可取消"已覆盖大部分
  实际使用场景（用户多在 LLM 启动慢时想中断）
- 没事务回滚：sweep 操作是原子的小步，取消 = 停止；已完成的 delete /
  archive 留下。语义"取消接下来的工作"而非"回到原状"
- 没显进度条 ms 倒计时：phase 文案 + N/8 已经够"知道还有几步"

## TODO 池

清空后按规则 #1 自主提出 5 条新需求。

## TODO 池新提案

1. PanelChat compose 草稿持久化 by sessionId
2. ChatMini 桌面气泡 markdown 块级语法
3. PanelTasks 归档独立 tab
4. PanelMemory hover 显 detail.md preview
5. PanelDebug 工具调用历史按 tool name group
