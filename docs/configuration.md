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
| `prompts/<名字>.md` | 改过的系统提示词（没改过就没有这个文件，跟随 App 内置文本） |
| `prompts/tools/<工具名>.md` | 改过的工具描述（同上，没改过就没有） |
| `~/.agents/skills/<技能名>/SKILL.md` | 技能手册（默认目录，所有 Agent 共享，见 [skills.md](skills.md)） |
| `sessions/` | 各会话历史（含每个 Agent 的 Telegram 会话 `telegram-<agent id>`） |
| `group/state.json` | 群聊转录与各成员私有上下文 |
| `logs/` | `app.log` / `llm.log` / 后台任务历史 `tasks.json` |
| `/tmp/pet/shell/` | bash 任务的 stdout/stderr |

打包出来的 app 和 `cargo build --release` 的 pet-cli 用 `pet/`；debug 构建
（`pnpm tauri dev`、`cargo run -p pet-cli`）自动改用 `pet-dev/`，所以开发时怎么折腾
都碰不到装好那份的配置、会话与记忆。想让 dev 沿用现有配置就拷一次：
`cp -R ~/Library/Application\ Support/pet{,-dev}`。

设 `PET_CONFIG_DIR` 可以整个换掉上表的根目录（`/tmp/...` 那行除外），且优先级高于
上面的 debug/release 之分。[评测](evals.md)靠它把每条用例跑在一次性目录里，碰不到
真实的配置与记忆。

## 全局池 + Agent 引用

配置分三块：全局的 `models`（模型池）和 `mcp_servers`（MCP 服务池），以及 `agents`
列表——每个 Agent 只按名字引用池里的条目，自己保留人设/记忆目录（`memory/<id>/`）、
Telegram 机器人和心跳计划。`active_agent` 指定当前应答桌面聊天的 Agent；聊天历史是
全局共享的，切换只改变「谁来回答」。

```yaml
models:                      # 模型池，key 就是切换器里显示的名字
  cobo-gpt:
    provider: openai
    api_base: https://litellm.1cobo.com
    api_key: sk-...
    model: gpt-5.6-sol-sub2api   # 真正发到 wire 上的模型 id
    context_window: 200000
    reasoning: medium
mcp_servers:                 # MCP 服务池，一台服务器一个进程，被引用的 Agent 共用
  fs:
    transport: stdio
    command: npx
    args: [-y, "@modelcontextprotocol/server-filesystem", /Users/you]
agents:
  - id: default
    name: 默认
    model: cobo-gpt          # 引用 models 里的名字
    mcp: [fs]                # 引用 mcp_servers 里的名字
```

同一个模型想要两种推理强度，就建两条（`gpt-fast` / `gpt-deep`）——上下文窗口和推理
强度属于模型，切模型时跟着一起切。在设置里改条目名会自动改掉引用它的 Agent。

### 模型池条目（`models.<名字>`）

| 字段 | 默认 | 说明 |
| --- | --- | --- |
| `provider` | `openai` | 线上协议：`openai`（chat-completions）/ `anthropic` / `gemini` / …，见 [provider.rs](../crates/pet-core/src/provider.rs)；网关托管的模型也要显式写，不按模型名猜 |
| `api_base` | `https://api.openai.com/v1/` | 端点，可填本地服务或代理（如 litellm） |
| `api_key` | 空 | API 密钥 |
| `model` | `gpt-4o-mini` | 模型名；视觉需用支持图像的模型 |
| `context_window` | `128000` | 上下文窗口大小（token），用于占用率显示 |
| `reasoning` | 空 | 推理强度：空 = 不传；关键字 `none`/`minimal`/`low`/`medium`/`high`/`xhigh`/`max`；或纯数字 = thinking token 预算（仅 Anthropic/Gemini 协议生效） |

### MCP 服务池条目（`mcp_servers.<名字>`）

| 字段 | 默认 | 说明 |
| --- | --- | --- |
| `transport` | `stdio` | `stdio`（本地进程）/ `sse` / `http` |
| `command` / `args` / `env` | 空 | stdio：可执行文件、参数、环境变量 |
| `url` / `headers` | 空 | sse / http：端点与自定义请求头 |
| `enabled` | `true` | 全局开关，关掉则所有 Agent 都不连它（设置里切换即时生效，不用重连） |

没有任何 Agent 引用、或被停用的服务器压根不会启动。

### 每个 Agent 的字段（`agents[]`）

| 字段 | 默认 | 说明 |
| --- | --- | --- |
| `id` | `default` | 稳定标识，也是记忆子目录名；创建后不要改 |
| `name` | `默认` | 显示名（切换器 / 群聊发言人） |
| `model` | 空 | 引用的模型池条目名；空或指向不存在的条目 ⇒ 该 Agent 无法对话（会直接报错，不会悄悄换个默认值） |
| `mcp` | `[]` | 引用的 MCP 服务名列表；指向不存在的条目会被忽略 |
| `telegram` | 关闭 | `bot_token` / `allowed_username` / `enabled`，见 [telegram.md](telegram.md) |
| `heartbeat_enabled` / `heartbeat_interval` | 关 / `60` | 定时心跳开关 / 间隔（分钟） |
| `heartbeat_context_turns` | `10` | 心跳携带的最近对话轮数 |

### 全局字段

| 字段 | 默认 | 说明 |
| --- | --- | --- |
| `active_agent` | `default` | 应答桌面聊天的 Agent id |
| `search_api_key` | 空 | [Tavily](https://tavily.com) API Key；填了才启用 `web_search`，所有 Agent 共享 |
| `skills_dir` | 空（= `~/.agents/skills`） | [技能](skills.md)目录，所有 Agent 共享；支持开头的 `~` |
| `live_2d_model_path` | `/models/miku/...` | 模型 `.model3.json` 路径（指向 `public/` 下你自己的模型） |
| `language` | `zh` | 界面语言：`zh` / `en` |
| `gallery_dir` / `gallery_enabled` / `gallery_interval` | — | 画廊幻灯片目录 / 开关 / 每张秒数 |
| `tools.disabled` | `[]` | 关掉的工具名列表，对所有 Agent 生效；见下 |
| `window` | — | 宠物窗口位置，随拖动自动写入，不在设置 UI 里 |

## 提示词与工具

面板「提示词 & 工具」。发给模型的两类文字都可以改：每轮的系统提示词，和每个工具的
`description`。

改之前 App 用内置文本，并随版本更新；一旦保存，就在 `prompts/` 下写出同名文件，之后
以你那份为准——所以「恢复默认」= 删掉那个文件。两边都是每轮重读，改完下一条消息就生效，
不用重启。

| 文件 | 内容 | 必须保留的变量 |
| --- | --- | --- |
| `prompts/persona.md` | 人设 + 长期记忆框架（聊天的第一条 system） | `{{soul}}` `{{user}}` `{{memory}}` |
| `prompts/tool_usage.md` | 工具使用指南（第二条 system） | `{{workdir}}` |
| `prompts/skills.md` | 技能清单的开头说明（清单本身是生成的） | — |
| `prompts/subagent.md` | 子代理的系统提示词 | — |
| `prompts/group.md` | 群聊礼仪 | — |
| `prompts/heartbeat.md` | 心跳说明 | `{{heartbeat}}` |

`{{变量}}` 由后端填：`persona` 还能用 `{{name}}` `{{memory_dir}}` `{{user_path}}`
`{{memory_path}}` `{{heartbeat_path}}`，`heartbeat` 还能用 `{{interval}}`
`{{heartbeat_path}}`。认不出来的 `{{xxx}}` 原样留着（方便在 LLM 日志里看出拼错了）；
少了上表里「必须保留」的那几个则直接拒绝保存——`{{memory}}` 掉了，宠物就再也看不到
MEMORY.md，而唯一的现象只是它变笨了。

哪些段落拼在一起、顺序如何，仍由代码决定；这里能改的是措辞。

```yaml
tools:
  disabled: [screenshot, web_search]   # 对所有 Agent 关掉
```

关掉的工具既不会出现在发给模型的工具列表里，模型从旧对话里翻出名字硬调也会得到
`unknown tool`。名字对 MCP 工具同样有效（面板里只列内置工具，MCP 按服务器开关；
要精确到单个 MCP 工具就在这里写名字）。

开关只能做减法：`chat` 只给心跳、`GroupChat` 只给群聊、`spawn_subagent` 不给子代理、
`web_search` 要有 Tavily Key——这些门在开关之前，配置里「打开」也不会绕过它们。

工具描述改的是 `prompts/tools/<工具名>.md`，只替换 `description`，参数结构不变。描述和
工具的实际行为对不上，模型就会用错——这是把它交给你的代价。

> GUI 内保存设置会广播 `settings-changed`，两个窗口热重载，无需重启。
> CLI 每轮重读配置；但 CLI 侧的修改（如切 Agent）不会通知已开启的 GUI 窗口。
