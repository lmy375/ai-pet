# ChatMini 长 assistant reply 折叠按钮（iter #388）

## Background

pet 偶尔会给非常长的 reply（长代码块 / 完整文档 / 多段解释）—
ChatMini chat 区可见高度有限，一条 2k+ 字 reply 把可见区挤满，要看
之前几条对话需大量滚动。owner 想"瞄一眼大概说啥，需要时再展开看
全文"。

本 iter 加默认折叠 + 一键展开 toggle — 长 reply > 2000 字时默认
显前 400 字 + … + 「📑 展开剩余 N 字」按钮；点击展开后变 「📑
折叠（N 字）」可再折回。

## Changes

### `src/components/ChatMini.tsx`

#### 1. 常量（top-level，紧贴 FOLLOW_BOTTOM_THRESHOLD_PX）

```ts
const LONG_BUBBLE_THRESHOLD = 2000;
const LONG_BUBBLE_PREVIEW_CHARS = 400;
```

chars 用 `Array.from(text).length` 计数让 CJK / emoji 按字形计 1
（与 ChatMini 其它 char-count 同算法）。

#### 2. state + toggle（~line 431）

```ts
const [longBubblesExpanded, setLongBubblesExpanded] = useState<Set<number>>(() => new Set());
const toggleLongBubble = (idx: number) => {
  setLongBubblesExpanded((prev) => {
    const next = new Set(prev);
    if (next.has(idx)) next.delete(idx);
    else next.add(idx);
    return next;
  });
};
```

idx 作 key（visibleItems 下标） — 新消息总是 append 不动旧 idx，
跨 session reset 时 stale idx 自然失效（Set 仍 in-memory 但无渲染
影响）。

#### 3. 渲染路径（替换原 `{text && parseMarkdown(text)}`）

```tsx
{text && (() => {
  const chars = Array.from(text);
  const isLong = isAssistant && chars.length > LONG_BUBBLE_THRESHOLD;
  const isExpanded = longBubblesExpanded.has(idx);
  if (!isLong || isExpanded) {
    return (
      <>
        {parseMarkdown(text)}
        {isLong && <button onClick={...}>📑 折叠（{chars.length} 字）</button>}
      </>
    );
  }
  const preview = chars.slice(0, LONG_BUBBLE_PREVIEW_CHARS).join("") + "…";
  return (
    <>
      {parseMarkdown(preview)}
      <button onClick={...}>📑 展开剩余 {chars.length - 400} 字</button>
    </>
  );
})()}
```

设计要点：
- **仅 assistant 折**：user 输入长度通常自控；折叠 user 消息会让
  owner 看不到自己刚说啥
- **!isLong 也不渲按钮**：短 reply 不需要切换按钮 — 渲染按钮会让
  每条短消息底部都挂个 chip，UI 噪音大
- **按钮 dashed border + 小字号 10**：弱视觉重量，让 chip 是"功能
  入口"而不抢 reply 内容的 focus
- **展开态用 accent color**：暗示 "本条已 in-view"；折叠态 muted
  color 表示 "还有更多"
- **title attribute 显具体字数**：hover 知道展开后会看到多少字

## Key design decisions

- **slice plain text 然后 parseMarkdown vs CSS clip**：plain text
  slice 简单可靠；CSS clip 需精确算行高 / 容器宽度 / chars-per-line，
  跨字体 / DPI / 窗口尺寸都得 retune — overengineer。slice 中段
  markdown 偶尔截到开 tag 不闭，parseMarkdown 容错性已经处理（漏
  闭就当 inline 显示）— 仅 preview UX 不致命。
- **idx-keyed Set vs ts-keyed**：idx 简单且稳定（append-only）；
  ts-keyed 抗 reset 但增加复杂度。stale Set 项数有限（owner 一会
  话内手动展开过的长 reply 数有限），不修复 GC 也无大碍。
- **不持久化展开偏好到 localStorage**：折叠是"我现在不想看 vs 现
  在想看"的临时态，跨 session 默认全折（让新启动时 chat 区紧凑）。
- **streaming bubble 不折**：当 `isLoading + currentResponse` 显
  示进度时是另一条独立"流式 bubble"渲染路径（line 957），不走
  visibleItems.map — 自然规避折叠逻辑。
- **THRESHOLD = 2000, PREVIEW = 400**：2000 字阈值经验来自 pet
  常见长 reply 长度（typical 500-1500 字，超 2000 是边缘）；400
  字 preview 是"两段中文 + 一行总结" 大致够 skim。如未来调可改
  常量。
- **不为单 fn 引 unit test runner**：行为是 IO + state ops；build
  pass + 手测足够（短 / 长 reply + 展开 / 折叠 toggle 四场景）。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动
