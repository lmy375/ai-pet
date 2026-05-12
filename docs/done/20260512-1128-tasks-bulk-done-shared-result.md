# PanelTasks bulk "✓ 标 done" 共享 result input

## 背景

单条任务 mark done 有 dialog 选填 `[result: ...]`（与 LLM 自动标 done 形态一致）。批量工具栏一直缺这条路径 —— 想"一批 5 个任务一次性标 done + 同一段 result"只能逐条点开 dialog。

## 改动

`PanelTasks.tsx`：

1. `bulkAction` 类型扩展加 `"done"`。
2. 新 state `bulkDoneResult: string` 持有共享 result 文本。
3. 新 handler `handleBulkMarkDoneConfirm`：走 `runBulk("标 done", pending|error → task_mark_done(title, result|null))`；终态任务跳过。result trim 后空串 → 传 `null`（等价键盘 d，仅追加 `[done]`）。
4. 工具栏在"重试"按钮后插一颗 `✓ 标 done` 切换按钮，跟其它 `cancel / priority / due / tags` 同款 active 状态视觉。
5. sub-panel：单个 input（autofocus + Enter 触发确认）+「确认」「关闭」两键，与 cancel sub-panel 同模板。

## 不做

- 不做"每条任务独立 result" —— 那是逐条 dialog 路径已经覆盖的场景。bulk 的语义是"共享一句话"，与 cancel reason 共享相同。
- 不写后端 batch 接口 —— 现 `task_mark_done` 单条调用 + frontend loop 已够；同一段 result 在每条 description 末尾各自追加，符合现有数据形态。
- 不写测试 —— 纯 UI 串联 + 已有命令复用。

## 验收

- 选中若干 pending 任务 → 点 `✓ 标 done` → input + 确认 / 关闭出现。
- 留空 result + 确认 → 每条 description 仅追 `[done]`（与键盘 d 一致）。
- 填 result + 确认 → 每条 description 追 `[done] [result: <text>]`。
- 选中含已 done / cancelled → runBulk 把它们 skipped 计入文案。

## 完成

- [x] PanelTasks.tsx
- [x] TODO.md 移除该行
- [x] 移入 docs/done/
