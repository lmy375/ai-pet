# PanelChat token badge 点击压缩历史

## 需求

session bar 上的 token 估算 badge 在 ≥ TOKEN_WARN (8k) 时变黄、≥ TOKEN_CRIT
(24k) 时变红，但只是被动显示。用户想"立即抢救 context" 还得手动开新 session。
让 badge 在 warn 阈值以上变成可点入口，弹三档"压缩历史"选项即时生效。

## 实现

`src/components/panel/PanelChat.tsx`：

- 新 state `compactPromptOpen: boolean` + outside-click / Esc 关闭 effect
  （setTimeout(0) 挂 mousedown 防同次 click 即开即关）
- 新 `compactHistory(lastKeep)` useCallback（放在 `saveCurrentSession` 之
  后避免 TS2448）：
  - trimCount = items.length - lastKeep；≤ 0 直接关 popover
  - items 直接 slice(trimCount)
  - messagesRef：findIndex system message → 抽出来 + 剩余消息保留尾部
    lastKeep 条（tool 调用对可能截断，接受不一致换 token 削减）
  - setItems + saveCurrentSession 即时落盘
  - 反馈 exportToast 4s "已压缩 N 条早期消息，保留近 M 条"
- token badge 改造：
  - `interactive = sessionTokensEstimate >= TOKEN_WARN`
  - interactive 时 cursor pointer + click 切换 popover
  - 浮窗（absolute top:100%+6 / left:0）渲三档：
    - 保留近 1/2（drop floor(total/2)，至少留 4）
    - 保留近 1/3
    - 仅保留最近 4 条
  - 每条按钮带"丢 X 条 / 保留 Y 条"副标
  - 底部 ⚠ 提示"不可撤销 · session 文件原地覆盖；想保留备份先用 📦 导出"

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 短会话（< 8k token）→ badge cursor:default，点击无反应
  - 长会话（≥ 8k）→ badge 变黄 cursor:pointer，hover tooltip 提示"点击压缩"
  - 点 badge → 浮窗显三档，每条带"丢 / 保留"具体数字
  - 点"保留近 1/2"→ 立即 trim items + messagesRef + 落盘 + 浮窗关 + toast
  - 下次 sendMessage → LLM 看到的 messages 只含 system + 保留的尾段；
    token 数即时跟着 badge 下降
  - 外部 click / Esc 关浮窗，无 trim
  - 全部 items < 4 → 三档按钮 drop = 0 → disabled
  - SOUL prompt 保留：system message 永远不被 trim 掉

## 不在本轮范围

- 没做"自动总结前 N 条变 1 句 system memo"：LLM-side summarization 要新增
  prompt + token 消耗 + 失败兜底；本轮只做"切除"路径
- 没做"撤销"按钮：trim 后 session 文件已原地写；UX 选项是 popover 底部
  提醒先 📦 导出。后续若用户高频反悔，可加 in-memory 一层 undo stack
- 没改 messagesRef 的 tool 对配对算法：tool_call_id 配对断了 LLM 通常容
  错；保留尾部 N 条可能让 tool response 没头部 assistant tool_calls，模
  型多数会跳过这种孤儿（无 fatal 错误）

## TODO 池

清空后按规则 #1 自主提出 5 条新需求。

## TODO 池新提案

1. PanelChat 压缩前自动备份当前 session 到剪贴板
2. ChatMini ⌘L 焦点到输入框
3. PanelTasks ⌘N 全屏 quick-add 模态
4. PanelMemory 类目折叠状态 localStorage 持久化
5. PanelDebug stats 一键导出 markdown
