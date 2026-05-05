# 任务 detail.md 编辑入口 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务 detail.md 编辑入口：当前任务详情页只读 detail.md；加一个「编辑」按钮让用户也能在面板里追加 / 改进度笔记，避免去翻 memories 目录。

## 目标

任务详情页的「进度笔记 (detail.md)」段当前是只读 pre 块。本轮加：
- 段落右上角加「编辑」按钮，点击 → 切换到 textarea 编辑模式
- 编辑模式底部「保存」/「取消」
- 保存调新 Tauri 命令 `task_save_detail(title, content)` → 落盘 → 刷新缓存
- 取消放弃改动回只读

让用户也能在面板里追加进度笔记（"完成了 X 一半"/"卡在 Y"），不必去 memories 目录翻文件。

## 非目标

- 不做 markdown 实时预览 —— 与既有 detail panel 的纯文本风格一致；
  详情段是 `whiteSpace: pre-wrap` 已能展示换行。
- 不做版本历史 / undo —— 用户想保留旧版自己复制；保存即覆盖。
- 不做并发冲突检测 —— LLM 也写 detail.md，若用户编辑期间 LLM 落盘，用户保存
  后会覆盖 LLM 那次写入。MVP 不解决；实战上发生概率低（编辑通常 < 1 分钟）。
- 不写 README —— 任务面板的内嵌补强。

## 设计

### 后端

`commands/task.rs` 新增：

```rust
#[tauri::command]
pub fn task_save_detail(title: String, content: String) -> Result<(), String>;
```

实现：
1. title 必填非空（trim 后）
2. `find_butler_task(title)` 查任务
3. `memory_edit("update", "butler_tasks", title, None, Some(content))`：
   description=None 保留 yaml 原描述；detail_content=Some(content) 覆盖 detail.md
4. content 不做长度上限 —— detail.md 是用户自管笔记，硬限不利

### 前端

`PanelTasks.tsx`：
- 新状态：
  - `editingDetailTitle: string | null` — 当前正在编辑哪条任务的 detail
  - `editingDetailContent: string` — textarea 内容
  - `savingDetail: boolean`
  - `editDetailErr: string`
- 视图模式（!editing）：detail.md pre 块右侧加「编辑」小按钮 → 切到编辑态
  并把当前 `detail.detail_md` 复制进 `editingDetailContent`
- 编辑模式：textarea (min-height 100px) + 「保存」+ 「取消」
- 保存：invoke task_save_detail → 成功后更新 detailMap（写入新 content）→ 退出
  编辑态。失败 → editDetailErr 红字
- 取消：editingDetailTitle = null（保留缓存中的旧 detail）
- 任意时刻只允许一条 detail 在编辑（与单 accordion 展开风格一致）—— 若 user
  切到另一条任务展开，编辑状态自动重置（折叠时清掉）

### 测试

后端 `task_save_detail` 校验路径：
- 空 title → Err（早退，不依赖 memory mock）

实际 IO 路径与现有 task_set_priority / task_set_due 同：复用 memory_edit，不为
集成测试 mock。

前端无测试基础设施。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `task_save_detail` Tauri 命令 + 校验单测 + 注册 |
| **M2** | PanelTasks 编辑状态 + 进度笔记段 UI 切换 + handler |
| **M3** | cargo test + pnpm build + cleanup |

## 复用清单

- `find_butler_task` / `memory_edit`（已有）
- `detailMap` 缓存（更新本地副本即可，不需 reload 整个 task list）
- `s.detailMdBox` / `s.bulkSubInput` 等现有样式

## 待用户裁定的开放问题

- 保存后自动刷整张 task_list 吗？本轮**否**——detail.md 改动不影响 description/
  due/priority/状态，task_list 视图不变；只更新本地 detailMap 即可。
- 编辑期间用户切到另一条任务展开 → 当前编辑丢失？本轮**对**——单 accordion
  语义下用户已"放弃"该条；做"未保存提示"会让交互复杂。

## 进度日志

- 2026-05-05 15:00 — 创建本文档；准备 M1。
- 2026-05-05 15:20 — 完成实现：
  - **M1**：`commands/task.rs` 加 `task_save_detail(title, content)` Tauri 命令：调 `memory_edit("update", "butler_tasks", title, None, Some(content))` —— `description=None` 保留 yaml 原描述（priority/due/markers 不动），`detail_content=Some(content)` 覆盖 detail.md。1 条新增单测覆盖空 title 早退。注册到 lib.rs。
  - **M2**：`PanelTasks.tsx` 加 4 个状态（editingDetailTitle / editingDetailContent / savingDetail / editDetailErr）+ 3 个 handler（enter / cancel / save）。`handleToggleExpand` 在切换 / 折叠时主动清编辑态，避免残留过时 textarea 内容。详情段「进度笔记」标题旁加「编辑」小按钮（编辑态时隐藏）；编辑态用 textarea + 「保存」/「取消」+ 错误红字。保存成功后只更新本地 detailMap 缓存（不 reload 整张 task_list —— detail.md 改动不影响 task 视图）。
  - **M3**：`cargo test --lib` 875/875（+1）；`pnpm tsc --noEmit` 干净；`pnpm build` 496 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 任务详情页的内嵌补强，与既有 R 系列任务面板迭代同性质。
  - **设计取舍**：单 accordion = 单编辑态（切换任务时编辑丢失，不做"未保存提示"避免增加交互复杂度）；保存只更新 detailMap 缓存而非 reload task_list（detail.md 与 task 视图正交，全量 reload 浪费 IO）；不做长度 / markdown 校验（detail.md 是用户自管笔记，硬规则不利）。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；后端校验有单测，前端 textarea + 保存路径是 detailMap 缓存更新的简单 state-machine，由 tsc 保证。
