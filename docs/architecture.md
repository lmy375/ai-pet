# 架构：一套核心，两套界面

Cargo workspace，三个成员：

```
crates/pet-core     引擎（无任何 UI 依赖）
src-tauri           桌面 GUI（Tauri 2 + React）
crates/pet-cli      终端 TUI（ratatui）
```

## pet-core

全部引擎逻辑都在这里：chat pipeline（流式 + 工具调用循环）、内置工具与
ToolRegistry、MCP 客户端管理、会话/配置/记忆的磁盘读写、系统提示词组装、
后台任务（bash / 子代理 / 心跳）、多 Agent 群聊编排器。**pet-core 里不允许
出现 `use tauri`。**

界面通过四个 trait 接入：

| trait | 作用 | GUI 实现 | CLI 实现 |
| --- | --- | --- | --- |
| `chat::ChatEventSink` | 单 Agent 运行的流式事件（chunk / reasoning / 工具 / usage） | Tauri Channel → 前端 | 终端渲染 + ItemBuilder |
| `shell::TaskNotifier` | 后台任务完成通知 | `background-finished` 事件 → 活动窗口 | channel → 事件循环自动续聊 |
| `group::GroupEvents` | 群聊活动（消息 / 各 Agent 流 / 完成） | `group-*` 全局事件 | channel → 群聊视图 |
| `tools::ChatHook` | 心跳 `chat` 工具的 UI 副作用（系统通知 + 刷新会话） | 通知插件 + `chat-inserted` | 无（CLI 不跑心跳） |

`chat::ItemBuilder` 把流事件折成前端的 ChatItem JSON，群聊编排器和 CLI 共用，
保证没有前端参与时落盘的会话与 GUI 渲染一致。

## 两套界面

- **src-tauri**：`#[tauri::command]` 薄包装 + 事件 emit。GUI 独有：双窗口管理、
  Live2D/画廊、Telegram、定时心跳、设置页。
- **pet-cli**：事件驱动 TUI。终端按键、流事件、群聊事件、任务完成统一进一个
  `AppEvent` channel，单循环消费；引擎调用全部在后台 task。详见 [cli.md](cli.md)。

两者共享磁盘状态（`config.yaml`、`sessions/`、`memory/`、`group/state.json`），
遵守同样的「发送前重载」规则，可同时运行。

## 加新功能的规则

引擎能力进 pet-core，藏在上面的 trait 后面；界面层只做展示与输入。
若一个功能需要新的 UI 副作用，先在 pet-core 加 trait 方法（或新 trait），
再在两个界面各自实现。
