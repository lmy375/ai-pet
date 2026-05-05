# Pet — 桌面 AI 宠物管家

一个常驻桌面的实时陪伴型 AI 宠物。它既是会主动找你聊天的情绪伙伴，也是能动手帮你处理事务的「宠物管家」。

> 产品定位与边界详见 [`docs/GOAL.md`](docs/GOAL.md)；当前需求池见 [`docs/TODO.md`](docs/TODO.md)。

## 产品介绍

- **形象**：基于 Live2D 的透明无边框桌面窗口，永远悬浮在屏幕一角。
- **大脑**：兼容 OpenAI Chat Completions 协议的任意模型。
- **手脚**：通过内置工具与 MCP 协议连接本地能力（文件、Shell、日历、天气、记忆库等），可被 AI 自主调用以完成任务。
- **多入口**：桌面气泡 / 面板窗口，可选 Telegram Bot 转发，让你在手机上也能继续与宠物对话。

## 产品功能

### 1. 被动聊天 — 桌面随时对话
- 点击桌面宠物即可呼出聊天气泡，输入文字与之交流。
- 支持流式输出、口型同步、情绪驱动的动作切换。
- 面板窗口提供完整的对话历史、人格设定、记忆查看与设置入口。
- **会话列表元信息**：dropdown 每个会话标题旁附 "(N 条)"消息总数，跨会话切换前一眼分辨深会话 vs 空会话，省去"切进去才发现是新建空白"的来回。

### 2. 主动聊天 — 后台陪伴
- 后台长期运行的 **proactive 引擎**：根据你的活跃应用、空闲时长、专注模式、当前情绪与近期话题，决定何时、用什么语气主动开口。
- 内建多重「门控」（mute / 专注模式 / 冷却时间 / 截止时间紧迫度）避免打扰。
- **早安简报**：每日固定时刻（默认 8:30，可配置）自动开口，调用 weather / calendar / memory 工具把天气、日程、提醒和昨日回顾汇成一段短播报；尊重 mute 与专注模式，绕过普通发言冷却。
- 每一次主动发言的决策都记录在 **decision log** 中，可在调试面板复盘。
- **决策日志过滤 + 批量复制**：除按 kind 多选 / reason 子串过滤外，新增"近 10m / 30m / 1h"快捷时间窗（三层 AND 叠加，方便 debug 短时间内事件）；filter 行尾"📋 复制 N"把当前过滤后的决策按 `[ts] kind reason` 多行格式一键复制，贴 issue / 终端 grep 都友好。

### 3. 自我进化 — 情绪 / 记忆 / 技能
- **情绪系统**：宠物拥有持续演化的心情，会影响台词风格与外观动画。
- **记忆系统**：聊天与互动会沉淀为长期记忆，定期由后台 **consolidate 循环** 整理压缩。
- **陪伴感知**：累计陪伴天数、每日发言次数、情绪曲线等指标可在面板查看。
- **反馈学习**：对气泡的忽略、关闭、点赞会被记录并反馈到主动发言策略。
- **记忆搜索高亮 + 分类活跃度**：搜索结果里 keyword 在 title / description 黄底深棕字标出（与聊天 / 设置 / 任务搜索同款）；每个 memory category section 标题附"最近 X 天前更新"小字，让用户感知哪些区域在活跃迭代、哪些是死库存。

### 4. 宠物管家 — 通用任务执行
- 内置工具集：`file_tools`、`shell_tools`、`calendar_tool`、`weather_tool`、`memory_tools`、`system_tools`。
- 通过 **MCP（Model Context Protocol）** 接入外部工具服务器，扩展能力边界。
- 工具调用前可通过 **tool review** 机制人工审核高风险操作（基于 `tool_risk` 的分级）。
- 支持后台计划任务（butler schedule）、提醒、每日小结。
- **任务队列面板**：在「任务」标签页填标题 / 描述 / 优先级（0-9）/ 截止时间，宠物在 proactive 循环里按"过期 → 优先级 → 早到期 → 早创建"自动取单执行，结果通过 `[done]` / `[error: ...]` 标记回流到面板。
- **自然语言派单**：在「聊天」里直接说「帮我整理 Downloads」/「记得明天下午催报告」，宠物识别后弹出任务确认卡（含解析好的标题/描述/优先级/截止时间），点「创建任务」即入队，省去切到面板填表单的步骤。
- **长任务心跳**：被宠物动过手却停滞超过阈值（默认 30 分钟，可配置）的 pending 任务会在下次 proactive turn 里被点名，宠物必须写一句进展或标 done / error，避免任务静默淤积。TG 派出的任务停滞也会通过 bot 主动发"任务 X 卡 N 分钟了，要不要我点一下"，附 `/retry` `/cancel` 命令模板，让多端用户也能即时响应。
- **任务取消与重试**：失败任务一键「重试」（剥掉 error 标记回到 pending）；进行中的任务可一键「取消」并填原因（写入 `[cancelled: 原因]` 标记 + decision log），把"已完成"与"已取消"在面板上区分展示。
- **周报合成**：每周日 20:00 后由后台 consolidate 自动汇总本周的任务（管家事件计数 + 完成/取消列表）、对话（主动开口次数）、情绪（top 心情 motion）、陪伴（累计天数），写入 `ai_insights/weekly_summary_YYYY-Www`。确定性流水线，不依赖 LLM —— 即便 API 失效也按时落地。
- **工具风险设置**：在「设置」标签页可以为每个内置工具单独选「自动 / 总是审核 / 总是放行」，覆盖分类器的默认行为。`bash` / `write_file` 等高危工具默认要审核，但用户可改成"放行"批量自动化；只读工具默认放行，但洁癖型用户可改成"总是审核"上一道保险。
- **Telegram 派单**：在 TG 里直接说「帮我整理 Downloads」/「记得明天提醒我交报告」，宠物自动调 `task_create` 入队（无需面板确认卡）。任务执行完毕（成功 / 失败 / 取消）由后台 watcher 主动把结果回传到原 TG 会话，桌面与 TG 之间形成派单 → 执行 → 回传的闭环。
- **任务-记忆联动**：任务描述支持 `#tag` 标签和 `[result: 产物]` 标记。完成的任务在面板上独立显示「✓ 产物：…」一行；周报按 tag 聚合（`#organize × 3、#weekly × 1`）+ 完成清单带产物，让"本周往哪个主题投入最多"和"具体做了什么"一目了然。
- **任务复盘视图**：队列标题下显"今日完成 X · 近 7 天 Y" 完成率统计；每条任务卡片附"X 天前创建"相对时间，分辨"新积压 vs 老欠债"；showFinished 视图把 done/cancelled 按"今天 / 昨天 / 本周 / 更早"分组渲染，配合完成率形成立体复盘视图。
- **视觉占用控制**：单任务长描述（> 200 字）默认折叠到前 120 字 + "展开 (N 字)"按钮（搜索命中时强制展开避免高亮被遮蔽）；butler_tasks 的"最近执行"section > 5 条时显前 5 + "展开全部 N 条"按钮 — 长 session 下面板不再被冗长内容压扁。

### 5. 多端接入
- **桌面**：主窗口 + 面板窗口 + 调试窗口。
- **Telegram Bot**：在设置中填入 Bot Token 后，宠物的主动发言会同步到 TG，回复也会回流到桌面会话。

### 6. 深色 / 浅色主题
- 面板窗口右上角 ☀️/🌙 一键切换；偏好持久化到 `localStorage`，重启保留。
- 基于 CSS 变量的设计令牌系统（6 个 framework token + 6 对 section tint），主题切换不触发 React 重渲染。
- 全部主面板（聊天 / 任务 / 调试 / 记忆 / 设置 / 人格）已完成 dark 适配；状态色 / 错误色 / 成功色 / 高风险审核 / 心情 motion 等"语义信号"色跨主题保持一致，不被主题覆盖。

## 技术栈

| 层 | 技术 |
| --- | --- |
| 前端 | React 19 + TypeScript + Vite |
| 形象 | pixi.js 7 + pixi-live2d-display-lipsyncpatch |
| 后端 | Rust + Tauri 2（macos-private-api） |
| LLM | OpenAI 兼容协议（reqwest + 流式 SSE） |
| 工具 | 自研 tool registry + rmcp（MCP 客户端） |
| 通讯 | teloxide（Telegram Bot） |

后端模块概览见 [`src-tauri/src/lib.rs`](src-tauri/src/lib.rs)。

## 目录结构

```
.
├── src/                    # 前端 (React)
│   ├── App.tsx             # 主窗口（桌面宠物）
│   ├── PanelApp.tsx        # 面板窗口（聊天 / 设置 / 记忆 / 调试）
│   ├── DebugApp.tsx        # 调试窗口
│   ├── components/         # UI 组件（含 Live2D、气泡、面板）
│   └── hooks/              # 自定义 hooks
├── src-tauri/              # 后端 (Rust / Tauri)
│   └── src/
│       ├── commands/       # Tauri 命令（前端调用入口）
│       ├── proactive/      # 主动发言引擎
│       ├── tools/          # 内建工具集
│       ├── mcp/            # MCP 客户端
│       ├── telegram/       # Telegram Bot
│       ├── mood*.rs        # 情绪系统
│       └── …               # 记忆 / 反馈 / 决策日志 / 专注模式 / 输入空闲检测
├── docs/                   # 产品文档 (GOAL / TODO / 已完成迭代记录)
└── public/                 # Live2D 模型与静态资源
```

## 快速开始

### 环境要求
- Node.js 18+ 与 [pnpm](https://pnpm.io/)
- Rust 工具链（`rustup`）
- macOS（其它平台未做适配，依赖 `macos-private-api`）

### 安装与运行

```bash
pnpm install
cp .env.example .env        # 填入你的 OPENAI_API_KEY
pnpm tauri dev              # 开发模式启动
# 或重启已在运行的实例：
pnpm relaunch
```

### 配置

- **`.env`**：模型与 API 密钥
  ```
  OPENAI_API_KEY=sk-...
  OPENAI_BASE_URL=https://api.openai.com/v1
  OPENAI_MODEL=gpt-4o-mini
  ```
- **应用设置**：通过面板窗口的「设置」页编辑，运行时持久化到用户配置目录。
- **MCP 服务器 / Telegram Bot**：同样在设置面板中配置。

### 构建发布版

```bash
pnpm tauri build
```

产物位于 `src-tauri/target/release/bundle/`。