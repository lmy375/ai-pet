# 配置参考

所有设置存于 `config.yaml`，可在面板「设置」里改，也可直接编辑文件
（保存时校验 YAML）。桌面 GUI 和 [pet-cli](cli.md) 读同一份配置。

## 文件位置（macOS）

配置目录为 `~/Library/Application Support/pet/`：

| 路径 | 内容 |
| --- | --- |
| `config.yaml` | 下方所有设置 |
| `memory/<agent id>/SOUL.md` | 该 Agent 的人设（人工编写，宠物只读） |
| `memory/<agent id>/USER.md` | 关于主人的事实与偏好（宠物维护） |
| `memory/<agent id>/MEMORY.md` | 长期记忆（宠物维护，只记有价值的，非流水账日记） |
| `memory/<agent id>/HEARTBEAT.md` | 定时任务清单（心跳读取/维护） |
| `sessions/` | 各会话历史（含每个 Agent 的 Telegram 会话 `telegram-<agent id>`） |
| `group/state.json` | 群聊转录与各成员私有上下文 |
| `~/.config/pet/logs/` | `app.log` / `llm.log` / 后台任务历史 |
| `/tmp/pet/shell/` | bash 任务的 stdout/stderr |

## 多 Agent

配置的核心是 `agents` 列表——每个 Agent 有自己的模型、人设/记忆目录
（`memory/<id>/`）、MCP 工具集、Telegram 机器人和心跳计划。`active_agent`
指定当前应答桌面聊天的 Agent；聊天历史是全局共享的，切换只改变「谁来回答」。

### 每个 Agent 的字段（`agents[]`）

| 字段 | 默认 | 说明 |
| --- | --- | --- |
| `id` | `default` | 稳定标识，也是记忆子目录名；创建后不要改 |
| `name` | `默认` | 显示名（切换器 / 群聊发言人） |
| `api_base` | `https://api.openai.com/v1` | OpenAI 兼容端点，可填本地服务或代理（如 litellm） |
| `api_key` | 空 | API 密钥 |
| `model` | `gpt-4o-mini` | 模型名；视觉需用支持图像的模型 |
| `context_window` | `128000` | 上下文窗口大小（token），用于占用率显示 |
| `reasoning_effort` | 空 | OpenAI 系推理控制（`minimal`/`low`/`medium`/`high`；空 = 不传） |
| `thinking_enabled` / `thinking_budget_tokens` | 关 / `1024` | Anthropic 扩展思考开关与预算 |
| `mcp_servers` | `{}` | MCP 服务表（transport：`stdio` / `sse` / `http`） |
| `telegram` | 关闭 | `bot_token` / `allowed_username` / `enabled`，见 [telegram.md](telegram.md) |
| `heartbeat_enabled` / `heartbeat_interval` | 关 / `60` | 定时心跳开关 / 间隔（分钟） |
| `heartbeat_context_turns` | `10` | 心跳携带的最近对话轮数 |

### 全局字段

| 字段 | 默认 | 说明 |
| --- | --- | --- |
| `active_agent` | `default` | 应答桌面聊天的 Agent id |
| `search_api_key` | 空 | [Tavily](https://tavily.com) API Key；填了才启用 `web_search`，所有 Agent 共享 |
| `live_2d_model_path` | `/models/miku/...` | 模型 `.model3.json` 路径（指向 `public/` 下你自己的模型） |
| `language` | `zh` | 界面语言：`zh` / `en` |
| `gallery_dir` / `gallery_enabled` / `gallery_interval` | — | 画廊幻灯片目录 / 开关 / 每张秒数 |
| `window` | — | 宠物窗口位置，随拖动自动写入，不在设置 UI 里 |

> GUI 内保存设置会广播 `settings-changed`，两个窗口热重载，无需重启。
> CLI 每轮重读配置；但 CLI 侧的修改（如切 Agent）不会通知已开启的 GUI 窗口。
