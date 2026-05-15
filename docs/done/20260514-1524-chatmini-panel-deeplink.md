# ChatMini → PanelChat 跳到本条 deeplink

## 背景

TODO 最后一项（auto-proposed 几轮之前）：

> ChatMini 消息长按 / 右键加"在 Panel 跳到这条":需要先做"按 idx 跳"的 deeplink 协议，让浮窗"在 Panel 中打开聊天"能真正定位。

20260514-1254 给 ChatMini 加了右键菜单，含「⛶ 在 Panel 中打开聊天」入口 —— 但那只是切到 Panel chat tab 不会定位到具体 bubble。"我刚才在桌面看到一句宠物说的话想回看完整上下文"得手动滚 panel 找。

PanelChat 早有跨会话搜索 `pendingScroll` + `data-item-idx` + 高亮的基础设施（20260514 之前的迭代）；deeplink 协议早有跨窗口 `pet-panel-deeplink` localStorage 通道（任务过期 pill 用）。把两者连起来即可。

## 改动

### `src/PanelApp.tsx` — 扩 deeplink 协议

deeplink payload 加一个新字段 `chatMatch?: { excerpt: string }`：

```ts
const [pendingChatMatch, setPendingChatMatch] = useState<string | null>(null);

// 在既有 consumePanelDeeplink 函数里：
if (p.chatMatch && typeof p.chatMatch === "object" &&
    typeof (p.chatMatch as { excerpt?: unknown }).excerpt === "string") {
  const excerpt = (p.chatMatch as { excerpt: string }).excerpt.trim();
  if (excerpt) {
    if (typeof p.tab !== "string") setActiveTab("聊天");  // 隐含切 tab
    setPendingChatMatch(excerpt);
  }
}
```

TTL 10s（沿用现有 dueFilter 同 guard），同一通道便于将来更多 deeplink 字段叠加。

PanelChat 收到 prop `pendingChatMatch` + `onConsumePendingChatMatch`，与现有 `pendingDueFilter / onConsumePendingDueFilter`（PanelTasks 用）同模式。

### `src/components/panel/PanelChat.tsx` — 反向扫 + scroll

```ts
interface PanelChatProps {
  pendingChatMatch?: string | null;
  onConsumePendingChatMatch?: () => void;
  // ...其它已有
}

useEffect(() => {
  if (!pendingChatMatch) return;
  if (items.length === 0) return; // 等 loadSession 落 items 再消费
  const needle = pendingChatMatch.toLowerCase();
  let foundIdx = -1;
  for (let i = items.length - 1; i >= 0; i--) {
    const it = items[i];
    if (it.type !== "user" && it.type !== "assistant") continue;
    if (typeof it.content !== "string") continue;
    if (it.content.toLowerCase().includes(needle)) { foundIdx = i; break; }
  }
  if (foundIdx >= 0) {
    setPendingScroll(foundIdx);
  } else {
    pushLocalAssistantNote(
      `⛶ 没在本会话找到含「${pendingChatMatch.slice(0, 20)}${pendingChatMatch.length > 20 ? "…" : ""}」的消息（可能在别的 session）。`,
    );
  }
  onConsumePendingChatMatch?.();
}, [pendingChatMatch, items.length]);
```

走既有 `setPendingScroll(idx)` 路径 → `scrollIntoView({ block: "center", behavior: "smooth" }) + setHighlightedItemIdx + 1.5s 清` 完整复用，无新视觉路径。

**为什么反向扫**：消息内容可能在 session 历史里有重复（特别是宠物 "好的 / 收到" 等高频短语），最近一条命中是用户最可能想看的；与跨会话搜索的"按 updated_at desc 排"理念一致。

**为什么 substring + lowercase**：与既有 `/search` 跨会话搜索一致；excerpt 取前 80 字符已足够独特。

**找不到的 fallback**：发一条 systemNote 解释为什么没跳（用户在 panel 里能看到反馈而非沉默无响应）。

### `src/components/ChatMini.tsx` — 右键菜单加新条目

```tsx
{onOpenPanel && hasText && (
  <button onClick={() => {
    setCtxMenu(null);
    const excerpt = Array.from(text).slice(0, 80).join("");
    try {
      window.localStorage.setItem(
        "pet-panel-deeplink",
        JSON.stringify({ chatMatch: { excerpt }, ts: Date.now() }),
      );
    } catch { /* fallback to just opening panel */ }
    onOpenPanel();
  }}>
    ⛶ 在 Panel 中定位本条
  </button>
)}
```

**条件 `hasText`**：纯图片 bubble 没文字给 substring 匹配，所以不渲染本菜单项。

**为什么 80 字符**：长到独特命中、短到不挤 localStorage。`Array.from(text).slice(0,80).join("")` 走 Unicode code point 切片避免 surrogate pair / 多字节切错。

**localStorage 失败兜底**：仍 `onOpenPanel()`——用户至少能进 panel chat tab，比"什么都没发生"好。

## 不做

- **不用 backend IPC 跨窗口传 deeplink**。localStorage 同模式与既有 dueFilter / 任务焦点 deeplink 路径一致，无新依赖；webview process 间的 storage event 通道已被验证。
- **不做"精确 idx 跳"**。ChatMini 的 `messages` 数组与 PanelChat 的 `items` 数组形态不同（ChatMini 是 useChat 内 messages 含 system / tool；PanelChat 是 session.items 含 user/assistant/tool/error）。直接传 idx 需要双向 schema 对齐 + 时序保证（消息加载完才 valid）。substring 反向扫规避了所有这些问题，找不到走 fallback note 即可。
- **不跨 session 搜索**。仅当前 active session 内匹配；跨 session 走 `/search` 命令更直接。
- **不写测试**。前端无 vitest；逻辑是 useEffect + reverse-scan + early-return，与既有 pendingScroll 路径同模式。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.15s
- 改动 ~80 行（PanelApp deeplink ext 25 + PanelChat props + effect 35 + ChatMini menu item 25）；既有 deeplink / pendingScroll / 右键菜单其它 entries / pushLocalAssistantNote 路径全部不动。

## TODO 状态

empty —— 下次进入 auto-propose 分支。

## 后续

- PanelChat 命中 bubble 高亮颜色从 1.5s 灰 → 2.0s 蓝（让 deeplink 跳到的命中视觉与 search hit 区分）。
- 多个 chatMatch 命中时显"还有 N 条更早的"提示让用户能继续找。
- ChatMini 双击 bubble 改为弹"打开 / 定位" 两选 menu，而不是默认 onOpenPanel（让定位成为更可发现的 action）。
