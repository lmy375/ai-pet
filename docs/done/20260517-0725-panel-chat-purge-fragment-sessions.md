# PanelChat「🧹 清碎片 session」按钮 + purge_fragment_sessions 后端（iter #264）

## Background

owner 多次 `/reset` 或频繁起新会话后，session 列表里积累一堆"只对话 1-2 条
就放弃"的碎片 session，让 session dropdown 变长，找有意义的会话也变难。当
前 PanelChat 只有「🗑 全清」按钮 —— 太重，需要保留 pinned / 长期会话。

本迭代加「🧹 清碎片」按钮：扫 session list 找 item_count ≤ 3 + 非 pinned +
非 active 的会话，armed 二次确认后一键清掉。owner 想要的"保留有内容会话
+ 清杂质" 一步到位。

## Changes

- `src-tauri/src/commands/session.rs`：新增 `purge_fragment_sessions` tauri
  命令：
  - 扫 `index.sessions`，找 `item_count <= 3 && !pinned && id != active_id`
    的条目
  - 逐条 `fs::remove_file(session_path(id))`；单条失败跳过不阻塞
  - 删完后 `retain` 把成功删除的从 index 移除 + `write_index`
  - 返回 `deleted: u32` 让前端显反馈

- `src-tauri/src/lib.rs`：注册 `purge_fragment_sessions` 命令。

- `src/components/panel/PanelChat.tsx`：
  - 新增 `purgeFragArmed: boolean` state + `purgeFragArmTimerRef` 5s 还原
  - 新增 `handlePurgeFragmentSessions` useCallback：先算 fragCount（前端预
    估同后端筛选规则）；为 0 时直接 toast 提示；否则 armed → 二次点 invoke
    + 刷 sessionList
  - 在 🗑 全清 按钮之前插「🧹 清碎片 (N)」按钮：fragCount = 0 时 disabled
    + 0.5 opacity；非 0 时显示"🧹 清碎片 (N)" / armed 时显示"⚠ 确认清 N？"
  - tooltip 三态文案（无碎片 / 待确认 / armed 红字提示）说明清理边界条件

## Key design decisions

- **门槛 item_count ≤ 3**：覆盖"碎片"定义 — 1 条 user 消息 + 1 条 assistant
  回复就是 2 条；加点 tool 调用也不超过 3。owner 真正的对话通常 5+ 条起步。
- **保留 pinned 与 active**：pinned 是 owner 显式钉过的"重要会话"即使 item
  少也要保留；active 删了 PanelChat 会失去显示锚点。这两条规则与既有
  `delete_session` 防护策略对齐（active 删除会自动切到 last session）。
- **前端预算 + 后端最终判定**：前端 `sessionList.filter(...)` 算出 fragCount
  仅用于 disable + 提示；后端 invoke 时重新读 index 做权威判断（避免
  race —— 期间 owner 在另一窗口 pin 了会话）。
- **armed 二次确认沿用既有 5s 模板**：与 🗑 全清 / 导入 sessions snapshot
  等其它"破坏性" 按钮保持一致的 5s 自动 disarm。

## Verification

- `cargo check` ✅
- `cargo test commands::session`（21 passed）✅
- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.29s)
