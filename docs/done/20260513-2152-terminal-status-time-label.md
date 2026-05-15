# 终态任务时间标签语义化（完成于 / 取消于 vs 更新于）

## 背景

PanelTasks 任务卡 meta 行显示 "创建于 X · Y 前" 和 "更新于 X · Y 前"。对 pending / error 状态的任务，"更新于"准确（任意时刻可能被编辑）。但对终态：
- done: updated_at = 标 done 时刻 → 用"完成于"更准
- cancelled: updated_at = 标 cancelled 时刻 → 用"取消于"更准

之前一律标"更新于"，要求用户脑补"哦这是它完成的时间"。多读一遍 status badge 才能确认。Tiny win，但出现在每条终态任务上，扫读时累积。

## 改动

`src/components/panel/PanelTasks.tsx`：

任务卡 meta 行的「更新于 X」span 按 status 分支：
- `t.status === "done"` → "完成于 "
- `t.status === "cancelled"` → "取消于 "
- 其它（pending / error）→ "更新于 "（保留原文案）

附文 "Y 前 · N 次更新" 不动 —— 它对所有 status 都有效。

## 不做

- 不动 created_at 行（创建动作语义对所有状态相同）
- 不动 error 行的标签（error 任务可重试 → updated_at 仍是动态的，"更新于"正确）
- 不动 history timeline / 详情页 / 归档区的 ts 渲染（独立模块）

## 验收

- `npx tsc --noEmit` ✅
- 切「任务」tab 看一条已完成任务：meta 行写"完成于 2026-05-12 ..."；取消的任务写"取消于 ..."
- pending / error 仍写"更新于 ..."

## 完成

- [x] 单行 if/else if/else 分支
- [x] 移到 docs/done/
