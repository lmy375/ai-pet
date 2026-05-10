# 三件套：自我画像编辑 / Esc 拒绝 / 任务页 d r 快捷键

> 对应需求（来自 docs/TODO.md）：
> 1. PanelPersona 自我画像支持手动编辑：用户可以纠正 LLM 写歪的 personaSummary，写回 ai_insights/persona_summary。
> 2. 工具审核弹窗按 Esc 等价于「拒绝」：快速否决路径，避免必须点按钮才能拒绝。
> 3. PanelTasks 行级键盘快捷键：在键盘焦点行按 d 标 done、r 触发 retry。

## 1. 自我画像手动编辑

`PanelPersona`：
- 状态：`editingPersona` / `personaDraft` / `savingPersona` / `personaError`。
- 入口：在 personaSummary 渲染段的"X 天前更新"小字旁加「✏️ 编辑」按钮，进入编辑态切到 textarea。
- 保存：`memory_edit("update", "ai_insights", "persona_summary", desc)` → 失败时 fallback 走 `create`（用户在 consolidate 跑之前就要编辑的边缘场景）。空字符串拒绝；保存成功立即更新本地 personaSummary / personaUpdatedAt 不等 5s 轮询。
- 取消：丢弃 draft 回展示态。
- 没有动后端 —— 完全复用既有的 `memory_edit` Tauri 命令。

## 2. 工具审核弹窗 Esc=拒绝

`PanelDebug`：在 `handleToolReviewDecision` 之后加一条 `useEffect`：
- 仅在 `pendingReviews.length > 0` 时挂 keydown。
- Esc → 拒绝 `pendingReviews[0]`（最上面那条）；处理后数组缩短，下次 Esc 自动作用到新的第一条。
- modal 底部 hint 改成「超过 60 秒未响应将按默认安全策略拒绝；按 Esc 立刻拒绝最上面这条」。
- 没用户态、没新命令；纯 UX 加速。

## 3. 任务页 d / r 快捷键

后端：
- `task_queue::append_done_marker(desc)`：纯 helper，幂等（已含 `[done]` 时原样返回）。4 单测覆盖正常追加 / 已 done 幂等 / done+result 幂等 / 空串。
- `commands/task::task_mark_done` Tauri 命令 + `task_mark_done_inner` 包装。终态行（done / cancelled）拒绝再标，决策日志写一条 `TaskMarkDone`。
- `lib.rs` 注册命令。

前端：
- `useTaskKeyboardNav` 接受新的 `handleMarkDone` / `handleRetry` 入参，对应 ref-stable 闭包。
- 监听 `d`：pending / error 行响应，调 handleMarkDone（与 hook 既有 Delete = 取消同模式 fire-and-forget）。
- 监听 `r`：仅 error 行响应，调 handleRetry。
- 守卫：无 modifier；tagName 守卫已挡掉 input / textarea / button focus。
- `PanelTasks` 新增 `handleMarkDone` 函数（与 `handleRetry` 同 invoke + reload + busyTitle 模式），把它和 `handleRetry` 一并传进 hook。

## 验证

- `cargo test --lib` → 892 passed（888 旧 + 4 append_done）。
- `tsc --noEmit` 干净。
- `cargo check` 干净。

## 未做

- d 没有 mouse 镜像按钮 —— 行内 retry / cancel 按钮已存在，没有"标 done"按钮（done 之前一律由 LLM 自己标）。本轮先只暴露键盘入口，后续如果发现 d 用得多再补按钮。
