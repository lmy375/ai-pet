# Pet — AI Desktop Companion · AI 桌面宠物

> **English** | [中文](#中文)

A Live2D desktop pet that lives on your screen and is backed by an LLM agent.
It can chat, see your screen, run shell commands, read and drive your macOS
apps, talk to you over Telegram, and act on its own via a scheduled heartbeat —
all while keeping a persistent long‑term memory of you.

Built with Tauri 2 + React 19 + TypeScript on the front end, and Rust on the
back end.

## Features

- **Live2D pet** — an animated character that floats on your desktop, auto‑hides
  to the screen edge, and can be pinned on top.
- **Chat in two windows** — the pet itself and a panel window share one
  conversation; talk to whichever is in front.
- **Any OpenAI‑compatible model** — point `api_base` at OpenAI, a local server,
  or a proxy (e.g. litellm). Streaming responses with a tool‑calling agent loop.
- **Vision** — paste images into the chat, or let the pet take a screenshot
  (the whole screen, or a single app's window) and look at it.
- **Tools** — `bash`, file read/write/edit, `screenshot`, `web_search`, and
  `spawn_subagent` for parallel background work; long commands run in the
  background and notify on completion.
- **Web search** — `web_search` looks things up on the live internet via
  [Tavily](https://tavily.com). Set `search_api_key` (Settings → Web Search) to
  enable it; without a key the tool isn't offered to the model.
- **macOS app control & reading** — via `osascript` the pet can drive scriptable
  apps (AppleScript) and GUI‑script the rest (System Events) — e.g. open
  Terminal and run a command, or read text straight from an app's UI. See
  [docs/macos-automation.md](docs/macos-automation.md).
- **Telegram bot** — chat with your pet from your phone, including sending and
  receiving images. See [docs/telegram.md](docs/telegram.md).
- **Scheduled heartbeat** — the pet wakes up in the background on an interval to
  run timed tasks or reach out proactively.
- **Persistent memory** — `SOUL.md` (persona), `USER.md` (about you), and
  `MEMORY.md` (its long-term memory) survive across conversations.
- **MCP support** — connect Model Context Protocol servers (stdio / SSE / HTTP)
  to extend the toolset.
- **Gallery mode** — swap the pet for a slideshow of a folder's images/videos.
- **Bilingual UI** — 中文 / English.

## Tech stack

Tauri 2 · React 19 · TypeScript · Vite · TailwindCSS 4 · PIXI.js 7 +
pixi-live2d-display · Rust (teloxide, reqwest, rmcp).

## Prerequisites

- [Node.js](https://nodejs.org/) + [pnpm](https://pnpm.io/)
- [Rust toolchain](https://rustup.rs/) and the
  [Tauri 2 system dependencies](https://v2.tauri.app/start/prerequisites/)
- A **Live2D model + Cubism SDK** of your own. These are copyrighted and **not**
  bundled — drop them into `public/models/` and `public/lib/` (both gitignored),
  then point `live_2d_model_path` at your model's `.model3.json`.

## Quick start

```bash
pnpm install
pnpm tauri dev          # run in development
pnpm tauri build        # build a release bundle
```

On first launch the pet window appears. Open the panel, go to **Settings**, and
fill in your API base, key, and model. Configuration is written to
`config.yaml` — see [docs/configuration.md](docs/configuration.md) for every
field and where the data lives.

## Documentation

- [Configuration reference](docs/configuration.md) — `config.yaml` fields and file locations
- [macOS automation](docs/macos-automation.md) — reading/controlling apps + permissions
- [Telegram bot](docs/telegram.md) — setup and image support

---

# 中文

> [English](#pet--ai-desktop-companion--ai-桌面宠物) | **中文**

一个住在你屏幕上的 Live2D 桌面宠物，背后是一个 LLM 智能体。它能聊天、看你的屏幕、
执行 shell 命令、读取并操作你的 macOS 应用、通过 Telegram 和你对话，还能靠定时心跳
自己主动行动 —— 同时对你保有一份跨对话的长期记忆。

前端 Tauri 2 + React 19 + TypeScript，后端 Rust。

## 功能

- **Live2D 宠物** —— 浮在桌面上的动画角色，会自动隐藏到屏幕边缘，可一键置顶。
- **双窗口聊天** —— 宠物本体和面板窗口共享同一段对话，哪个在前就跟哪个说。
- **任意 OpenAI 兼容模型** —— `api_base` 可指向 OpenAI、本地服务或代理（如 litellm）。
  流式输出 + 工具调用 agent 循环。
- **视觉** —— 可往聊天里粘贴图片，或让宠物截图（整屏，或某个 App 的单个窗口）来看。
- **工具** —— `bash`、文件读/写/改、`screenshot`、`web_search`、`spawn_subagent`
  （并行后台任务）；长命令转入后台，完成后自动通知。
- **联网搜索** —— `web_search` 通过 [Tavily](https://tavily.com) 搜索实时互联网。
  在「设置 → 联网搜索」填入 `search_api_key` 即启用；不填则不向模型提供该工具。
- **macOS 应用操作与读取** —— 通过 `osascript`，宠物能驱动可脚本化应用（AppleScript），
  对其余应用做 GUI 自动化（System Events）—— 比如打开 Terminal 跑命令、或直接读取
  App 界面上的文字。详见 [docs/macos-automation.md](docs/macos-automation.md)。
- **Telegram 机器人** —— 在手机上和宠物聊天，支持收发图片。
  详见 [docs/telegram.md](docs/telegram.md)。
- **定时心跳** —— 宠物按设定间隔在后台醒来，执行定时任务或主动找你。
- **长期记忆** —— `SOUL.md`（人设）、`USER.md`（关于你）、`MEMORY.md`（它的长期记忆）
  跨对话保留。
- **MCP 支持** —— 接入 Model Context Protocol 服务（stdio / SSE / HTTP）扩展工具集。
- **画廊模式** —— 把宠物换成某个文件夹的图片/视频幻灯片。
- **中英双语界面**。

## 技术栈

Tauri 2 · React 19 · TypeScript · Vite · TailwindCSS 4 · PIXI.js 7 +
pixi-live2d-display · Rust（teloxide、reqwest、rmcp）。

## 前置条件

- [Node.js](https://nodejs.org/) + [pnpm](https://pnpm.io/)
- [Rust 工具链](https://rustup.rs/) 及
  [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)
- 你自己的 **Live2D 模型 + Cubism SDK**。它们受版权保护、**不随仓库分发** —— 放进
  `public/models/` 和 `public/lib/`（均已 gitignore），再把 `live_2d_model_path`
  指向你模型的 `.model3.json`。

## 快速开始

```bash
pnpm install
pnpm tauri dev          # 开发运行
pnpm tauri build        # 打包发布版
```

首次启动会出现宠物窗口。打开面板进入 **设置**，填入你的 API base、key 和模型。
配置写入 `config.yaml` —— 每个字段和数据存放位置见
[docs/configuration.md](docs/configuration.md)。

## 文档

- [配置参考](docs/configuration.md) —— `config.yaml` 字段与文件位置
- [macOS 自动化](docs/macos-automation.md) —— 读取/操作应用与所需权限
- [Telegram 机器人](docs/telegram.md) —— 配置与图片支持
