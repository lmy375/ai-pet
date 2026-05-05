# 自然语言派单 — 开发计划

> 对应需求（来自 docs/TODO.md「已确认」）：
> 自然语言派单：识别聊天中「帮我…」请求，弹出确认卡后入队，省去手动建任务步骤。

## 目标

用户在 panel 「聊天」里说一句「帮我整理 Downloads」/「记得明天下午提醒我交报告」，宠物的回复中弹出一张**任务确认卡**（含解析出来的标题 / 描述 / 优先级 / 截止时间），点「创建任务」→ 自动 `task_create` 入队，省去用户切到「任务」标签页手填表单。

不是新写一套 NLU 流水线 —— 而是给 LLM 一个新工具 `propose_task`，让它在 reactive chat 里识别意图、解析参数，前端把工具结果渲染成卡片。这样：
- LLM 已有的语言理解 / 工具调用能力直接复用，无需训练分类器。
- 卡片始终带"用户确认"门 —— 误识别一次成本仅是用户多点一下「取消」。
- 与已有 `task_create` 命令对齐 —— 卡片内最终走的就是同一条入队路径。

## 非目标

- 不在桌面气泡（proactive）里显示卡片 —— 这一轮只接 panel 「聊天」。proactive 引擎里宠物已经会自己创建任务（通过 `memory_edit`），不需要确认。
- 不实现「自动入队不询问」 —— 与需求「弹出确认卡后入队」相悖。
- 不做参数二次编辑 —— v1 卡片字段渲染为只读，用户只有「创建 / 取消」两个出口；未来需要时再补内联编辑。

## 设计

### 工具：`propose_task`

新建 `src-tauri/src/tools/task_tool.rs`：

```rust
#[derive(Deserialize)]
struct ProposeTaskArgs {
    title: String,
    #[serde(default)] body: String,
    priority: u8,           // 0..=9
    #[serde(default)] due: Option<String>,  // YYYY-MM-DDThh:mm 或 None
}

// execute 体验：
// - 参数校验失败 → 返回 {"error": "..."}，LLM 看到后会重试或道歉
// - 校验通过 → 返回 {"proposed": true, "title": ..., "body": ..., "priority": ..., "due": ...}
// - **不写任何持久化数据** —— 仅是"提议"，由前端拦截渲染卡片
```

注册到 `ToolRegistry::new`。工具不在 `CACHEABLE_TOOLS` 里 —— 同样的提议在两轮里需要分别确认。

工具描述（LLM 视角）极其重要，必须明确：
- 何时调用：用户在自然语言里**让宠物做某件具体的事**且这件事适合放入任务队列（不是当下立即聊天回应）。例如「帮我整理 Downloads」/「记得明天 18:00 之前催 Alex 回复邮件」。
- 何时**不**调用：用户在闲聊 / 提问 / 抒情；用户让宠物**立即**做且能在当前轮内完成（如「现在帮我看看天气」）；用户已经在 panel 里手动建过同名任务。
- 调用后：LLM 仍需在自然语言回复里简短承接「好的，我把这个加到队列里 —— 看看面板确认下？」之类，让用户知道卡片是工具产物。

### 前端：`TaskProposalCard`

新建 `src/components/panel/TaskProposalCard.tsx`：
- 输入：解析后的提案对象
- 渲染：标题、正文、优先级徽章、截止时间，下方两个按钮 [创建任务] [取消]
- 「创建任务」点击 → invoke `task_create` → 成功后切换为「已加入队列 ✓」状态（按钮置灰）；失败显示错误
- 「取消」点击 → 卡片切换为「已忽略」状态（按钮置灰）

PanelChat 里在渲染聊天 items 时检测：若 item 是 tool 类型且 name === `propose_task` 且 result 含 `"proposed": true` → 渲染 `TaskProposalCard` 而不是 `ToolCallBlock`；解析失败回退到 `ToolCallBlock`（保持调试能见性）。

### Live 工具阶段（streaming）

工具 streaming 会先 fire `ToolStart`（`isRunning: true`），再 fire `ToolResult`。卡片在 `result` 还没回来前显示成「⏳ 正在解析…」骨架；`result` 一到立刻替换成完整卡片。这与 ToolCallBlock 的 isRunning 模式一致。

### 与现有 `propose_task` / 已建任务的关系

工具本身不与 butler_tasks / 任何持久化交互 —— 只是把"我打算建这条"信号通过工具结果通道传给前端。重复提议（用户错过卡片再说一次）也不会污染队列，因为只有「创建任务」按钮才走 `task_create`。

## 阶段划分

| 阶段 | 范围 | 状态 |
| --- | --- | --- |
| **M1** | `tools/task_tool.rs` 实现 + 注册 + 单测 | ✅ 完成（7 条 validate 单测） |
| **M2** | `TaskProposalCard.tsx` + PanelChat 渲染分支 | ✅ 完成 |
| **M3** | 联调 + 收尾（README / TODO / done/） | ✅ 完成 |

## 复用清单

- `task_queue::TASK_PRIORITY_MAX` —— 校验优先级范围
- `commands::task::task_create` —— 卡片确认时调用
- `tools::Tool` trait + `ToolRegistry::new` —— 注册新工具
- 前端 chat 流式事件（ToolStart / ToolResult）—— 已有事件通道直接用

## 待用户裁定的开放问题

1. **桌面气泡里要不要也支持卡片**？当前选「不」，气泡 UI 太窄；用户在气泡里说「帮我…」时宠物可以普通回应「我把它加到队列了，去面板看下」并直接走 memory_edit / task_create 自创建。后续看反馈再决定要不要补。
2. **卡片是否允许内联编辑参数**？当前选「不」，先验证基础流程；用户嫌优先级 / 截止时间不对可以「取消」后口语再说一次。
3. **如何防止 LLM 误识别**（用户只是闲聊却被弹卡片）？依赖工具 description 的 prompt 引导；卡片本身有「取消」逃生口；不引入二次校验环节。

## 进度日志

- 2026-05-04 14:00 — 创建本文档；准备进入 M1。
- 2026-05-04 14:30 — M1-M3 一次性合到 main：
  - **M1**：`src-tauri/src/tools/task_tool.rs` 落 `ProposeTaskTool`，pure `validate(args) -> Result<ProposalPayload, String>` + 异步包装。注册到 `ToolRegistry::new`。`cargo test --lib task_tool` 7/7（覆盖空 title / 越界 priority / 无效 due / 空字符串 due == None / 字段 trim）。`cargo test --lib` 全套 679/679。
  - **M2**：`src/components/panel/TaskProposalCard.tsx` 提供 5 态状态机（pending → creating → created/cancelled/error），按 `phase` 切换按钮组。PanelChat 在历史 items 与 live currentToolCalls 两处都加了 `tc.name === "propose_task"` 分支：解析成功 → 渲染卡；解析失败 → 回退 ToolCallBlock，保留调试能见性。`tsc --noEmit` 干净。
  - **M3**：README 加亮点（紧贴任务队列条目下方）；`docs/TODO.md` 移除条目；本文件移入 `docs/done/`。
- **开放问题答复**：
  - Q1 桌面气泡卡片：暂不做。气泡 UI 太窄，且气泡里的 reactive 路径目前不渲染工具结果。
  - Q2 内联编辑参数：暂不做。"取消 + 重说"是足够廉价的纠错手段，先验证基础流程。
  - Q3 防误识别：依赖工具 description 的 prompt 引导（已写明何时调 / 何时不调）+ 用户「取消」逃生口。后续如果误报率高再补"reject 后让 LLM 说明为什么 propose 是合理的" 二次校验。
