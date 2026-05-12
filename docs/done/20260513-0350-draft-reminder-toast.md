# PanelChat session 切换草稿提示 toast

## 需求

PanelChat 已经把 prev session 的 input 在切换时立即写盘（防 3s debounce
未触发就丢稿），切回去时 draft 也会自动还原。但用户切走的瞬间没视
觉反馈 —— "我刚在 [session A] 写了 50 字想发，但切到 [session B] 想
查一句话，结果忘了回 A"。floating toast 5s 提示让这条信号显式。

## 实现

`src/components/panel/PanelChat.tsx`：

- 新 state `draftReminder: {sessionId, title, charCount} | null` + timer ref
- session-switch effect 内：writeDraft(prev) 后检查 `prevDraft.trim().length > 0`
  → 从 sessionList 找 prev session 标题（找不到时 "（未知会话）"）→ set
  draftReminder + 5s timer 自动清空
- 渲染 absolute top-center toast：accent border + 📝 emoji + "「title」
  有 N 字未发草稿 · 点此切回"
- onClick：清 toast + 调既有 loadSession(prevId)，自动 loadSession 路径
  会把 draft 填入 textarea —— 复用 iter session 切换的 draft 还原逻辑
- 仅在 prev session 与 new session 不同时触发（同 session 重新挂载不
  误触）
- maxWidth 85% + ellipsis 防长标题溢出

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - session A 中敲 "你好我想问..." → 切到 session B → 顶部浮 5s toast
    "「A title」有 11 字未发草稿 · 点此切回"
  - 点击 toast → 切回 A，textarea 回填刚才的内容
  - 5s 不点 → toast 自动消失（草稿仍在 storage 保留）
  - 切到 B 后立刻再切到 C → 新 toast 显 B 的草稿状态（覆盖 A 那条；不
    叠加）
  - 切走时 input 为空 / 仅空白 → 不浮 toast
  - 新建 session（prevId 空）→ 不浮（防首次 mount 误触）

## 不在本轮范围

- 没做"草稿列表"全局查看（显所有有未发草稿的 session）：toast 是即
  时反馈；全局列表是另一种 UX，本轮 scope 只覆盖切换瞬间
- 没让 toast 持续显示（不超时）：5s 是平衡"够看清"与"不阻挡屏"；用
  户回头看 session list 上未发草稿可用 session 自带的 item_count 或
  自定义 marker (future)
- 没 sync 到 pet 桌面气泡通知：场景轻，不值得跨 webview emit
- 没做"撤销切换"按钮（一键切回 + 跳过 toast）：直接点 toast 就是
  alias

## TODO 池剩余

- PanelSettings "📋 导出全部 settings 为 markdown" 按钮
- PanelPersona "重置 SOUL.md 为内置默认" 按钮
