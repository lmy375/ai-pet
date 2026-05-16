# PanelChat session 下拉 row hover 1s 浮 "最近 3 条" preview

## 背景

PanelChat session 下拉里展示 sessionList 列表（title + meta），但要看"这个 session 最后聊了什么"必须 click 切过去 + 等加载 + scroll 到底。owner 跨 session 选择时（尤其 30+ 会话）成本高。

加 hover 1s 后 lazy load + inline 显"最近 3 条 (role glyph + 文字片段)" preview，让 owner 不必 click 即可瞄一眼判断。

## 改动

### `src/components/panel/PanelChat.tsx`

#### 1. State + cache + handlers

```ts
const [previewSessionId, setPreviewSessionId] = useState<string | null>(null);
const previewSessionTimerRef = useRef<number | null>(null);
const [previewCache, setPreviewCache] = useState<Record<string, ChatItem[]>>({});

const handleSessionPreviewEnter = useCallback((sid: string) => {
  if (previewSessionTimerRef.current !== null) return;
  if (sid === sessionId) return;  // 当前 session 不显
  previewSessionTimerRef.current = window.setTimeout(async () => {
    previewSessionTimerRef.current = null;
    if (!previewCache[sid]) {
      try {
        const session = await invoke<Session>("load_session", { id: sid });
        const last3 = (session.items ?? []).slice(-3);
        setPreviewCache((prev) => ({ ...prev, [sid]: last3 }));
      } catch (e) {
        console.error("session preview load failed:", e);
        return;
      }
    }
    setPreviewSessionId(sid);
  }, 1000);
}, [sessionId, previewCache]);

const handleSessionPreviewLeave = useCallback(() => {
  if (previewSessionTimerRef.current !== null) {
    window.clearTimeout(previewSessionTimerRef.current);
    previewSessionTimerRef.current = null;
  }
  setPreviewSessionId(null);
}, []);

useEffect(() => {
  // 下拉关闭 → reset preview state；cache 保留
  if (!showSessionList) {
    if (previewSessionTimerRef.current !== null) {
      window.clearTimeout(previewSessionTimerRef.current);
      previewSessionTimerRef.current = null;
    }
    setPreviewSessionId(null);
  }
}, [showSessionList]);
```

#### 2. Row 加 mouseEnter/Leave + 改 flex-column 容纳 preview 段

```tsx
<div
  className="pet-session-row"
  onMouseEnter={() => handleSessionPreviewEnter(s.id)}
  onMouseLeave={handleSessionPreviewLeave}
  style={{
    display: "flex",
    flexDirection: "column",  // 改 column 让 preview 段叠在原行下
    alignItems: "stretch",
    gap: "4px",
    padding: "8px 12px",
    ...
  }}
>
  <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
    ... 既有原 row 内容 ...
  </div>
  {/* 新 preview 段 */}
  {previewSessionId === s.id && s.id !== sessionId && (
    <div style={{ marginTop: 4, padding: "4px 6px", background: muted bg, border: dashed, fontSize: 10, ... }}>
      {(() => {
        const cached = previewCache[s.id];
        if (!cached) return "加载预览中...";
        if (cached.length === 0) return "（空会话）";
        return cached.map((it, i) => {
          const glyph = it.type === "user" ? "🧑" : it.type === "assistant" ? "🐾" : "🛠" / "⚠";
          const txt = (it.content ?? "").replace(/\s+/g, " ").trim();
          const snip = txt.length > 80 ? txt.slice(0, 80) + "…" : txt;
          return <div key={i}><span>{glyph}</span><span style={ellipsis}>{snip}</span></div>;
        });
      })()}
    </div>
  )}
</div>
```

## 关键设计

- **1s timer 而非即时**：让 cursor 路过 row（滚动 / 找不同条）不触发 preview。1s 是 owner 自然停留下限。
- **lazy load + Record cache**：未命中 cache 才 invoke load_session（IO 廉价 ~10ms-50ms）。同一 session 再次 hover 命中 cache 即时显，无 IPC 重复。
- **关下拉 reset previewSessionId**：避免重新打开时残留旧预览闪现。cache 保留让重打开命中。
- **当前 session 不显 preview**：主聊天区已可见，preview 冗余 + 噪音。gate `s.id !== sessionId`。
- **role glyph 用 ChatItem.type**：ChatItem schema 用 `type: "user" | "assistant" | "tool" | "error"`（不是 `role`）。user=🧑 / assistant=🐾 / tool=🛠 / error=⚠。
- **content 直接 string**：ChatItem.content 是 string（与 ChatMini ChatMessage 多模态不同），无需 extractText。
- **flex column row 改造**：让 preview 段叠在原 row 下。原 row 内容包进新 inner div 保留水平 layout。
- **80 字符 snip**：单行容量；> 80 截 + "…"。replace(/\s+/g, " ") 把换行 / 多空格归一为单空格让单行更紧凑。
- **fontSize 10 + muted color + dashed border**：与既有 hover preview hint 风格一致（PanelMemory item / PanelTasks task row），让 owner 感"hover-revealed 二级信息"模式统一。

## 不做

- **不渲染 markdown**：纯文本 + 空白归一。预览只是"瞄一眼"，markdown 渲染成本高 + 多行打破布局。
- **不持久化 cache 到 localStorage**：cache 是 ephemeral —— session 内容变频率不高但变化时 cache 会脏。close-and-reopen panel 重新 fetch 简洁。
- **不在悬停 row 上锁住其它 row 的 preview**：previewSessionTimerRef 仅一个 timer，下一次 mouseEnter 等之前 timer 完成后才 set 新 timer；用户快速扫多 row 时不会触发多个 IPC。
- **不为 `tool` / `error` row 显额外信号**：与 user/assistant glyph 同 row 一行，type 决定 glyph 即可。
- **不写测试**：纯 UI hover + lazy fetch；视觉验证（开下拉 → hover 一个 session 行 1s → 浮 3 行 preview，再 hover 另一行 → 切换）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.15s
- 改动 ~110 行（state + 3 handlers 60 + row flex-column 改造 5 + preview JSX 50 + 注释）。既有 session row click / 右键菜单 / rename / delete / sort 等路径完全不动。

## TODO 状态

剩 1 条留池：
- butler_task edit-schedule modal 扩支 every_weekdays

## 后续

- preview 卡片 click 触发 switchSession（与外层 row click 同语义），让 owner 在 hover 看到预览后不必移到外层 row 标题区 click。
- preview 显当前 session 也接入（"你正在这条 session"对偶语义），但当前主聊天区可见冲突 —— 设计成 read-only ambient cue 而非 preview。
- preview 内 ⌘+click 复制末条消息文本（与 iter #208 ChatMini bubble ⌘+click 同源习惯）。
- IntersectionObserver 把"接近视口的 row 预先 load_session"（即时显 hover preview），但 trade IPC 量。
