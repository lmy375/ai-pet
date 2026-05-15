# PanelChat 跨会话搜索框 history datalist

## 背景

PanelChat 顶部的跨会话搜索（搜本会话 / 全部会话）只有 Esc 退出 + live filter，**没有 history datalist**。刚抽出的 `useSearchHistory` hook 是第 3 个用得着的地方。

加 datalist 让用户反复搜同一关键词时（如周报里反复搜 `#weekly`）能快速重选。

## 改动

`src/components/panel/PanelChat.tsx`：

- import `useSearchHistory`
- 加 hook 实例：`const { history: chatSearchHistory, push: pushChatSearchHistory } = useSearchHistory("pet-chat-search-history");`
- 搜索 input 加 `list="pet-chat-search-history"`
- onKeyDown 加 Enter 分支：`if (e.key === "Enter" && searchQuery.trim()) pushChatSearchHistory(searchQuery)`
- 渲染 `<datalist>`（仅 history 非空时）

storage key 与 PanelMemory `pet-memory-search-history` / PanelTasks `pet-tasks-search-history` 各自独立 —— search 上下文不同，不该串。

## 不做

- 不动 searchScope（本会话 / 全部）逻辑：history datalist 与 scope 无关
- 不动 Esc-清行为：已经在那里

## 验收

- `npx tsc --noEmit` ✅
- 切到 search mode → 敲 query → Enter → datalist 多一项
- 再次进入 search mode → 输入框 native dropdown 浮 history

## 完成

- [x] PanelChat.tsx: useSearchHistory hook + datalist + onKeyDown Enter
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
