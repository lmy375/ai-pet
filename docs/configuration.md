# 配置参考

所有设置存于 `config.yaml`，可在面板「设置」里改，也可直接编辑文件。

## 文件位置（macOS）

配置目录为 `~/Library/Application Support/pet/`：

| 路径 | 内容 |
| --- | --- |
| `config.yaml` | 下方所有设置 |
| `memory/SOUL.md` | 宠物人设（人工编写，宠物只读） |
| `memory/USER.md` | 关于主人的事实与偏好（宠物维护） |
| `memory/MEMORY.md` | 宠物日记（宠物维护） |
| `memory/HEARTBEAT.md` | 定时任务清单（心跳读取/维护） |
| `sessions/` | 各会话历史（含 Telegram 的 `telegram-bot`） |
| `/tmp/pet/shell/` | bash 任务的 stdout/stderr |

## config.yaml 字段

| 字段 | 默认 | 说明 |
| --- | --- | --- |
| `live_2d_model_path` | `/models/miku/...` | 模型 `.model3.json` 路径（指向 `public/` 下你自己的模型） |
| `api_base` | `https://api.openai.com/v1` | OpenAI 兼容端点，可填本地服务或代理 |
| `api_key` | 空 | API 密钥 |
| `model` | `gpt-4o-mini` | 模型名；视觉需用支持图像的模型 |
| `language` | `zh` | 界面语言：`zh` / `en` |
| `mcp_servers` | `{}` | MCP 服务表（transport：`stdio` / `sse` / `http`） |
| `telegram` | 关闭 | `bot_token` / `allowed_username` / `enabled`，见 [telegram.md](telegram.md) |
| `gallery_dir` / `gallery_enabled` / `gallery_interval` | — | 画廊幻灯片目录 / 开关 / 每张秒数 |
| `heartbeat_enabled` / `heartbeat_interval` | 关闭 / 60 | 定时心跳开关 / 间隔（分钟） |
| `window` | — | 宠物窗口位置，随拖动自动写入，不在设置 UI 里 |

> 改动通过设置命令保存时会广播 `settings-changed`，两个窗口都会热重载，无需重启。
