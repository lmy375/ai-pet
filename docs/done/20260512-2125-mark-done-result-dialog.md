# PanelTasks 手动标 done 可选 result 摘要 dialog

## 需求

LLM 自动标 done 时常附 `[result: 38 个文件已归档]` 之类的产物摘要 —
回看任务历史时一眼能看到"做了什么"。但用户从面板手动标 done 走的
是 task_mark_done 命令，仅追加 `[done]` —— 产物信息只能后续手编辑
detail.md 补。给鼠标点击的"标 done"路径增加 dialog 让用户可选填
result，与 LLM 路径形态对齐；键盘 d 保留零摩擦快速通道。

## 实现

### 后端

`src-tauri/src/task_queue.rs`：

- 新 `append_done_marker_with_result(desc, result: Option<&str>)`：
  - result None / 空 / 仅空白 → 等同 `[done]`
  - 非空 trim 后 → `[done] [result: <trim>]`
- 旧 `append_done_marker` 保留作 shim 调用 new 函数（向后兼容；编译
  期 warn unused，无 runtime 影响）

`src-tauri/src/commands/task.rs`：

- `task_mark_done` 新 param `result: Option<String>`
- `task_mark_done_inner` 同步加 param，调
  `crate::task_queue::append_done_marker_with_result(..., result.as_deref())`
- LLM 自动标 done 走 memory_edit 直接写 description（不经此命令），
  互不干扰

### 前端

`src/components/panel/PanelTasks.tsx`：

- `handleMarkDone` 签名变 `(taskTitle, result?: string)`，invoke 时
  把 result 透出（undefined → `null`）
- 键盘 d 路径仍调 `handleMarkDone(title)` —— 零摩擦快路径不变
- 新增 `markDoneTitle` + `markDoneResult` state、`openMarkDoneDialog` /
  `closeMarkDoneDialog` / `confirmMarkDone` 三个 handler
- 鼠标点击触发的两条路径（status picker 子菜单 / 右键 ctx menu）从
  `void handleMarkDone(t.title)` 改 `openMarkDoneDialog(t.title)`
- 新 dialog modal（与 quickAdd 同 style 系列，居中圆角白底）：
  - 标题 `标记「TITLE」为已完成`
  - textarea + placeholder 示例 + label 说明语义
  - Enter（无 shift）提交，Esc 取消，backdrop click 取消
  - 两个 button "取消" + 绿色"✓ 确认"
  - 确认时空 result → 走 `[done]` 路径；非空 → `[done] [result: ...]`

## 验证

- `cargo check`：clean
- `npx tsc --noEmit`：clean
- 行为：
  - 键盘 d → 直接标 done，无 dialog；description 末尾加 `[done]`
  - 鼠标点 status 子菜单"✓ 标 done" → 浮 dialog
  - 鼠标右键 → 选"✓ 标 done" → 浮 dialog
  - 空 result + 确认 → description 加 `[done]`
  - 输入 result + 确认 → description 加 `[done] [result: <text>]`
  - Esc / backdrop click / 取消按钮 → 不改 description
  - 二次标 done（已 done 任务）→ 后端拒绝 + actionErr "task already finished"
  - LLM 自动标 done 写 `[done] [result: ...]` → PanelMemory ✅ chip 显
    （与 iter #184 路径互通）

## 不在本轮范围

- 没改键盘 d 路径同步弹 dialog：键盘用户的诉求是"快"，加 dialog 反而
  破坏 muscle memory；result 想填的人会用鼠标
- 没做 result 模板下拉（"已归档 N 个文件" / "已发送" / "已完成"）：
  freeform textarea 涵盖度更广；模板等用户反馈再加
- 没让 mark-done bulk batch（选多条统一标 done）：当前 bulk 操作走
  task_retry / 等其它路径，bulk mark done 是独立大任务

## TODO 池剩余

- PanelMemory butler_tasks item 描述里的「task title」ref token 渲 hover preview / 双击导航
