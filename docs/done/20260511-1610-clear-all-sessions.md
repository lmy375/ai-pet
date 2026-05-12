# PanelChat 顶部全局清屏

## 需求

`/clear` 命令只清当前 session 的 messages / items（保留 session 文件）。用户
想"彻底重置聊天历史"得 dropdown 里每条逐个删 + 期间 dropdown 自动切到下一
条很烦。提供一个"全清"按钮一键删所有 session + 新建空会话。

## 实现

### 后端

`src-tauri/src/commands/session.rs` 新加 `clear_all_sessions() -> Result<u32, String>`：

- 遍历 sessions_dir 删所有 *.json（含 index.json）
- 计 deleted（含 index.json，实际"清掉了几个"覆盖 user 直觉）
- 调既有 `create_session()` 起一个新空 session，自动写 index.json + active_id
- 返回 deleted

不动 memory / SOUL.md / butler_history / config.yaml —— 仅聊天历史。这一点
在 tooltip + armed 提示文案里说清楚，避免用户误以为是 nuke 全数据。

### 前端

`src/components/panel/PanelChat.tsx`：

- 新 state `clearAllArmed: boolean` + armed timer ref
- `handleClearAllSessions` callback：armed 二次确认（5s 自动撤回），第一次
  显"⚠ 再点一次确认清空全部 N 个 session"，5s 内再点 → invoke +
  list_sessions + loadSession 新 active 切过去 + toast"已清空 N 个 session"
- snapshot 工具栏（导出 / 导入 + 清 orphan 那行）末尾加 🗑 全清按钮，
  marginLeft:auto 把它推到行末，与 export/import（迁移性）拉开语义距离
- armed 态变红填充，与既有的 reset / import armed 视觉一致

`src-tauri/src/lib.rs` 注册 `commands::session::clear_all_sessions`。

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - dropdown 顶部出现 🗑 全清按钮
  - 第一次点 → 红色 + 文案"⚠ 确认全清？" + toast 提示 N 个 session
  - 5s 内再点 → invoke 完成 → toast"已清空 N 个 session · 起了一个新空会话"
    + dropdown 显单条"新会话"
  - 5s 后不点 → armed 自动撤回，按钮回灰
  - memory / SOUL / config / butler_history 全不动（验证：tab 切去看仍在）

## 不在本轮范围

- 没做"export-then-clear"组合按钮：用户想保留备份的话先点导出快照再点全清；
  组合按钮反而隐藏一步重要操作
- 没做 dry-run 列出"将删的 session 标题"：armed 提示已显条数 + 红色 + 5s 撤
  回三层防护够；列详情会让面板抖动

## TODO 池剩余

- 设置页"重启 pet 窗口"按钮（最后一条）
