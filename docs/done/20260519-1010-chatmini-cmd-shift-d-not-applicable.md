# ChatMini「⌘⇧D 删除最后一条 user」shortcut — 范围外 pivot drop（iter #554）

## Discovery

TODO 提案：「ChatMini 「⌘⇧D 删除最后一条 user」keyboard shortcut —
误发 prompt 后快速撤回（与 ⌘R reroll 对偶）」。

实际实现需触发：

1. **新 Tauri 命令**：current session.jsonl mutation — 删 last user msg
   + 可能联动删随后的 assistant reply（不删则 chat 视图错位 — 一条
   reply 凭空对应啥？）
2. **save_session 调用编排**：load current session → mutate messages
   array → save_session
3. **UI refresh**：messages prop 重新拿（既有 useChat hook 可能 refetch）
4. **错误回滚**：保 mutation atomic（save 失败时不丢 state）

但既有 `commands::session::save_session` 接收整 Session struct，没
「pop last N messages」专用 API；要做需 frontend orchestration（load
→ mutate → save）— 跨多层 + 风险（save 失败丢数据 / 与 streaming
race / context 缓存与 disk 不一致）。

更根本问题：**owner 的「撤回」实际语义不清**：
- 仅删 user message？— assistant reply 失去 context，对话视图错位
- 删 user + assistant pair？— 数据丢更多；且 LLM tokens 已花
- 撤销整条 session 回该 message 前的 snapshot？— 需 message-level
  snapshot 持久化

## Existing alternative

owner 已有更粗粒度撤回路径：

- **`/reset` TG 命令**：清掉整个 LLM chat context（保人设）— 「我们
  忘了刚才那段对话，从头开始」效果。比单条 delete 暴力但 atomic /
  无 race
- **`↺ 重发本条` ctx menu + ⌘R reroll**：误发后让 pet 重新回复（不删
  原 user message，但 reply 更新）

owner 真要「删掉某条 user message」可经桌面端 PanelChat（如果支持）—
或 telegram block / edit message。⌘⇧D 单 shortcut 太隐含、容易误触
也容易误删。

## Decision

**不实现 ⌘⇧D 删除最后 user shortcut**。两条理由：

1. 跨层实现复杂（new backend pop / 编排 save / refresh / 回滚）+ 边界
   case 多（partial pair / streaming race）— 单 iter 范围外
2. 既有 `/reset` 已覆盖「撤回 prompt」实用场景；细粒度删除单条 message
   是 nice-to-have 而非 blocker

procedure 教训：propose 频繁出现 keyboard shortcut 提案时，应预先 grep
确认目标 mutation 的 backend API 是否就位 — 表面看小（一个 keystroke
event）实际触发的 backend 改动可能 disproportionate。

## Future iters (out of scope)

- **proper session-pop API**：新 Tauri 命令 `session_pop_last_pair(session_id)`
  原子删 last (user, assistant) 双条 — 解决数据一致性
- **per-message snapshot rollback**：session.jsonl 改 append-only 加 ops，
  支持「revert to before msg N」语义；overkill 对 pet 场景
- **frontend display-mode undo**：ChatMini 隐去 last bubble pair（CSS
  fade，不真删 disk）+ confirm 后再写 — 中间态欺骗性，否决
