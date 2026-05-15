# 桌面输入框 ↑/↓ 回溯历史（shell 风）

## 背景

桌面宠物的底部输入框（`ChatPanel.tsx`）只支持 Enter 发送、⌘L 聚焦。每次想"再问一遍刚才那条"或"在刚发的话后面加一句"都要重打 —— 而宠物聊天里这种重复回溯非常高频（追问、改措辞、复测调试 prompt）。

terminal / shell 用户的肌肉记忆是 `↑` 召回上一条命令；同一 mental model 自然应用到这里。

## 改动

`src/components/ChatPanel.tsx`：

### 持久化历史栈

- localStorage key `pet-chat-history`，cap 20 条，dedup（同内容 push 提到最前 + 移除旧位置）
- 加载：`useState` 惰性初始化时 try-parse；invalid → 空数组
- 写入：`submit` 成功（已调 onSend）后写一次，本地状态 + localStorage 同步

### `↑` / `↓` 召回 + 移动

`handleKeyDown` 扩展：

- `↑`（无修饰）：仅当 `input === ""` 或 `historyCursor !== null && recalledValue === input`（仍在历史浏览中）时才拦截：
  - 第一次：cursor 设到 0，setInput 为最新一条历史
  - 之后：cursor += 1（clamp 到 length-1）
- `↓`（无修饰）：仅当处于历史浏览状态时拦截：
  - cursor -= 1；若变 -1 → 退出历史模式，input 清空
  - 否则 setInput 为新位置历史

非历史模式时按 ↑↓ 走 textarea 默认行为（多行光标移动）—— 不强占用户已经在编辑的体感。

### 跳出历史浏览

用户在历史 recall 后手敲编辑：textarea `onChange` 检测 `e.target.value !== recalledValueRef.current` → 清掉 cursor + recalledValue 引用。下次 ↑ 就从头开始而非继续往后翻。

### refs 而非 state 记 cursor

`historyCursor` / `recalledValue` 用 `useRef` 而非 `useState`：游标只影响下一次 keydown / change 的判断，不需要 re-render。这样 keydown 内 `setInput` 后立刻读 cursor / recalledValue 都拿到最新值，避免 controlled-component 半 render 时序问题。

## 不做

- 不加 search-as-you-type history（`Ctrl-R` 在 terminal 里）—— 用户多半只回溯 1-2 条，不值得引入 search UI
- 不在 PanelChat（面板的大聊天框）同步加：那边有完整会话列表 + ⌘K + ⌘F + 历史 prompt 列表，召回路径更丰富；pet 窗的迷你输入是缺这种简手势的主战场

## 验收

- `npx tsc --noEmit` ✅
- 发一条消息 → 输入框敲 ↑ → 拉回原文，再 ↑ → 拉回更早一条；↓ 反向
- 历史召回后改 1 字 → 再敲 ↑ 不动（已跳出历史）
- 关 / 开宠物窗口 → 历史仍在（localStorage 持久）

## 完成

- [x] ChatPanel.tsx: 历史栈 state + persist + ↑/↓ 拦截 + onChange 跳出
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
