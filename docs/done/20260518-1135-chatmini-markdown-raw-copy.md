# ChatMini bubble 右键「📋 markdown 原文复制」（iter #470）

## Background

ChatMini bubble 右键菜单已有「📋 复制本条」— 调 `extractText(m.content)`
拿 plain text。但 `extractText` 内部走 `stripMdImages()` 把
`![alt](url)` markdown image 语法**抹掉**（让纯文本剪贴板粘出去干净）。

owner 想把 pet reply 完整粘到 markdown 编辑器 / detail.md / TG /quick
让 `「title」` ref tokens / ` ```code blocks``` ` / `![](image)` 等
markdown 语法保留渲染时，当前 plain 复制丢图片语法。

本 iter 加 sibling 「📋 markdown 原文」 — 不 strip 任何 markdown 语法，
原样 raw 复制。

## Changes

### `src/components/ChatMini.tsx`

#### 1. `getRawMarkdown` inline helper（在 ctx menu IIFE 内）

```ts
const getRawMarkdown = (content: unknown): string => {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content
    .filter((p): p is { type: "text"; text: string } =>
      !!p && typeof p === "object" &&
      (p as { type?: string }).type === "text" &&
      typeof (p as { text?: unknown }).text === "string",
    )
    .map((p) => p.text)
    .join("\n");
};
const rawMarkdown = getRawMarkdown(m.content);
const hasRawMarkdown = rawMarkdown.trim().length > 0;
```

与 `extractText` 差别：
- **不** 调 `stripMdImages` — 保留 `![alt](url)` 完整 markdown image
- 同结构 `content` 处理（string 直接返；array 取 text parts join）
- type-narrow filter 让 TypeScript 知道返 `Array<{text: string}>`

#### 2. 新增按钮（紧贴「📋 复制本条」之后）

```tsx
<button
  disabled={!hasRawMarkdown}
  onClick={() => {
    setCtxMenu(null);
    if (!hasRawMarkdown) return;
    navigator.clipboard.writeText(rawMarkdown).then(() => {
      setBubbleCopyIdx(ctxMenu.idx);
      window.setTimeout(() => setBubbleCopyIdx((cur) => cur === ctxMenu.idx ? null : cur), 1500);
    }).catch((err) => console.error("markdown raw copy failed:", err));
  }}
  title={hasRawMarkdown ? `复制 ${rawMarkdown.length} 字 markdown 原文…` : "本条无可复制 markdown 原文"}
>
  📋 markdown 原文
</button>
```

- 复用既有 `setBubbleCopyIdx` 1.5s ✓ 反馈（与 「📋 复制本条」/「⌚ 含
  时间戳」/「🔗 复制 task ref」复制族同 visual）
- disabled 当 `rawMarkdown.trim()` 为空（纯 image content 数组无 text
  part 的 multimodal 消息）+ tooltip 解释
- title 显具体 char 数让 owner 一眼看「我复制的是多大块文本」

## Key design decisions

- **`getRawMarkdown` 在 ctx menu IIFE inline 而非提取到 utils**：与
  既有 `refTitlesSet` / `extractText` callsite 同 inline 风格 — 本逻
  辑只此一处用，提取 helper 收益不显。若未来另一 surface 也需 raw
  copy（如 PanelChat 同 ctx 菜单）再 refactor
- **不 fallback 到 `text` 当 rawMarkdown 空**：raw markdown 空 ≈ 消息
  是纯 image / system 类无 text；owner 期望"复制 markdown 原文"得到
  image 数据语义不清。disabled 让 owner 知道「这条没 markdown 文本」
  比 silent fallback 更直观
- **位置紧贴「📋 复制本条」**：两 emoji 都是 📋 — 视觉相邻让 owner 心
  智「plain vs markdown 两选项」自然 — label 区分（「复制本条」vs
  「markdown 原文」）。两 button 紧排序，避免 ctx menu 中间散开
- **不写 unit test**：纯字符串处理 + click 副作用；逻辑 trivial（既
  有 extractText 类型已 production 验证 — 本 helper 是其反向 minimum
  variant）。GOAL.md "meaningful tests only" 规则下不引装饰性测试
- **不持久化 "已复制 markdown 原文"vs "已复制本条" 视觉区别**：两 button
  共享同 setBubbleCopyIdx ✓ 反馈 — 复制族 succeed 视觉一致；具体哪
  种复制走 console.log + clipboard 内容验证

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.26s)
- 后端无改动 — 纯前端 UI
- 手测：pet bubble 含 markdown image（如 `![](http://...)`）→ 右键 →
  「📋 复制本条」粘出无 image 语法；右键 → 「📋 markdown 原文」粘
  出含完整 `![](url)`
