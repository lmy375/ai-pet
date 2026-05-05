# proactive prompt 调试器 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> proactive prompt 调试器：「调试」标签加按 turn 回放 prompt + 工具调用 + 回复的查看器，prompt 调优时不必去翻日志。

## 目标

调试窗口的「应用日志」标签已经有 "看上次 prompt" 按钮（Iter E1-E4）—— 弹一个
modal，prev/next 翻最近 5 个 turn 的 **prompt + reply**，显示工具**名字**列
表（`tools_used: Vec<String>`）。本轮把缺失的中间环节补上：每次 turn 的**全部
工具调用记录**（名字 + 参数 + 结果），按时间顺序在 prompt 与 reply 之间内嵌
展示，用户调 prompt 时一眼能看到"我让 LLM 看到这个 → 它叫了哪些工具看了什么
→ 最后说了什么"。

## 非目标

- 不做新的「调试」标签——重复现有 modal。复用 PanelDebug 的"看上次 prompt"
  入口与 prev/next 导航。
- 不做工具调用流式实时回放（一边跑一边看）——modal 是事后 inspect 视图，turn
  完成后 snapshot 即够；流式调试已有 LLM 日志页可看。
- 不做跨进程持久化 ring buffer（重启就丢）—— 与现有 `LAST_PROACTIVE_TURNS`
  保持一致，重启相当于 "重新开始 debug" 即可，加文件持久化是 over-engineering
  对 debug 场景。
- 不做工具调用的耗时 / latency 显示 —— LLM 日志页已含；本轮专注 in/out 内容。
- 不写 README —— 调试器内部能力补强，与既有 PanelDebug 迭代同性质。

## 设计

### 后端

1. **新增 `ToolCallEntry`**（在 `proactive/telemetry.rs`）：

   ```rust
   #[derive(Clone, Debug, serde::Serialize)]
   pub struct ToolCallEntry {
       pub name: String,
       pub arguments: String,  // 原始 LLM 给的 JSON 字符串
       pub result: String,     // 工具返回的字符串（已 redact 由 record 路径外层处理）
   }
   ```

2. **扩展 `TurnRecord`** 加 `pub tool_calls: Vec<ToolCallEntry>`。在
   `LAST_PROACTIVE_TURNS` 推入时一并填。`#[serde(default)]` 让旧的（不存在该
   字段的）测试 / 序列化兼容。

3. **扩展 `ToolContext`** 加 `pub tool_calls: Option<Arc<Mutex<Vec<ToolCallEntry>>>>`
   + builder method `with_tool_calls_collector(arc)`。语义与现有 `tools_used`
   完全对称（一个收 names，一个收 full records）。

4. **在 chat 流水线推 entry**：`commands/chat.rs` 工具结果计算完后（line ~1132
   附近，紧挨 `record_tool_call` 之后），若 `ctx.tool_calls.is_some()` 就 push
   `ToolCallEntry { name, args, result }`。这里 `args` 用 `tc_args.to_string()`
   即可（已是 JSON string），`result` 是工具返回的字符串。

5. **在 `run_proactive_turn` 收集**：
   ```rust
   let tool_calls: Arc<Mutex<Vec<ToolCallEntry>>> = ...;
   let ctx = ... .with_tool_calls_collector(tool_calls.clone());
   // 跑完后
   let collected = tool_calls.lock().map(|g| g.clone()).unwrap_or_default();
   ```
   把 `collected` 塞进 `TurnRecord.tool_calls`（在已有 `tools_used` 旁推入的
   位置同步）。

### 前端

`PanelDebug.tsx` 的 modal 已在 prompt（pre 块）和 reply（pre 块）之间。本轮
**插入一段"⇢ TOOL CALLS"** 在两者中间：

- 标题行 "⇢ TOOL CALLS (N 个)"，N=0 时整段不渲染（保持 modal 整洁）
- 每个调用一个折叠条目：
  - 默认折叠展示 `🔧 <name>`
  - 点击展开 → 两个 pre 子块：`arguments` (JSON pretty-print 失败时原样字符串)
    + `result` (原样字符串，长度过长可滚动)
  - 颜色：args 浅蓝背景 (`#eff6ff`)，result 浅绿背景 (`#f0fdf4`)
- 多个调用按 push 顺序展示（即 LLM 调用顺序）

复用现有 modal scrollable container 与样式风格。

`TurnRecord` 类型补 `tool_calls?: ToolCallEntry[]`（前端 type）；旧 ring-buffer
项缺该字段时按空数组处理（向前兼容）。

### 测试

后端：
- 不易直接单测 chat 流水线收集逻辑（需要 mock ToolRegistry + 一系列状态）。
  改测 `ToolCallEntry` 的 serde round-trip + `TurnRecord` 含 tool_calls 字段
  的 JSON 形态（小测就够）。
- `r33_tests` 现有测用的 helper `turn(outcome)` 要在 TurnRecord 加新字段后
  补上 `tool_calls: vec![]`，保证不破现有 11 测。

前端无测试基础设施，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `ToolCallEntry` 结构 + `ToolContext` collector 字段 + builder method |
| **M2** | chat.rs 在 tool 结果处推 entry；`TurnRecord.tool_calls` 字段 + 现有测试 backfill |
| **M3** | `run_proactive_turn` allocate Arc + 扩 TurnRecord 推入 |
| **M4** | PanelDebug modal 加 "TOOL CALLS" 段（折叠条目）+ `TurnRecord` 前端 type |
| **M5** | `cargo test` + `pnpm build` + TODO 清理 + done/ |

## 复用清单

- `proactive/telemetry.rs::TurnRecord` 现有结构 / `LAST_PROACTIVE_TURNS` 静态
- `commands/chat.rs` tool 执行循环现有位置
- `tools/context.rs::ToolContext` 现有 builder pattern
- PanelDebug modal 与 prev/next 导航布局

## 待用户裁定的开放问题

- result 字段是否做 redaction？目前 chat 流水线在 `record_tool_call` 之前
  result 已是工具原始输出；如果工具内部已用 redact_with_settings 已经做过，
  这里直接塞即可。**本轮不再 redact**，与 LLM 日志页处理一致（panel 用户 == 
  机主，看到原始内容是预期）。
- arguments 长度截断？通常 LLM 给的 JSON args 不会太长（<2KB），暂不截断；
  modal 区有滚动，长内容自然能看完。
- 多个工具结果是否折叠所有？默认折叠（节省纵向空间），点开看。

## 进度日志

- 2026-05-05 07:00 — 创建本文档；准备 M1。
- 2026-05-05 07:30 — 完成实现：
  - **M1**：`proactive/telemetry.rs` 加 `ToolCallEntry { name, arguments, result }` 结构（pub Serialize）+ `TurnRecord` 加 `tool_calls: Vec<ToolCallEntry>` 字段（`#[serde(default)]` 向前兼容）。`tools/context.rs` 加 `tool_calls: Option<Arc<Mutex<Vec<ToolCallEntry>>>>` + builder method `with_tool_calls_collector`，与现有 `tools_used` 完全对称。
  - **M2**：`commands/chat.rs` 在 tool 结果计算完后（`record_tool_call` 之后），若 `ctx.tool_calls.is_some()` 推一条 `ToolCallEntry`。其它路径（reactive chat / telegram / consolidate）不开 collector → 零开销。
  - **M3**：`run_proactive_turn` allocate `Arc<Mutex<Vec<ToolCallEntry>>>` + 走 builder 注入，pipeline 跑完后从 collector 拿出推入 `LAST_PROACTIVE_TURNS` 的 TurnRecord 里。
  - **M4**：`PanelDebug.tsx` 的 `recentTurns` type 加可选 `tool_calls`，老 ring-buffer 项缺该字段时空数组兜底。modal 在 PROMPT 与 REPLY 之间内嵌 "🔧 TOOL CALLS (N 个)" 段：每条调用一个折叠条目（默认折叠展示 `🔧 #N <name> <args 截断到 60 char>`，点开展示 args / result 两 pre 块）。args / result 用新加的 `prettyPrintIfJson` helper 做 JSON 美化（解析失败保留原样）。turn 切换时 `expandedToolCallIdx` 重置避免错位。
  - **M5**：`cargo test --lib` 853/853（+3 telemetry round-trip）通过；`pnpm tsc --noEmit` 干净；`pnpm build` 494 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 「调试」窗口的内嵌增强，与既有 E1-E4 / R25 系列同性质。
  - **设计取舍**：collector 用 `Arc<Mutex<Vec<...>>>` 与已有 `tools_used` 完全对称（相同的"opt-in 路径才开"模式，零 cost 默认），避免引入新通信通道；JSON pretty-print 做 lazy 路径（解析成功才美化），保证非 JSON 工具结果（如纯文本）不被破坏；折叠默认收起 + 展开时 args/result 两段双色（蓝/绿）区分输入输出。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；后端 round-trip 序列化有单测，前端 modal 状态机由 tsc + 推演保证。
