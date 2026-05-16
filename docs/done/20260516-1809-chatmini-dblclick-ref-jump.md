# ChatMini 双击气泡「title」ref → 跨窗口跳 PanelTasks 任务行

## 背景

PanelChat 内双击「title」ref token 已经能跳到 PanelTasks（走 panelChatBits 的 `renderContentWithTaskRefs` 把 ref 包成 clickable span + onDoubleClick → `requestFocusTask`）。

桌面 ChatMini 在独立 webview 内，至今双击气泡只走 fallback `onOpenPanel()` —— owner 在桌面快速对话时看到 assistant 提"我看了任务「整理 Downloads」" 想跳过去，只能开 Panel + 手动滚到任务行。这条 ergo 缺口与 detail.md `[task:]` ref chip / PanelChat 「」ref 形成阻断。

补一个 selection-based 双击检测，让 ChatMini 双击「title」即跨窗口跳 PanelTasks。

## 改动

### 选择 selection-based 检测而非"标记 + 重渲染"

ChatMini 用 `parseMarkdown(text)` 把消息文本渲为 ReactNode 树。要在 token 上挂 onDoubleClick 得改 parseMarkdown / 加一层 splitter —— 但 parseMarkdown 是共享 util，blast radius 大（chat / memory / panel 多处共用）。

selection-based 检测 = 双击触发时读 `window.getSelection()` 起点，向左找 `「`、向右找 `」`，命中即提取 title 调 onRefDoubleClick。**无需触碰 parseMarkdown，仅在双击事件分支判定**。

### `src/components/ChatMini.tsx`

新 prop + 替换既有 bubble onDoubleClick：

```ts
onRefDoubleClick?: (title: string) => void;
```

```tsx
<div
  onDoubleClick={(e) => {
    if (onRefDoubleClick) {
      const sel = window.getSelection();
      if (sel && sel.rangeCount > 0) {
        const range = sel.getRangeAt(0);
        const node = range.startContainer;
        if (node.nodeType === Node.TEXT_NODE) {
          const text = node.textContent ?? "";
          const start = range.startOffset;
          const end = range.endOffset;
          let lb = -1;
          for (let i = start - 1; i >= 0; i--) {
            const ch = text[i];
            if (ch === "「") { lb = i; break; }
            if (ch === "」" || ch === "\n") break;
          }
          if (lb >= 0) {
            let rb = -1;
            for (let i = end; i < text.length; i++) {
              const ch = text[i];
              if (ch === "」") { rb = i; break; }
              if (ch === "「" || ch === "\n") break;
            }
            if (rb > lb) {
              const title = text.slice(lb + 1, rb).trim();
              if (title) {
                e.preventDefault();
                e.stopPropagation();
                onRefDoubleClick(title);
                return;
              }
            }
          }
        }
      }
    }
    onOpenPanel?.();  // fallback
  }}
  title={`${formatBubbleTimestamp(m.ts)}${onRefDoubleClick ? " · 双击「title」跳任务面板该卡片" : ""}${onOpenPanel ? " · 双击气泡空白处进入面板聊天（看完整历史 / 多会话切换）" : ""}`}
>
```

### `src/App.tsx` —— 跨窗口 deeplink handler

```ts
const handleMiniRefDoubleClick = useCallback((title: string) => {
  try {
    window.localStorage.setItem(
      "pet-panel-deeplink",
      JSON.stringify({ tab: "任务", taskFocusTitle: title, ts: Date.now() }),
    );
  } catch (e) {
    console.error("write pet-panel-deeplink failed:", e);
  }
  invoke("open_panel").catch(console.error);
}, []);

<ChatMini ... onRefDoubleClick={handleMiniRefDoubleClick} />
```

### `src/PanelApp.tsx` —— consumePanelDeeplink 扩字段

```ts
if (typeof p.taskFocusTitle === "string" && p.taskFocusTitle.trim()) {
  requestFocusTask(p.taskFocusTitle.trim());
}
```

`requestFocusTask` 既有：`setActiveTab("任务")` + `setPendingTaskFocusTitle(title)`，PanelTasks `pendingFocusTitle` prop → useEffect 找 idx + scrollIntoView。

## 关键设计

- **selection-based 检测无需触碰 parseMarkdown**：浏览器双击自然选中一个 word/中文片段，selection range 落在 ref token 区内时双向扫描定界符 → 提取 title。优雅、零侵入。
- **方向扫描碰另一边引号 / 换行先停**：防 selection 落在两个独立 ref token 之间（`「a」 普通文本 「b」`），从中间向外扫不会"穿透"到错的 token。换行同理 —— `「a」` 在第一行、第二行有 `「`，从第一行末双击不该串到第二行。
- **range.startContainer 必须是 text node**：emoji / 表情 span / image 双击时 startContainer 是 element，textContent 仍有值但 offset 语义不同 —— 直接跳过用 fallback。
- **fallback onOpenPanel**：未命中 ref 时保留既有"双击气泡空白处进面板聊天" UX。owner 习惯不破坏。
- **trim title**：`「 整理 Downloads 」` 这种用户手敲带前后空格的 ref 也能命中（PanelTasks setPendingTitleFocus 严格 match title，需先 trim）。
- **跨窗口走既有 deeplink 通道**：不引新 IPC / 不新加 emit 事件；与 `dueFilter` / `chatMatch` 同 schema 扩字段。PanelApp 已开则 storage 事件即时消费；未开则首次 mount 消费（TTL 10s 兜底）。
- **复用 `requestFocusTask` pipeline**：与 PanelChat 内双击 ref / PanelMemory item click / completed 小卡 click 同源。所有 jump-to-task 入口都收敛到一条路径，UX 一致。
- **不修改既有 PanelChat 路径**：PanelChat 双击通过 `onRefDoubleClick={onRequestFocusTask}` 直传，已在 same window —— 不需 deeplink。本 iter 仅给 ChatMini（独立 webview）补 cross-window 桥。

## 不做

- **不把 ChatMini 改成像 PanelChat 那样渲 ref 为 clickable span**：要侵入 parseMarkdown / 重做 ChatMini bubble 内容渲染。selection-based 已经足够 — 双击命中率高、不影响阅读。
- **不在 PanelApp 端给 taskFocusTitle 加视觉脉冲反馈**：requestFocusTask 已让 PanelTasks 滚到该行 + outline 高亮（既有 pendingTitleFocus 行为）。owner 已能看出"跳过来的目标"。
- **不显式 toast / 反馈"已跳到 task X"**：openPanel 自带视觉切换，scrollIntoView + outline 已足够确认。
- **不写测试**：纯 selection API + 字符扫描；行为通过手动验证（在桌面 chat 双击 user/assistant 气泡中的「整理 Downloads」→ 应自动开 Panel + 任务 tab + 该行高亮）确认。
- **不解析"半角 ASCII 引号"**：与 PanelChat 既有规则一致 — 仅认全角直角引号「」（与 ⌘K picker 输出格式严格匹配）。误用半角引号是用户手抖，不抢这条语义。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.35s
- 改动 ~85 行（ChatMini 新 prop + dblclick handler 60 + App.tsx callback 15 + PanelApp deeplink 字段 6 + 注释 4）。parseMarkdown / panelChatBits / PanelTasks pendingTitleFocus 既有路径完全不动；既有 ChatMini 双击进面板聊天 fallback 保留。

## TODO 状态

剩 2 条留池：
- 桌面 pet 右键菜单加「切 Live2D 模型」子菜单
- butler_task 描述新增 [reminderMin: N] 标记

## 后续

- ⌥+ 双击「title」复制 ref token 到剪贴板（不跳）—— 让 owner 想"把这条 ref 引到别处" 时少一步。
- 给 ChatMini 加 PanelChat 同款 ref hover preview（dotted underline + status / updated_at 浮卡）—— 完整对偶。需要让 ChatMini 拿到 taskRefMap，意味着加一个轻量 IPC poll；当前 ChatMini scope 故意小，下一 iter 评估值不值得。
- assistant 文本里出现的 ref token 自动注释一段 "(已 ✓ 完成)" / "(⏰ 今日 18:00)" 后缀 —— 让 owner 不双击也知道引用对象的当前状态。
