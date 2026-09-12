# 架构：一套核心，两套界面

Cargo workspace，三个成员：

```
crates/pet-core     引擎（无任何 UI 依赖）
src-tauri           桌面 GUI（Tauri 2 + React）
crates/pet-cli      终端 TUI（ratatui）
```

## pet-core

全部引擎逻辑都在这里：chat pipeline（流式 + 工具调用循环）、**聊天轮次的
运行与持久化（`turn::TurnRunner`）**、内置工具与 ToolRegistry、MCP 客户端管理、
会话/配置/记忆/[技能](skills.md)的磁盘读写、系统提示词组装、后台任务
（bash / 子代理 / 心跳）、多 Agent 群聊编排器。**pet-core 里不允许出现 `use tauri`。**

界面通过四个 trait 接入：

| trait | 作用 | GUI 实现 | CLI 实现 |
| --- | --- | --- | --- |
| `turn::TurnEvents` | 聊天轮次活动（开始 / 流事件 / 结束），全会话一条流 | `turn` 全局事件 → 各窗口按会话过滤 | channel → TUI 按会话过滤 |
| `chat::ChatEventSink` | 单 Agent 运行的流式事件（心跳、群聊 Agent 用；聊天轮次由 runner 自带的 sink 处理） | — | — |
| `group::GroupEvents` | 群聊活动（消息 / 各 Agent 流 / 完成） | `group-*` 全局事件 | channel → 群聊视图 |
| `tools::ChatHook` | 心跳 `chat` 工具的 UI 副作用（系统通知 + 刷新会话） | 通知插件 + `chat-inserted` | 无（CLI 不跑心跳） |

`shell::TaskNotifier`（后台任务完成）由 `TurnRunner` 自己实现：完成结果写进
发起该任务的会话并以一轮新对话续聊（会话忙则排队），不经过任何界面。

**轮次归后端所有。** 界面只做两件事：`send` 发起一轮，`attach` 随时接上正在
进行的一轮（回放已产生的事件）。所以切页、刷新、关窗都不会中断或丢失回复，
多个会话可以并行各跑一轮。`chat::ItemBuilder` 把流事件折成前端的 ChatItem
JSON，runner、群聊编排器共用；前端用同构的 reducer 渲染进行中的一轮，保证
落盘内容与实时渲染一致。

## 两套界面

- **src-tauri**：`#[tauri::command]` 薄包装 + 事件 emit。GUI 独有：双窗口管理、
  Live2D/画廊、Telegram、定时心跳、设置页。
- **pet-cli**：事件驱动 TUI。终端按键、流事件、群聊事件、任务完成统一进一个
  `AppEvent` channel，单循环消费；引擎调用全部在后台 task。详见 [cli.md](cli.md)。

两者共享磁盘状态（`config.yaml`、`sessions/`、`memory/`、`group/state.json`、
技能目录）；轮次都经由 `TurnRunner`（它在追加前从磁盘重载会话），可同时运行。

## 加新功能的规则

引擎能力进 pet-core，藏在上面的 trait 后面；界面层只做展示与输入。
若一个功能需要新的 UI 副作用，先在 pet-core 加 trait 方法（或新 trait），
再在两个界面各自实现。
