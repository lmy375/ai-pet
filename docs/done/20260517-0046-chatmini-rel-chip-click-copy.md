# ChatMini bubble 底 ⏱ 相对时间 chip click 复制相对时间字符串

## 背景

iter #223 给 ChatMini bubble 顶 ts chip 加了 click 复制 ISO timestamp。底 ⏱ "N 分前 / 昨天 / N 天前" 相对时间 chip 是对偶视觉信号，自然也应该可 click 复制 —— owner 想 "我 5 分钟前问过的那条..." 时直接 copy "5 分前" 到 chat / 报告。

## 改动

### `src/components/ChatMini.tsx`

#### 1. 新 `relCopyIdx` state（同 tsCopyIdx 模板）

```ts
const [relCopyIdx, setRelCopyIdx] = useState<number | null>(null);
```

#### 2. 底 ⏱ chip 加 click handler + ✓ 视觉态

```tsx
<span
  className="pet-mini-row-rel"
  onClick={(e) => {
    e.stopPropagation();  // 防 bubble ⌘+click / dblclick ref 误触
    navigator.clipboard.writeText(rel).then(() => {
      setRelCopyIdx(idx);
      window.setTimeout(() => setRelCopyIdx((cur) => cur === idx ? null : cur), 1500);
    }).catch((err) => console.error("rel chip copy failed:", err));
  }}
  style={{
    ...原 style + cursor: "pointer",
    color: isRelCopied ? "tint-green-fg" : "muted",
    fontWeight: isRelCopied ? 600 : undefined,
  }}
  title={`相对时间 · ${formatFullTimestamp(m.ts)} · 点击复制 "${rel}"`}
>
  {isRelCopied ? "✓ " : ""}⏱ {rel}
</span>
```

完全镜像顶 ts chip click pattern（iter #223）。

## 关键设计

- **完全对偶顶 ts chip**：iter #223 顶 chip 复制 ISO `"2026-05-17T00:46:18+08:00"`；底 chip 复制 relative `"5 分前"`。owner 按需选择粒度。
- **e.stopPropagation 防 bubble click 路径误触**：与顶 chip 同处理 —— iter #208 加了 ⌘+click bubble 复制；iter #189 加了 dblclick 跳 ref。本 chip 自己有 click，避免冒泡。
- **1.5s ✓ 反馈 + tint-green-fg**：与既有 bubbleCopyIdx / tsCopyIdx 同 timeout / 同 color，UX 一致。
- **title attr 含 rel 字面 + 完整 ISO**：让 owner hover 即可看到要复制什么。

## 不做

- **不同时复制 ISO + relative**：owner 期望单一可预测剪贴板内容；分开两 chip 给两选项最清晰。
- **不绑 ⌘+click 复制全文**：与 bubble ⌘+click 复制 (iter #208) 重叠；底 chip 单 click 已用作复制 rel 串。
- **不写测试**：纯 click handler + navigator.clipboard.writeText；视觉验证（hover 底 chip → click → 短暂 ✓ + 剪贴板含 "N 分前"）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.19s
- 改动 ~40 行（relCopyIdx state 3 + chip IIFE 内加 click handler + 视觉态 30 + 注释）。既有底 chip CSS hover-reveal / formatBubbleRelative / 顶 ts chip click / bubble ⌘+click 路径完全不动。

## TODO 状态

剩 5 条留池：
- detail.md 编辑器 toolbar "📑 复制大纲"
- PanelSettings 顶 search input
- PanelMemory "今天新增" chip drill-down
- PanelTasks 拖行改 priority toast 反馈
- PanelChat session 右键菜单加「📌 钉住会话」

## 后续

- ⌘+click 底 chip 复制 ISO（与顶 chip 复制 ISO 等价），让 owner 不必区分 chip 时也能拿到精确串。
- ChatPanel 消息行底也加同款 ⏱ chip（与 ChatMini bubble 视觉对偶），让 panel 看历史也能一键 copy。
