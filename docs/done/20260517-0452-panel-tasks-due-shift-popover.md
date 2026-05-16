# PanelTasks 行内「📅 调期」按钮（iter #250）

## Background

owner 在 PanelTasks 里 hover / 展开一条 task 后想要"再推一下 due 时间"是高频
诉求 —— 原计划周二做完的事，发现还没做要往后挪 1 天 / 1 周。原有路径：
- bulk「改 due」需要先勾选 → 输入 datetime-local 字符串 → 提交（5 次点击 +
  键盘输入）
- 行内右键 / inline rename 没有调期入口

snooze chip（💤）虽然能"暂停到 N 时之后"，但语义是"先藏，到点后再现"，与
"我要把 due 截止时刻整体后移"不同。本迭代加 📅 调期 chip + 相对增量 preset
popover（+1h / +1d / +3d / +1w / +2w + 清除），点 chip → 选 preset → 一步搞定。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- **state**：在 `snoozePickerTitle` 旁加 `dueShiftPickerTitle: string | null`
  （null = 关）。同时只允许一条 task 处于打开态。

- **union close effect 扩展**：把 `dueShiftPickerTitle` 加入既有 outside-click
  + Esc 联合关闭的 useEffect（与 priority / status / ctxMenu / tagColor /
  snoozePicker 五位一起）。

- **chip + popover**：在 expanded `s.itemMeta` 区域的 ⏰ 还 N 分 chip 之后插
  「📅 调期」按钮：
  - **可见条件**：`!isFinished(t.status)` —— done / cancelled 终态行不显
    （调期对结束态无意义，与既有 snooze chip 同收敛规则）。
  - **popover 5 preset**：+1h / +1d / +3d / +1w / +2w。click → 计算 `new
    Date(Date.now() + deltaMs)` → 走既有 `formatDueInput` 拼 `YYYY-MM-DDThh:mm`
    （datetime-local 协议，与 `task_set_due` 后端 parse 同形）→ `invoke
    task_set_due` → reload。
  - **底部"清除 due"**：accent 色加粗，与 snooze popover「解除暂停」对偶。
  - **失败兜底**：`setActionErr` 写错误 + `finally setBusyTitle(null)` 防忙
    锁卡死。

## Key design decisions

- **走 `task_set_due` 而不是改 description 里的 `[deadline:]` marker**：
  - 实际 task data flow 里 PanelTasks 的 due 字段是 task header
    （`[task pri=N due=...]`）渲染出来的，与 butler_tasks 行 `[deadline: ...]`
    marker 不是同一回事。
  - 后端 `task_set_due` 已实现单字段原子修改 + 保留其它 markers，比前端 regex
    rewrite raw_description 更安全。
  - 原 TODO 措辞「改 [deadline:] marker」是写需求时的简化，落到实现层最妥当
    的 API 就是 `task_set_due`。

- **preset = 相对增量，不是绝对锚点**：snooze 已经覆盖了"今晚 / 明早 / 下周一"
  这种绝对锚点；这里走相对增量补全"再推 X 天 / 周"的频繁微调场景。owner 想要
  精确锚点时仍可走 bulk「改 due」走 datetime-local。

- **复用 snooze chip popover 模板**：popover 容器 / preset 按钮 hover / 底
  部 separator + accent 色"清除"按钮 全部从 snooze chip 复刻过来，让两条
  chip（💤 / 📅）的交互语言一致 — owner 看到 chip 就知道点击会弹 mini menu，
  Esc / 外点关闭。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.19s)
