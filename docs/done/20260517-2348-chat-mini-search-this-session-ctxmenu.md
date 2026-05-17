# ChatMini bubble ctx menu「🔍 在 Panel 内搜本会话」(iter #423)

## Background

ChatMini bubble 右键菜单已含「⛶ 在 Panel 中定位本条」（chatMatch
deeplink → PanelChat 滚到该消息 + 1.5s 高亮）— 单点定位。但
owner 想「搜本会话所有含此关键词的消息」时只能切到 Panel + 手敲
search + select scope=current。

本 iter 加「🔍 在 Panel 内搜本会话」ctx menu item — 用 bubble 文
本前 60 字作 keyword，通过新 deeplink 字段 `chatSearch.keyword`
推到 PanelChat 自动开 search bar + scope=current + 填 query。

与「⛶ 在 Panel 中定位本条」对偶：那个滚 1 处（单点），本入口开
搜索循环（多点 audit）。

## Changes

### `src/components/ChatMini.tsx`（ctx menu，紧贴 ⛶ 之前）

```tsx
{onOpenPanel && hasText && (
  <button
    onClick={() => {
      setCtxMenu(null);
      const keyword = text.replace(/\s+/g, " ").trim().slice(0, 60);
      if (!keyword) return;
      localStorage.setItem("pet-panel-deeplink", JSON.stringify({
        chatSearch: { keyword },
        ts: Date.now(),
      }));
      onOpenPanel();
    }}
  >
    🔍 在 Panel 内搜本会话
  </button>
)}
```

60 字 keyword cap：与 ChatMini selection toolbar 「🔍 在当前 session
找类似」按钮（handleFindSimilarInSession，30 字）类似但稍宽 —
bubble 文本通常完整句，给更多 keyword 上下文。flatten whitespace
防多行 query。

### `src/PanelApp.tsx`

#### 1. pendingChatSearch state

```ts
const [pendingChatSearch, setPendingChatSearch] = useState<string | null>(null);
```

与既有 pendingChatMatch / pendingChatPrefill 同模板。

#### 2. consumePanelDeeplink 加 chatSearch 解析

```ts
const ps = parsed as { chatSearch?: unknown };
if (ps.chatSearch && typeof ps.chatSearch === "object"
    && typeof (ps.chatSearch as { keyword?: unknown }).keyword === "string") {
  const keyword = (ps.chatSearch as { keyword: string }).keyword.trim();
  if (keyword) {
    if (typeof p.tab !== "string") setActiveTab("聊天");
    setPendingChatSearch(keyword);
  }
}
```

紧贴 chatMatch 解析之后；隐含切聊天 tab（与 chatMatch 一致）；
与既有 deeplink 字段并存（caller 二选一即可）。

#### 3. PanelChat prop 串接

```tsx
<PanelChat
  ...
  pendingChatSearch={pendingChatSearch}
  onConsumePendingChatSearch={() => setPendingChatSearch(null)}
/>
```

### `src/components/panel/PanelChat.tsx`

#### 1. Props

```ts
pendingChatSearch?: string | null;
onConsumePendingChatSearch?: () => void;
```

#### 2. 消费 effect（紧贴 pendingChatMatch 之后）

```ts
useEffect(() => {
  if (!pendingChatSearch) return;
  setSearchMode(true);
  setSearchScope("current");
  setSearchQuery(pendingChatSearch);
  onConsumePendingChatSearch?.();
}, [pendingChatSearch]);
```

reuse 既有 setSearchMode / setSearchScope / setSearchQuery state
setters — 与 handleFindSimilarInSession（line 540）同 channel；
不引第二条 search 启动路径。

## Key design decisions

- **新 deeplink 字段而非复用 chatMatch**：两者语义不同（match =
  单点滚动，search = 多点 audit）；合一字段后 PanelChat 端要靠 flag
  区分动作，更复杂。新字段并存让 caller 二选一明确意图
- **deeplink TTL 10s 复用既有守门**：PanelApp 既有 ts 验证 cover
  新字段，不重复实现
- **隐含切聊天 tab**：与 chatMatch 一致 — owner 写 chatSearch 显然
  期望落在聊天视图
- **60 字 cap 而非 30**：与 selection toolbar 30 字不同因 bubble
  整句通常更长；60 仍小于 PanelChat search bar 实际能装的字符数 +
  防过长 keyword 命中过少
- **不为 deeplink 加 unit test**：纯字符串 + state setter；build
  pass + 手测足够（右键 bubble → 看「🔍 在 Panel 内搜本会话」item
  → click → Panel 自动切聊天 tab + search bar 开 + scope=current +
  query 填入 → 看高亮命中清单）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动 — 纯 localStorage deeplink + 既有 setSearchMode pipeline
