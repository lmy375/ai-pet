# 任务批量改 due — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务批量改 due：批量操作工具栏里加「改 due」按钮，统一调整选中任务的截止时间，对应"全部推迟一天 / 集中到下周末"等场景。

## 目标

「任务」面板批量工具栏现有：重试 / 取消 / 改优先级。本轮加「改 due」第四个
动作 —— 用户多选任务后填一个新 due（datetime-local），统一覆盖；填空则清掉
所有选中任务的 due（让它们变成"无截止"任务）。覆盖典型场景：
- 周一立项一批任务，周五全部推迟到下周
- 一批小事原本散在一周里，决定集中到周末做

## 非目标

- 不做相对时间（"全部推 1 天"）—— 解析自然语言成本不值；用户在 panel 里直接
  输绝对时间更可控。
- 不做"加 N 小时"等增量调整 —— 同上。
- 不做后端 bulk RPC —— 与改优先级同模式：循环 single-op，N 通常 < 10。
- 不写 README —— 任务管理增量补强。

## 设计

### 后端

`commands/task.rs` 加 Tauri 命令 `task_set_due(title, due)` 与 `task_set_priority`
对偶：

```rust
#[tauri::command]
pub fn task_set_due(title: String, due: Option<String>) -> Result<(), String>;
```

行为：
- title 必填非空（trim 后）；失败 → Err("title is required")
- `due == None` 或 `due.trim().is_empty()` → 清掉 due 字段（写出 `[task pri=N]`
  无 due 形式）
- 否则 parse `YYYY-MM-DDThh:mm`，失败 → Err
- 找到任务 → parse_task_header → 替换 due → format → memory_edit("update", ...)
- legacy 无 header 任务：构造 header `{ priority: 0, due, body: trim(desc) }`
  （与 `task_set_priority` 兼容路径一致，以保留所有 markers 在 body 里）

不推 decision_log（与 priority 同：日常 UX 调整，非状态转移）。

### 前端

`PanelTasks.tsx`：
- `bulkAction` 状态当前是 `"cancel" | "priority" | null`，扩成
  `"cancel" | "priority" | "due" | null`
- 新增 `bulkDue` 状态（datetime-local 字符串，可空）
- toolbar 加按钮"改 due"，与"改优先级"对称：active 时切到 due sub-panel
- sub-panel：`<input type="datetime-local" />` + "确认" / "关闭" + 一个"清空 due"
  辅助按钮（点了确认 == 把所有选中任务清 due）
- handler `handleBulkSetDueConfirm` 走通用 `runBulk`（label="改 due"，predicate
  全 true，op = invoke `task_set_due`）

### 测试

后端 `task_set_due` 校验路径：
- 空 title → Err
- due 无效格式 → Err
- 与 `task_set_priority` 同等的"涉 IO 路径不写集成测试"原则

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `task_set_due` Tauri 命令 + 校验单测 + 注册 lib.rs |
| **M2** | PanelTasks 状态扩 + toolbar 按钮 + sub-panel + handler |
| **M3** | cargo test + pnpm build + cleanup |

## 复用清单

- `task_queue::{parse_task_header, format_task_description, TaskHeader}`
- `commands::task::find_butler_task`
- `runBulk` 通用 helper

## 待用户裁定的开放问题

- "清空 due" 是单独按钮 vs 直接确认空输入？本轮选**直接确认空输入**——多按一
  个按钮反而冗余；输入框默认值已经是空，第一次进入即可"清空"。
- due 在终态任务（done / cancelled）上是否允许改？本轮**允许**——与改优先级
  同：终态下 due 只影响展示，无害。

## 进度日志

- 2026-05-05 14:00 — 创建本文档；准备 M1。
- 2026-05-05 14:20 — 完成实现：
  - **M1**：`commands/task.rs` 加 `task_set_due(title, due: Option<String>)` Tauri 命令，复用现有 `parse_task_header` / `format_task_description` / `find_butler_task`：parse 失败的 legacy 任务自动 promote 为带 header 形式；空 / null due 写出无 `due=` header（清空场景）；非空 due 严格 `YYYY-MM-DDThh:mm` 解析失败 → Err。注册到 lib.rs。新增 2 条单测覆盖空 title / 无效格式早退。
  - **M2**：`PanelTasks.tsx` `bulkAction` 类型扩 `"due"` 第三种；新增 `bulkDue` 状态（datetime-local 字符串）+ `handleBulkSetDueConfirm`（trim 空 → null 走清空路径），走通用 `runBulk` helper（label 根据是否清空动态切换"改 due" / "清空 due"）。toolbar 加「改 due」按钮（与「改优先级」对称的 active 样式），sub-panel 含 datetime-local 输入 + 智能确认按钮（空输入按钮文案变"确认（清空 due）"提示行为）。
  - **M3**：`cargo test --lib` 874/874（+2）；`pnpm tsc --noEmit` 干净；`pnpm build` 496 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 任务批量操作的第四个动作，与既有 R 系列任务面板迭代同性质。
  - **设计取舍**：留空 = 清 due（一个按钮覆盖两种语义）vs 单独"清空 due"按钮 —— 选前者，UI 更紧凑且按钮文案随输入态切换提示用户实际行为；终态任务也允许改 due（与改 priority 同：仅影响展示，无害）；后端复用 datetime-local 协议字符串而非 ISO with timezone（与 task_create 同输入格式，前后端协议一致）。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；后端校验有单测，前端是 `runBulk` 模板 + 既有 sub-panel 样式的复制，由 tsc + 既有模式保证。
