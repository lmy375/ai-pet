# DONE

记录每次迭代完成的实质性变化（按时间倒序）。

## 2026-05-01 — Iter 1：主动开口骨架
- 在 `AppSettings` 加入 `ProactiveConfig`（enabled / interval_seconds=300 / idle_threshold_seconds=900），默认关闭。
- 新增 `src-tauri/src/proactive.rs`：
  - `InteractionClock` 共享状态记录上次互动时间。
  - `spawn(AppHandle)` 后台 tokio 循环，每 tick 读 settings，若启用且 idle ≥ 阈值则触发主动检查。
  - 加载最新 session 历史 + SOUL，注入特殊 user 提示（`<silent>` 表示选择沉默）。
  - 复用 `run_chat_pipeline` + `CollectingSink` 调 LLM。非沉默回复持久化到 session，并通过 Tauri event `proactive-message` 推给前端。
- `chat` 命令在请求前后调用 `clock.touch()`。
- `useChat` 监听 `proactive-message` 事件，把 pet 主动消息加入 messages / items（后端已写盘，前端不再重复保存）。
- cargo check / tsc --noEmit 均通过（仅两条与本次无关的预存 warning）。

后续验证：开发期需打开 config.yaml 把 `proactive.enabled: true` 才会生效；面板 UI 留待 Iter 2+。
